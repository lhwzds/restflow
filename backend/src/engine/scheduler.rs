use crate::models::{Node, Workflow, TaskRecord, TaskStatus, WorkflowTask};
use crate::engine::context::ExecutionContext;
use crate::engine::graph::WorkflowGraph;
use crate::storage::{Storage, TaskQueue};
use anyhow::Result;
use serde_json::Value;
use std::sync::Arc;

const DEFAULT_STALL_TIMEOUT_SECONDS: i64 = 300; // 5 minutes

pub struct Scheduler {
    queue: TaskQueue,
    storage: Arc<Storage>,
}

impl Scheduler {
    pub fn new(queue: TaskQueue, storage: Arc<Storage>) -> Self {
        Self { queue, storage }
    }

    /// Add a node task to the queue
    pub fn push_task(
        &self,
        execution_id: String,
        node: Node,
        workflow: Workflow,
        context: ExecutionContext,
        input: Value,
    ) -> Result<String> {
        // Serialize context for storage
        let context_data = serde_json::to_vec(&context)?;
        
        // Create lightweight TaskRecord
        let record = TaskRecord::new(
            execution_id,
            workflow.id.clone(),
            node.id.clone(),
            input,
            context_data,
        );
        let task_id = record.id.clone();

        let priority = chrono::Utc::now().timestamp_millis() as u64;
        let serialized = serde_json::to_vec(&record)?;
        self.queue.insert_pending(priority, &serialized)?;

        Ok(task_id)
    }

    /// Add a single node task for standalone execution
    pub fn push_single_node(&self, node: Node, input: Value) -> Result<String> {
        // Create a minimal workflow for single node
        let workflow = Workflow {
            id: format!("single-{}", node.id),
            name: format!("Single Node: {}", node.id),
            nodes: vec![node.clone()],
            edges: vec![],
        };
        
        // Store the workflow first
        self.storage.workflows.create_workflow(&workflow)
            .map_err(|e| anyhow::anyhow!("Failed to create workflow: {}", e))?;
        
        let execution_id = uuid::Uuid::new_v4().to_string();
        let context = ExecutionContext::new(execution_id.clone());
        let context_data = serde_json::to_vec(&context)?;
        
        let record = TaskRecord::new(
            execution_id,
            workflow.id.clone(),
            node.id.clone(),
            input,
            context_data,
        );
        let task_id = record.id.clone();

        let priority = chrono::Utc::now().timestamp_millis() as u64;
        let serialized = serde_json::to_vec(&record)?;
        self.queue.insert_pending(priority, &serialized)?;

        Ok(task_id)
    }

    /// Pop a task from the queue (blocks until task available)
    pub async fn pop_task(&self) -> Result<WorkflowTask> {
        loop {
            match self.try_pop_task()? {
                Some(task) => return Ok(task),
                None => {
                    // Wait for notification when queue is empty
                    self.queue.wait_for_task().await;
                }
            }
        }
    }

    /// Try to pop a task without blocking
    fn try_pop_task(&self) -> Result<Option<WorkflowTask>> {
        // Get first pending task
        if let Some((priority, data)) = self.queue.get_first_pending()? {
            let mut record: TaskRecord = serde_json::from_slice(&data)?;
            
            // Load workflow from storage
            let workflow = self.storage.workflows.get_workflow(&record.workflow_id)
                .map_err(|e| anyhow::anyhow!("Failed to get workflow: {}", e))?
                .ok_or_else(|| anyhow::anyhow!("Workflow {} not found", record.workflow_id))?;
            
            // Deserialize context
            let context: ExecutionContext = serde_json::from_slice(&record.context_data)?;
            
            // Update task status
            record.status = TaskStatus::Running;
            record.started_at = Some(chrono::Utc::now().timestamp());
            
            // Create runtime WorkflowTask
            let task = WorkflowTask::from_record(record.clone(), workflow, context)
                .map_err(|e| anyhow::anyhow!(e))?;
            
            // Move to processing
            let serialized = serde_json::to_vec(&record)?;
            self.queue.move_to_processing(priority, &record.id, &serialized)?;
            
            Ok(Some(task))
        } else {
            Ok(None)
        }
    }

    /// Mark a task as completed with output
    pub fn complete_task(&self, task_id: &str, output: Value) -> Result<()> {
        self.finish_task(task_id, TaskStatus::Completed, Some(output), None)
    }

    /// Mark a task as failed with error message
    pub fn fail_task(&self, task_id: &str, error: String) -> Result<()> {
        self.finish_task(task_id, TaskStatus::Failed, None, Some(error))
    }

    /// Internal helper to finish a task
    fn finish_task(
        &self,
        task_id: &str,
        status: TaskStatus,
        output: Option<Value>,
        error: Option<String>,
    ) -> Result<()> {
        // Get task record from processing
        if let Some(data) = self.queue.get_from_processing(task_id)? {
            let mut record: TaskRecord = serde_json::from_slice(&data)?;
            record.status = status;
            record.completed_at = Some(chrono::Utc::now().timestamp());
            record.output = output;
            record.error = error;
            
            // Move to completed (lightweight record without workflow)
            let serialized = serde_json::to_vec(&record)?;
            self.queue.move_to_completed(task_id, &serialized)?;
        }
        
        Ok(())
    }

