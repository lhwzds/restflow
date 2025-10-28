use crate::engine::context::{ExecutionContext, namespace};
use crate::engine::graph::WorkflowGraph;
use crate::models::{Node, Task, TaskStatus, Workflow};
use crate::storage::{Storage, TaskQueue};
use anyhow::Result;
use chrono::Utc;
use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

// Tasks processing longer than this threshold are considered stalled and will be reset to pending
const DEFAULT_STALL_TIMEOUT_SECONDS: i64 = 300; // 5 minutes

pub struct Scheduler {
    queue: TaskQueue,
    storage: Arc<Storage>,
}

impl Scheduler {
    pub fn new(queue: TaskQueue, storage: Arc<Storage>) -> Self {
        Self { queue, storage }
    }

    /// Accepts `Arc<Workflow>` to avoid expensive cloning in downstream task queueing
    pub fn push_task(
        &self,
        execution_id: String,
        node: Node,
        workflow: Arc<Workflow>,
        context: ExecutionContext,
    ) -> Result<String> {
        // Parse node config as NodeInput (uses serde's tagged enum for O(1) type dispatch)
        let node_input: crate::models::NodeInput = serde_json::from_value(node.config.clone())
            .map_err(|e| anyhow::anyhow!("Failed to parse node config as NodeInput: {}", e))?;

        let task = Task::new(
            execution_id,
            workflow.id.clone(),
            node.id.clone(),
            node_input,
            context,
        );
        let task_id = task.id.clone();

        let _ = task.set_workflow(workflow);

        self.storage.execution_history.record_task_created(
            &task.workflow_id,
            &task.execution_id,
            task.created_at,
        )?;

        let priority = task.priority();
        let serialized = serde_json::to_vec(&task)?;
        self.queue.insert_pending(priority, &task_id, &serialized)?;

        Ok(task_id)
    }

    pub fn push_single_node(&self, node: Node, _input: Value) -> Result<String> {
        // For single-node execution, parse NodeInput from node.config
        // The node.config already contains the tagged union {"type": "...", "data": {...}}
        let node_input: crate::models::NodeInput = serde_json::from_value(node.config.clone())
            .map_err(|e| anyhow::anyhow!("Failed to parse node config as NodeInput: {}", e))?;

        let task = Task::for_single_node(node, node_input);
        let task_id = task.id.clone();

        self.storage.execution_history.record_task_created(
            &task.workflow_id,
            &task.execution_id,
            task.created_at,
        )?;

        let priority = task.priority();
        let serialized = serde_json::to_vec(&task)?;
        self.queue.insert_pending(priority, &task_id, &serialized)?;

        Ok(task_id)
    }

    pub fn submit_workflow(&self, workflow: Workflow, input: Value) -> Result<String> {
        let execution_id = Uuid::new_v4().to_string();
        self.submit_workflow_internal(workflow, input, execution_id)
    }

    fn submit_workflow_internal(
        &self,
        workflow: Workflow,
        input: Value,
        execution_id: String,
    ) -> Result<String> {
        let workflow = Arc::new(workflow);

        let mut context =
            ExecutionContext::with_execution_id(workflow.id.clone(), execution_id.clone());
        context.ensure_secret_storage(&self.storage);
        context.set(namespace::trigger::PAYLOAD, input);

        let graph = WorkflowGraph::from_workflow(&workflow);
        let start_nodes = graph.get_nodes_with_no_dependencies();

        if start_nodes.is_empty() {
            return Err(anyhow::anyhow!(
                "No start nodes found in workflow {}",
                workflow.id
            ));
        }

        for node_id in start_nodes {
            if let Some(node) = graph.get_node(&node_id) {
                self.push_task(
                    execution_id.clone(),
                    node.clone(),
                    workflow.clone(),
                    context.clone(),
                )?;
            }
        }

        Ok(execution_id)
    }

    /// Submit a workflow by ID for execution
    pub fn submit_workflow_by_id(&self, workflow_id: &str, input: Value) -> Result<String> {
        let workflow = self
            .storage
            .workflows
            .get_workflow(workflow_id)
            .map_err(|e| anyhow::anyhow!("Failed to load workflow {}: {}", workflow_id, e))?;

        self.submit_workflow(workflow, input)
    }

    pub fn submit_workflow_by_id_with_execution_id(
        &self,
        workflow_id: &str,
        input: Value,
        execution_id: String,
    ) -> Result<String> {
        let workflow = self
            .storage
            .workflows
            .get_workflow(workflow_id)
            .map_err(|e| anyhow::anyhow!("Failed to load workflow {}: {}", workflow_id, e))?;

        self.submit_workflow_internal(workflow, input, execution_id)
    }

    pub async fn pop_task(&self) -> Result<Task> {
        loop {
            match self.try_pop_task()? {
                Some(task) => return Ok(task),
                None => {
                    self.queue.wait_for_task().await;
                }
            }
        }
    }

