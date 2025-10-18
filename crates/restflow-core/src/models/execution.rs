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
    pub started_at: i64,
    /// Completion time (millisecond timestamp)
    pub completed_at: Option<i64>,
    /// Total number of tasks
    pub total_tasks: usize,
    /// Number of completed tasks
    pub completed_tasks: usize,
    /// Number of failed tasks
    pub failed_tasks: usize,
}

impl ExecutionSummary {
    /// Create ExecutionSummary from Task list
    pub fn from_tasks(execution_id: String, workflow_id: String, tasks: &[crate::models::Task]) -> Self {
        if tasks.is_empty() {
            return Self {
                execution_id,
                workflow_id,
                status: ExecutionStatus::Running,
                started_at: chrono::Utc::now().timestamp_millis(),
                completed_at: None,
                total_tasks: 0,
                completed_tasks: 0,
                failed_tasks: 0,
            };
        }

        let total_tasks = tasks.len();
        let completed_tasks = tasks.iter().filter(|t| t.status == crate::models::TaskStatus::Completed).count();
        let failed_tasks = tasks.iter().filter(|t| t.status == crate::models::TaskStatus::Failed).count();
        let running_tasks = tasks.iter().filter(|t| t.status == crate::models::TaskStatus::Running).count();

        // Calculate execution status
        let status = if failed_tasks > 0 && running_tasks == 0 && (completed_tasks + failed_tasks == total_tasks) {
            ExecutionStatus::Failed
        } else if completed_tasks == total_tasks {
            ExecutionStatus::Completed
        } else {
            ExecutionStatus::Running
        };

        // Find earliest start time
        let started_at = tasks
            .iter()
            .filter_map(|t| t.started_at.or(Some(t.created_at)))
            .min()
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

        // Find latest completion time (if all tasks are done)
        let completed_at = if status == ExecutionStatus::Running {
            None
        } else {
            tasks.iter().filter_map(|t| t.completed_at).max()
        };

        Self {
            execution_id,
            workflow_id,
            status,
            started_at,
            completed_at,
            total_tasks,
            completed_tasks,
            failed_tasks,
        }
    }
}