    /// Get task records by execution ID from all tables
    pub fn get_tasks_by_execution(&self, execution_id: &str) -> Result<Vec<TaskRecord>> {
        let mut tasks = Vec::new();
        
        // Check all three tables
        for data in self.queue.get_all_pending()? {
            let record: TaskRecord = serde_json::from_slice(&data)?;
            if record.execution_id == execution_id {
                tasks.push(record);
            }
        }
        
        for data in self.queue.get_all_processing()? {
            let record: TaskRecord = serde_json::from_slice(&data)?;
            if record.execution_id == execution_id {
                tasks.push(record);
            }
        }
        
        for data in self.queue.get_all_completed()? {
            let record: TaskRecord = serde_json::from_slice(&data)?;
            if record.execution_id == execution_id {
                tasks.push(record);
            }
        }
        
        // Sort by creation time
        tasks.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(tasks)
    }

    /// Get a task record by ID from any table
    pub fn get_task(&self, task_id: &str) -> Result<Option<TaskRecord>> {
        if let Some(data) = self.queue.get_from_any_table(task_id)? {
            let record: TaskRecord = serde_json::from_slice(&data)?;
            Ok(Some(record))
        } else {
            Ok(None)
        }
    }

    /// List task records with optional filters
    pub fn list_tasks(&self, workflow_id: Option<&str>, status: Option<TaskStatus>) -> Result<Vec<TaskRecord>> {
        let mut tasks = Vec::new();
        
        // Check pending tasks
        if status.is_none() || status == Some(TaskStatus::Pending) {
            for data in self.queue.get_all_pending()? {
                let record: TaskRecord = serde_json::from_slice(&data)?;
                if Self::matches_record_filter(&record, workflow_id, None) {
                    tasks.push(record);
                }
            }
        }
        
        // Check running tasks
        if status.is_none() || status == Some(TaskStatus::Running) {
            for data in self.queue.get_all_processing()? {
                let record: TaskRecord = serde_json::from_slice(&data)?;
                if Self::matches_record_filter(&record, workflow_id, None) {
                    tasks.push(record);
                }
            }
        }
        
        // Check completed tasks
        if status.is_none() || matches!(status, Some(TaskStatus::Completed) | Some(TaskStatus::Failed)) {
            for data in self.queue.get_all_completed()? {
                let record: TaskRecord = serde_json::from_slice(&data)?;
                if Self::matches_record_filter(&record, workflow_id, status.as_ref()) {
                    tasks.push(record);
                }
            }
        }
        
        // Sort by creation time (newest first)
        tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(tasks)
    }

    /// Recover tasks that have been processing too long
    pub fn recover_stalled_tasks(&self) -> Result<u32> {
        let mut recovered = 0;
        let now = chrono::Utc::now().timestamp();
        
        // Find and recover stalled tasks
        for data in self.queue.get_all_processing()? {
            let mut record: TaskRecord = serde_json::from_slice(&data)?;
            
            // Check if task has been processing too long
            if let Some(started_at) = record.started_at {
                if now - started_at > DEFAULT_STALL_TIMEOUT_SECONDS {
                    // Reset status and move back to pending
                    record.status = TaskStatus::Pending;
                    record.started_at = None;
                    
                    let priority = chrono::Utc::now().timestamp_millis() as u64;
                    let serialized = serde_json::to_vec(&record)?;
                    
                    self.queue.remove_from_processing(&record.id)?;
                    self.queue.insert_pending(priority, &serialized)?;
                    
                    recovered += 1;
                }
            }
        }
        
        Ok(recovered)
    }

    /// Check if dependencies are met for a node
    pub fn are_dependencies_met(
        graph: &WorkflowGraph,
        node_id: &str,
        context: &ExecutionContext,
    ) -> bool {
        graph.get_dependencies(node_id)
            .iter()
            .all(|dep| context.node_outputs.contains_key(dep))
    }

    /// Queue downstream tasks after a node completes
    pub fn queue_downstream_tasks(
        &self,
        task: &WorkflowTask,
        output: Value,
    ) -> Result<()> {
        // Update context with node output
        let mut context = task.context.clone();
        context.set_node_output(task.node.id.clone(), output);
        
        // Find and queue ready downstream nodes
        let graph = WorkflowGraph::from_workflow(&task.workflow);
        let downstream_nodes = graph.get_downstream_nodes(&task.node.id);
        
        for downstream_id in downstream_nodes {
            if let Some(downstream_node) = graph.get_node(&downstream_id) {
                if Self::are_dependencies_met(&graph, &downstream_id, &context) {
                    self.push_task(
                        task.record.execution_id.clone(),
                        downstream_node.clone(),
                        task.workflow.clone(),
                        context.clone(),
                        Value::Null,
                    )?;
                }
            }
        }
        
        Ok(())
    }

    /// Helper to check if task record matches filters
    fn matches_record_filter(
        record: &TaskRecord,
        workflow_id: Option<&str>,
        status: Option<&TaskStatus>,
    ) -> bool {
        workflow_id.map_or(true, |id| record.workflow_id == id)
            && status.map_or(true, |s| &record.status == s)
    }
}