    /// Uses atomic_pop_pending with callback to ensure atomicity
    fn try_pop_task(&self) -> Result<Option<Task>> {
        self.queue.atomic_pop_pending(|task| task.start())
    }

    pub fn complete_task(&self, task_id: &str, output: crate::models::NodeOutput) -> Result<()> {
        self.finish_task(task_id, TaskStatus::Completed, Some(output), None)
    }

    pub fn fail_task(&self, task_id: &str, error: String) -> Result<()> {
        self.finish_task(task_id, TaskStatus::Failed, None, Some(error))
    }

    fn finish_task(
        &self,
        task_id: &str,
        status: TaskStatus,
        output: Option<crate::models::NodeOutput>,
        error: Option<String>,
    ) -> Result<()> {
        if let Some(data) = self.queue.get_from_processing(task_id)? {
            let mut task: Task = serde_json::from_slice(&data)?;

            match status {
                TaskStatus::Completed => {
                    if let Some(output) = output {
                        task.complete(output);
                    }
                }
                TaskStatus::Failed => {
                    if let Some(error) = error {
                        task.fail(error);
                    }
                }
                _ => {}
            }

            let serialized = serde_json::to_vec(&task)?;
            self.queue.move_to_completed(task_id, &serialized)?;

            let timestamp_ms = Utc::now().timestamp_millis();
            match status {
                TaskStatus::Completed => {
                    self.storage.execution_history.record_task_completed(
                        &task.workflow_id,
                        &task.execution_id,
                        timestamp_ms,
                    )?;
                }
                TaskStatus::Failed => {
                    self.storage.execution_history.record_task_failed(
                        &task.workflow_id,
                        &task.execution_id,
                        timestamp_ms,
                    )?;
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn query_all_tasks<F>(&self, filter: F) -> Result<Vec<Task>>
    where
        F: Fn(&Task) -> bool,
    {
        let mut tasks = Vec::new();

        for data in self.queue.get_all_pending()? {
            let task: Task = serde_json::from_slice(&data)?;
            if filter(&task) {
                tasks.push(task);
            }
        }

        for data in self.queue.get_all_processing()? {
            let task: Task = serde_json::from_slice(&data)?;
            if filter(&task) {
                tasks.push(task);
            }
        }

        for data in self.queue.get_all_completed()? {
            let task: Task = serde_json::from_slice(&data)?;
            if filter(&task) {
                tasks.push(task);
            }
        }

        Ok(tasks)
    }

    pub fn get_tasks_by_execution(&self, execution_id: &str) -> Result<Vec<Task>> {
        let mut tasks = self.query_all_tasks(|task| task.execution_id == execution_id)?;

        tasks.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(tasks)
    }

    pub fn get_task(&self, task_id: &str) -> Result<Option<Task>> {
        if let Some(data) = self.queue.get_from_any_table(task_id)? {
            let task: Task = serde_json::from_slice(&data)?;
            Ok(Some(task))
        } else {
            Ok(None)
        }
    }

    pub fn list_tasks(
        &self,
        workflow_id: Option<&str>,
        status: Option<TaskStatus>,
    ) -> Result<Vec<Task>> {
        let mut tasks = self.query_all_tasks(|task| {
            workflow_id.is_none_or(|id| task.workflow_id == id)
                && status.as_ref().is_none_or(|s| &task.status == s)
        })?;

        tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(tasks)
    }

    pub fn recover_stalled_tasks(&self) -> Result<u32> {
        let mut recovered = 0;
        let now = chrono::Utc::now().timestamp_millis();

        for data in self.queue.get_all_processing()? {
            let mut task: Task = serde_json::from_slice(&data)?;

            if let Some(started_at) = task.started_at {
                let stall_threshold_ms = DEFAULT_STALL_TIMEOUT_SECONDS * 1000;
                if now - started_at > stall_threshold_ms {
                    task.status = TaskStatus::Pending;
                    task.started_at = None;

                    let task_id = task.id.clone();
                    let priority = task.priority();
                    let serialized = serde_json::to_vec(&task)?;

                    self.queue.remove_from_processing(&task_id)?;
                    self.queue.insert_pending(priority, &task_id, &serialized)?;

                    recovered += 1;
                }
            }
        }

        Ok(recovered)
    }

    pub fn are_dependencies_met(
        graph: &WorkflowGraph,
        node_id: &str,
        context: &ExecutionContext,
    ) -> bool {
        graph
            .get_dependencies(node_id)
            .iter()
            .all(|dep| context.get_node(dep).is_some())
    }

    /// Uses `Arc<Workflow>` to avoid expensive cloning in large workflows
    pub fn push_downstream_tasks(&self, task: &Task, output: crate::models::NodeOutput) -> Result<()> {
        let workflow = task.get_workflow(&self.storage)?;

        // Serialize NodeOutput to Value for context storage
        let output_value = serde_json::to_value(&output)?;

        let mut context = task.context.clone();
        context.set_node(&task.node_id, output_value);

        let graph = WorkflowGraph::from_workflow(&workflow);
        let downstream_nodes = graph.get_downstream_nodes(&task.node_id);

        for downstream_id in downstream_nodes {
            if let Some(downstream_node) = graph.get_node(&downstream_id)
                && Self::are_dependencies_met(&graph, &downstream_id, &context)
            {
                self.push_task(
                    task.execution_id.clone(),
                    downstream_node.clone(),
                    workflow.clone(),
                    context.clone(),
                )?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::context::ExecutionContext;
    use crate::models::{Task, TaskStatus};
    use crate::storage::Storage;
    use tempfile::tempdir;

    fn setup_test_scheduler() -> (Scheduler, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let storage = Arc::new(Storage::new(db_path.to_str().unwrap()).unwrap());
        let scheduler = Scheduler::new(storage.queue.clone(), storage.clone());
        (scheduler, temp_dir)
    }

    fn create_test_input() -> crate::models::NodeInput {
        use crate::models::{NodeInput, ManualTriggerInput};

        NodeInput::ManualTrigger(ManualTriggerInput {
            payload: Some(serde_json::json!({})),
        })
    }

    #[test]
    fn test_recover_stalled_tasks() {
        let (scheduler, _temp_dir) = setup_test_scheduler();

        // Create a task with started_at 10 minutes ago (should be recovered)
        let ten_minutes_ago = chrono::Utc::now().timestamp_millis() - (10 * 60 * 1000);
        let mut task = Task::new(
            "exec-1".to_string(),
            "wf-1".to_string(),
            "node-1".to_string(),
            create_test_input(),
            ExecutionContext::new("exec-1".to_string()),
        );
        task.status = TaskStatus::Running;
        task.started_at = Some(ten_minutes_ago);

        // Put task in processing
        let serialized = serde_json::to_vec(&task).unwrap();
        scheduler
            .queue
            .move_to_processing(0, &task.id, &serialized)
            .unwrap();

        // Recover stalled tasks
        let recovered = scheduler.recover_stalled_tasks().unwrap();
        assert_eq!(recovered, 1, "Should recover 1 stalled task");

        // Verify task is back in pending
        let pending_tasks = scheduler.queue.get_all_pending().unwrap();
        assert_eq!(pending_tasks.len(), 1, "Should have 1 pending task");

        // Verify task is no longer in processing
        let processing_tasks = scheduler.queue.get_all_processing().unwrap();
        assert_eq!(processing_tasks.len(), 0, "Should have 0 processing tasks");
    }

    #[test]
    fn test_get_pending_task() {
        let (scheduler, _temp_dir) = setup_test_scheduler();

        // Create a task
        let task = Task::new(
            "exec-1".to_string(),
            "wf-1".to_string(),
            "node-1".to_string(),
            create_test_input(),
            ExecutionContext::new("exec-1".to_string()),
        );
        let task_id = task.id.clone();

        // Push to queue
        let priority = task.priority();
        let serialized = serde_json::to_vec(&task).unwrap();
        scheduler
            .queue
            .insert_pending(priority, &task_id, &serialized)
            .unwrap();

        // Get task should find it in pending
        let found = scheduler.get_task(&task_id).unwrap();
        assert!(found.is_some(), "Should find task in pending");
        assert_eq!(found.unwrap().id, task_id);
    }

    #[test]
    fn test_submit_workflow() {
        use crate::models::{Node, NodeType};

        let (scheduler, _temp_dir) = setup_test_scheduler();

        // Create a simple workflow with one node
        let node = Node {
            id: "start_node".to_string(),
            node_type: NodeType::Agent,
            config: serde_json::json!({
                "type": "Agent",
                "data": {
                    "model": "gpt-4",
                    "prompt": "test prompt",
                    "temperature": null,
                    "api_key_config": null,
                    "tools": null
                }
            }),
            position: None,
        };

        let workflow = Workflow {
            id: "test-workflow".to_string(),
            name: "Test Workflow".to_string(),
            nodes: vec![node],
            edges: vec![],
        };

        // Submit workflow
        let input = serde_json::json!({"test": "data"});
        let execution_id = scheduler.submit_workflow(workflow, input).unwrap();

        // Verify execution_id is valid UUID format
        assert!(!execution_id.is_empty(), "Execution ID should not be empty");

        // Verify task was queued
        let pending_tasks = scheduler.queue.get_all_pending().unwrap();
        assert_eq!(pending_tasks.len(), 1, "Should have 1 pending task");

        // Verify task has correct execution_id
        let task: Task = serde_json::from_slice(&pending_tasks[0]).unwrap();
        assert_eq!(task.execution_id, execution_id);
        assert_eq!(task.node_id, "start_node");
    }
}
