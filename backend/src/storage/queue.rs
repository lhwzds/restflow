use anyhow::Result;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Notify;
use uuid::Uuid;
use crate::core::workflow::{Node, Workflow};
use crate::engine::context::ExecutionContext;

// KISS: Three-table design achieves O(1) pop vs single table's O(n) scan - simpler and faster

const PENDING: TableDefinition<u64, &[u8]> = TableDefinition::new("pending");
const PROCESSING: TableDefinition<&str, &[u8]> = TableDefinition::new("processing");
const COMPLETED: TableDefinition<&str, &[u8]> = TableDefinition::new("completed");

// Task is considered stalled after 5 minutes
const TASK_STALL_TIMEOUT_SECONDS: i64 = 300;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

// KISS: Everything is a node task - workflows are just decomposed at submission
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowTask {
    pub id: String,
    pub execution_id: String,  // Groups nodes from same workflow execution
    pub node: Node,            // The node to execute
    pub workflow: Workflow,    // Complete workflow for finding downstream nodes
    pub context: ExecutionContext,  // Accumulated execution context
    pub status: TaskStatus,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub input: Value,
    pub output: Option<Value>,
    pub error: Option<String>,
}

impl WorkflowTask {
    /// Create a node task
    pub fn new(
        execution_id: String,
        node: Node,
        workflow: Workflow,
        context: ExecutionContext,
        input: Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            execution_id,
            node,
            workflow,
            context,
            status: TaskStatus::Pending,
            created_at: chrono::Utc::now().timestamp(),
            started_at: None,
            completed_at: None,
            input,
            output: None,
            error: None,
        }
    }
    
    /// Create a single node task for standalone execution
    pub fn new_single_node(node: Node, input: Value) -> Self {
        // For single node, create a minimal workflow
        let workflow = Workflow {
            id: format!("single-{}", node.id),
            name: format!("Single Node: {}", node.id),
            nodes: vec![node.clone()],
            edges: vec![],
        };
        
        let execution_id = Uuid::new_v4().to_string();
        let context = ExecutionContext::new(execution_id.clone());
        
        Self::new(execution_id, node, workflow, context, input)
    }
}

pub struct TaskQueue {
    db: Arc<Database>,
    notify: Arc<Notify>,
}

impl TaskQueue {
    /// Create a new task queue instance
    pub fn new(db: Arc<Database>) -> Result<Self> {
        // Ensure tables exist
        let write_txn = db.begin_write()?;
        write_txn.open_table(PENDING)?;
        write_txn.open_table(PROCESSING)?;
        write_txn.open_table(COMPLETED)?;
        write_txn.commit()?;

        Ok(Self {
            db,
            notify: Arc::new(Notify::new()),
        })
    }

    /// Add a node task to the queue
    pub fn push(&self, 
        execution_id: String,
        node: Node, 
        workflow: Workflow, 
        context: ExecutionContext, 
        input: Value
    ) -> Result<String> {
        let task = WorkflowTask::new(execution_id, node, workflow, context, input);
        let task_id = task.id.clone();

        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(PENDING)?;
            
            let priority = chrono::Utc::now().timestamp_millis() as u64;
            let serialized = serde_json::to_vec(&task)?;
            table.insert(priority, serialized.as_slice())?;
        }
        write_txn.commit()?;
        
        self.notify.notify_one();
        
