use crate::engine::context::ExecutionContext;
use crate::models::{Node, NodeOutput, Workflow};
use crate::storage::Storage;
use anyhow::Result;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

/// Unified task structure that replaces both TaskRecord and WorkflowTask
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Task {
    // Core fields (always persisted)
    pub id: String,
    pub execution_id: String,
    pub workflow_id: String,
    pub node_id: String,
    pub status: TaskStatus,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    #[ts(type = "any")]
    pub input: Value,
    pub output: Option<NodeOutput>,
    pub error: Option<String>,

    // Execution context (serialized and stored)
    pub context: ExecutionContext,

    // Runtime data (lazy-loaded, not serialized)
    #[serde(skip)]
    #[ts(skip)]
    node: OnceCell<Node>,
    #[serde(skip)]
    #[ts(skip)]
    workflow: OnceCell<Arc<Workflow>>,
}

impl Task {
    /// Create a new task
    pub fn new(
        execution_id: String,
        workflow_id: String,
        node_id: String,
        input: Value,
        context: ExecutionContext,
    ) -> Self {
        // Use nanosecond precision to avoid collision in high-concurrency scenarios
        // Note: Nanosecond precision provides ~10^9 unique values per second, making collision
        // probability negligible in practice. If absolute uniqueness is required in the future,
        // consider using (timestamp_nanos, uuid) composite key for the pending queue.
        let created_at = chrono::Utc::now().timestamp_nanos_opt().unwrap_or_else(|| {
            // Fallback for year > 2262 (extremely unlikely)
            chrono::Utc::now().timestamp_millis() * 1_000_000
        });

        Self {
            id: Uuid::new_v4().to_string(),
            execution_id,
            workflow_id,
            node_id,
            status: TaskStatus::Pending,
            created_at,
            started_at: None,
            completed_at: None,
            input,
            output: None,
            error: None,
            context,
            node: OnceCell::new(),
            workflow: OnceCell::new(),
        }
    }

    /// Get the node for this task (lazy-loaded)
    pub fn get_node(&self, storage: &Storage) -> Result<&Node> {
        self.node.get_or_try_init(|| {
            let workflow = self.get_workflow(storage)?;
            workflow
                .nodes
                .iter()
                .find(|n| n.id == self.node_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Node {} not found in workflow", self.node_id))
        })
    }

    /// Get the workflow for this task (lazy-loaded and shared)
    pub fn get_workflow(&self, storage: &Storage) -> Result<Arc<Workflow>> {
        self.workflow
            .get_or_try_init(|| Ok(Arc::new(storage.workflows.get_workflow(&self.workflow_id)?)))
            .cloned()
    }

    /// Pre-populate the workflow Arc to avoid lazy loading from storage
    /// This is useful when creating tasks from a workflow that's already in memory
    pub fn set_workflow(&self, workflow: Arc<Workflow>) -> Result<(), Arc<Workflow>> {
        self.workflow.set(workflow)
    }

    /// Mark task as running
    pub fn start(&mut self) {
        self.status = TaskStatus::Running;
        self.started_at = Some(chrono::Utc::now().timestamp_millis());
    }

    /// Mark task as completed
    pub fn complete(&mut self, output: NodeOutput) {
        self.status = TaskStatus::Completed;
        self.completed_at = Some(chrono::Utc::now().timestamp_millis());
        self.output = Some(output);
    }

    /// Mark task as failed
    pub fn fail(&mut self, error: String) {
        self.status = TaskStatus::Failed;
        self.completed_at = Some(chrono::Utc::now().timestamp_millis());
        self.error = Some(error);
    }

    /// Create a task for a single node execution (no workflow context)
    pub fn for_single_node(node: Node, input: Value) -> Self {
        let execution_id = Uuid::new_v4().to_string();
        let workflow_id = format!("single-node-{}", node.id);
        let context = ExecutionContext::new(execution_id.clone());

        let task = Self::new(execution_id, workflow_id, node.id.clone(), input, context);

        // Pre-populate the node since we already have it
        let _ = task.node.set(node);
        task
    }

    /// Get priority for queue ordering (lower timestamp = higher priority)
    pub fn priority(&self) -> u64 {
        self.created_at as u64
    }
}
