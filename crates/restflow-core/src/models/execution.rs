use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Execution status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub enum ExecutionStatus {
    Running,
    Completed,
    Failed,
}

/// Execution summary - used for execution history list
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExecutionSummary {
    /// Execution ID
    pub execution_id: String,
    /// Workflow ID
    pub workflow_id: String,
    /// Execution status
    pub status: ExecutionStatus,
    /// Start time (millisecond timestamp)
    #[ts(type = "number")]
    pub started_at: i64,
    /// Completion time (millisecond timestamp)
    #[ts(type = "number | null")]
    pub completed_at: Option<i64>,
    /// Total number of tasks
    pub total_tasks: usize,
    /// Number of completed tasks
    pub completed_tasks: usize,
    /// Number of failed tasks
    pub failed_tasks: usize,
}

impl ExecutionSummary {
    /// Create a new ExecutionSummary
    pub fn new(execution_id: String, workflow_id: String) -> Self {
        Self {
            execution_id,
            workflow_id,
            status: ExecutionStatus::Running,
            started_at: chrono::Utc::now().timestamp_millis(),
            completed_at: None,
            total_tasks: 0,
            completed_tasks: 0,
            failed_tasks: 0,
        }
    }
}

/// Paginated execution history response
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExecutionHistoryPage {
    pub items: Vec<ExecutionSummary>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}