        Ok(task_id)
    }
    
    /// Add a single node task for standalone execution  
    pub fn push_single_node(&self, node: Node, input: Value) -> Result<String> {
        let task = WorkflowTask::new_single_node(node, input);
        let task_id = task.id.clone();

        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(PENDING)?;
            
            let priority = chrono::Utc::now().timestamp_millis() as u64;
            let serialized = serde_json::to_vec(&task)?;
            table.insert(priority, serialized.as_slice())?;
        }
        write_txn.commit()?;
        
        self.notify.notify_one();
        
        Ok(task_id)
    }

    /// Pop a task from the queue (blocks until task available)
    pub async fn pop(&self) -> Result<WorkflowTask> {
        loop {
            match self.try_pop() {
                Ok(Some(task)) => return Ok(task),
                Ok(None) => {
                    // Wait for notification when queue is empty
                    self.notify.notified().await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Try to pop a task without blocking
    fn try_pop(&self) -> Result<Option<WorkflowTask>> {
        let write_txn = self.db.begin_write()?;
        
        // Get first pending task
        let task = {
            let pending = write_txn.open_table(PENDING)?;
            if let Some((key, value)) = pending.first()? {
                let task: WorkflowTask = serde_json::from_slice(value.value())?;
                Some((key.value(), task))
            } else {
                None
            }
        };

        if let Some((key, task)) = task {
            // Update task status
            let mut running_task = task;
            running_task.status = TaskStatus::Running;
            running_task.started_at = Some(chrono::Utc::now().timestamp());
            
            // Remove from pending
            {
                let mut pending = write_txn.open_table(PENDING)?;
                pending.remove(&key)?;
            }
            
            // Add to processing
            {
                let mut processing = write_txn.open_table(PROCESSING)?;
                let serialized = serde_json::to_vec(&running_task)?;
                processing.insert(running_task.id.as_str(), serialized.as_slice())?;
            }
            
            write_txn.commit()?;
            Ok(Some(running_task))
        } else {
            write_txn.commit()?;
            Ok(None)
        }
    }

    /// Mark a task as completed with output
    pub fn complete(&self, task_id: &str, output: Value) -> Result<()> {
        self.finish_task(task_id, TaskStatus::Completed, Some(output), None)
    }

    /// Mark a task as failed with error message
    pub fn fail(&self, task_id: &str, error: String) -> Result<()> {
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
        let write_txn = self.db.begin_write()?;
        
        // Get task from processing table
        let task = {
            let processing = write_txn.open_table(PROCESSING)?;
            if let Some(data) = processing.get(task_id)? {
                let mut task: WorkflowTask = serde_json::from_slice(data.value())?;
                task.status = status;
                task.completed_at = Some(chrono::Utc::now().timestamp());
                task.output = output;
                task.error = error;
                Some(task)
            } else {
                None
            }
        };

        if let Some(task) = task {
            // Remove from processing
            {
                let mut processing = write_txn.open_table(PROCESSING)?;
                processing.remove(task_id)?;
            }
            
            // Add to completed
            {
                let mut completed = write_txn.open_table(COMPLETED)?;
                let serialized = serde_json::to_vec(&task)?;
                completed.insert(task_id, serialized.as_slice())?;
            }
        }
        
        write_txn.commit()?;
        Ok(())
    }

    /// Get a task by ID from any table
    pub fn get_task(&self, task_id: &str) -> Result<Option<WorkflowTask>> {
        let read_txn = self.db.begin_read()?;
        
        // Check processing table
        let processing = read_txn.open_table(PROCESSING)?;
        if let Some(data) = processing.get(task_id)? {
            let task: WorkflowTask = serde_json::from_slice(data.value())?;
            return Ok(Some(task));
        }
        
        // Check completed table
        let completed = read_txn.open_table(COMPLETED)?;
        if let Some(data) = completed.get(task_id)? {
            let task: WorkflowTask = serde_json::from_slice(data.value())?;
            return Ok(Some(task));
        }
        
        // Check pending table (requires iteration)
        let pending = read_txn.open_table(PENDING)?;
        for entry in pending.iter()? {
            let (_, value) = entry?;
            let task: WorkflowTask = serde_json::from_slice(value.value())?;
            if task.id == task_id {
                return Ok(Some(task));
            }
        }
        
        Ok(None)
    }

    /// List tasks with optional filters
    pub fn list_tasks(&self, workflow_id: Option<&str>, status: Option<TaskStatus>) -> Result<Vec<WorkflowTask>> {
        let read_txn = self.db.begin_read()?;
        let mut tasks = Vec::new();
        
        // Check pending tasks
        if status.is_none() || status == Some(TaskStatus::Pending) {
            let pending = read_txn.open_table(PENDING)?;
            for entry in pending.iter()? {
                let (_, value) = entry?;
                let task: WorkflowTask = serde_json::from_slice(value.value())?;
                if Self::matches_filter(&task, workflow_id, None) {
                    tasks.push(task);
                }
            }
        }
        
        // Check running tasks
        if status.is_none() || status == Some(TaskStatus::Running) {
            let processing = read_txn.open_table(PROCESSING)?;
            for entry in processing.iter()? {
                let (_, value) = entry?;
                let task: WorkflowTask = serde_json::from_slice(value.value())?;
                if Self::matches_filter(&task, workflow_id, None) {
                    tasks.push(task);
                }
            }
        }
        
        // Check completed tasks
        if status.is_none() || matches!(status, Some(TaskStatus::Completed) | Some(TaskStatus::Failed)) {
            let completed = read_txn.open_table(COMPLETED)?;
            for entry in completed.iter()? {
                let (_, value) = entry?;
                let task: WorkflowTask = serde_json::from_slice(value.value())?;
                if Self::matches_filter(&task, workflow_id, status.as_ref()) {
                    tasks.push(task);
                }
            }
        }
        
        // Sort by creation time (newest first)
        tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(tasks)
    }

    /// Recover tasks that have been processing too long
    pub fn recover_stalled_tasks(&self) -> Result<u32> {
        let write_txn = self.db.begin_write()?;
        let mut recovered = 0;

        // Find stalled tasks
        let stalled_tasks = {
            let processing = write_txn.open_table(PROCESSING)?;
            let mut tasks = Vec::new();
            for entry in processing.iter()? {
                let (key, value) = entry?;
                let task: WorkflowTask = serde_json::from_slice(value.value())?;
                
                // Check if task has been processing too long
                let now = chrono::Utc::now().timestamp();
                if let Some(started_at) = task.started_at {
                    if now - started_at > TASK_STALL_TIMEOUT_SECONDS {
                        tasks.push((key.value().to_string(), task));
                    }
                }
            }
            tasks
        };
        
        // Move stalled tasks back to pending
        for (task_id, mut task) in stalled_tasks {
            // Remove from processing
            {
                let mut processing = write_txn.open_table(PROCESSING)?;
                processing.remove(task_id.as_str())?;
            }

            // Reset status and add back to pending
            {
                task.status = TaskStatus::Pending;
                task.started_at = None;
                
                let mut pending = write_txn.open_table(PENDING)?;
                let priority = chrono::Utc::now().timestamp_millis() as u64;
                let serialized = serde_json::to_vec(&task)?;
                pending.insert(priority, serialized.as_slice())?;
            }
            
            recovered += 1;
        }
        
        write_txn.commit()?;
        
        // Notify if we recovered any tasks
        if recovered > 0 {
            self.notify.notify_one();
        }
        
        Ok(recovered)
    }

    /// Helper to check if task matches filters
    fn matches_filter(
        task: &WorkflowTask,
        workflow_id: Option<&str>,
        status: Option<&TaskStatus>,
    ) -> bool {
        workflow_id.map_or(true, |id| task.workflow.id == id)
            && status.map_or(true, |s| &task.status == s)
    }
}