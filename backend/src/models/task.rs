use serde::{Deserialize, Serialize};
use serde_json::Value;
use anyhow::Result;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

/// Lightweight task record for storage - only IDs and essential data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: String,
    pub execution_id: String,
    pub workflow_id: String,
    pub node_id: String,
    pub status: TaskStatus,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub input: Value,
    pub output: Option<Value>,
    pub error: Option<String>,
    
    // Store serialized context to avoid loading it separately
    pub context_data: Vec<u8>,
}

impl TaskRecord {
    pub fn new(
        execution_id: String,
        workflow_id: String,
        node_id: String,
        input: Value,
        context_data: Vec<u8>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            execution_id,
            workflow_id,
            node_id,
            status: TaskStatus::Pending,
            created_at: chrono::Utc::now().timestamp(),
            started_at: None,
            completed_at: None,
            input,
            output: None,
            error: None,
            context_data,
        }
    }
}

/// Runtime task with full data for execution
#[derive(Debug, Clone)]
pub struct WorkflowTask {
    pub record: TaskRecord,
    pub node: crate::models::Node,
    pub workflow: crate::models::Workflow,
    pub context: crate::engine::context::ExecutionContext,
}

impl WorkflowTask {
    /// Create from TaskRecord by loading workflow and node
    pub fn from_record(
        record: TaskRecord,
        workflow: crate::models::Workflow,
        context: crate::engine::context::ExecutionContext,
    ) -> Result<Self> {
        // Find the node in the workflow
        let node = workflow
            .nodes
            .iter()
            .find(|n| n.id == record.node_id)
            .ok_or_else(|| anyhow::anyhow!("Node {} not found in workflow", record.node_id))?
            .clone();
        
        Ok(Self {
            record,
            node,
            workflow,
            context,
        })
    }
    
    /// Convert to TaskRecord for storage
    pub fn to_record(&self) -> Result<TaskRecord> {
        let context_data = serde_json::to_vec(&self.context)
            .map_err(|e| anyhow::anyhow!("Failed to serialize context: {}", e))?;
        
        Ok(TaskRecord {
            id: self.record.id.clone(),
            execution_id: self.record.execution_id.clone(),
            workflow_id: self.workflow.id.clone(),
            node_id: self.node.id.clone(),
            status: self.record.status.clone(),
            created_at: self.record.created_at,
            started_at: self.record.started_at,
            completed_at: self.record.completed_at,
            input: self.record.input.clone(),
            output: self.record.output.clone(),
            error: self.record.error.clone(),
            context_data,
        })
    }
}