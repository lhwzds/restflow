use crate::models::{Node, Workflow, Task, TaskStatus};
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
        // Create unified Task
        let task = Task::new(
            execution_id,
            workflow.id.clone(),
            node.id.clone(),
            input,
            context,
        );
        let task_id = task.id.clone();

        let priority = task.priority();
        let serialized = serde_json::to_vec(&task)?;
        self.queue.insert_pending(priority, &serialized)?;

        Ok(task_id)
    }

    /// Add a single node task for standalone execution
    pub fn push_single_node(&self, node: Node, input: Value) -> Result<String> {
        // Create task for single node
        let task = Task::for_single_node(node, input);
        let task_id = task.id.clone();

        let priority = task.priority();
        let serialized = serde_json::to_vec(&task)?;
        self.queue.insert_pending(priority, &serialized)?;

        Ok(task_id)
    }

    /// Pop a task from the queue (blocks until task available)
    pub async fn pop_task(&self) -> Result<Task> {
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
    fn try_pop_task(&self) -> Result<Option<Task>> {
        // Get first pending task
        if let Some((priority, data)) = self.queue.get_first_pending()? {
            let mut task: Task = serde_json::from_slice(&data)?;
            
            // Update task status
            task.start();
            
            // Move to processing
            let serialized = serde_json::to_vec(&task)?;
            self.queue.move_to_processing(priority, &task.id, &serialized)?;
            
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
        // Get task from processing
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
            
            // Move to completed
            let serialized = serde_json::to_vec(&task)?;
            self.queue.move_to_completed(task_id, &serialized)?;
        }
        
        Ok(())
    }

    /// Query all tasks across three tables with custom filter
    fn query_all_tasks<F>(&self, filter: F) -> Result<Vec<Task>>
    where
        F: Fn(&Task) -> bool,
    {
        let mut tasks = Vec::new();

        // Query all three tables
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

    /// Get task records by execution ID from all tables
    pub fn get_tasks_by_execution(&self, execution_id: &str) -> Result<Vec<Task>> {
        let mut tasks = self.query_all_tasks(|task| task.execution_id == execution_id)?;

        // Sort by creation time
        tasks.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(tasks)
    }

    /// Get a task by ID from any table
    pub fn get_task(&self, task_id: &str) -> Result<Option<Task>> {
        if let Some(data) = self.queue.get_from_any_table(task_id)? {
            let task: Task = serde_json::from_slice(&data)?;
            Ok(Some(task))
        } else {
            Ok(None)
        }
    }

    /// List tasks with optional filters
    pub fn list_tasks(&self, workflow_id: Option<&str>, status: Option<TaskStatus>) -> Result<Vec<Task>> {
        let mut tasks = self.query_all_tasks(|task| {
            workflow_id.map_or(true, |id| task.workflow_id == id)
                && status.as_ref().map_or(true, |s| &task.status == s)
        })?;

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
            let mut task: Task = serde_json::from_slice(&data)?;
            
            // Check if task has been processing too long
            if let Some(started_at) = task.started_at {
                if now - started_at > DEFAULT_STALL_TIMEOUT_SECONDS {
                    // Reset status and move back to pending
                    task.status = TaskStatus::Pending;
                    task.started_at = None;
                    
                    let priority = task.priority();
                    let serialized = serde_json::to_vec(&task)?;
                    
                    self.queue.remove_from_processing(&task.id)?;
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
            .all(|dep| context.get_node(dep).is_some())
    }

    /// Queue downstream tasks after a node completes
    pub fn queue_downstream_tasks(
        &self,
        task: &Task,
        output: Value,
    ) -> Result<()> {
        // Get workflow from task
        let workflow = task.get_workflow(&self.storage)?;
        
        // Update context with node output
        let mut context = task.context.clone();
        context.set_node(&task.node_id, output);
        
        // Find and queue ready downstream nodes
        let graph = WorkflowGraph::from_workflow(&workflow);
        let downstream_nodes = graph.get_downstream_nodes(&task.node_id);
        
        for downstream_id in downstream_nodes {
            if let Some(downstream_node) = graph.get_node(&downstream_id) {
                if Self::are_dependencies_met(&graph, &downstream_id, &context) {
                    // Nodes reference data via {{...}} templates in config, no need for resolve_node_input
                    self.push_task(
                        task.execution_id.clone(),
                        downstream_node.clone(),
                        (*workflow).clone(),
                        context.clone(),
                        Value::Null,  // No longer need to pass input
                    )?;
                }
            }
        }

        Ok(())
    }
}
