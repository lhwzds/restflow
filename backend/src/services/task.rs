use crate::{AppCore, models::{Task, TaskStatus, Node}};
use anyhow::{Result, Context};
use serde_json::Value;
use std::sync::Arc;

// Get task status by ID
pub async fn get_task_status(
    core: &Arc<AppCore>,
    task_id: &str
) -> Result<Task> {
    core.executor.get_task_status(task_id).await
        .with_context(|| format!("Failed to get status for task {}", task_id))
}

// Get execution status (all tasks for an execution)
pub async fn get_execution_status(
    core: &Arc<AppCore>,
    execution_id: &str
) -> Result<Vec<Task>> {
    core.executor.get_execution_status(execution_id).await
        .with_context(|| format!("Failed to get execution status for {}", execution_id))
}

// List tasks with optional filters
pub async fn list_tasks(
    core: &Arc<AppCore>,
    execution_id: Option<String>,
    status: Option<TaskStatus>,
    limit: Option<u32>
) -> Result<Vec<Task>> {
    // If execution_id is provided, get tasks for that execution
    if let Some(exec_id) = execution_id {
        let mut tasks = core.executor.get_execution_status(&exec_id).await
            .with_context(|| format!("Failed to get tasks for execution {}", exec_id))?;

        // Apply status filter if provided
        if let Some(status_filter) = status {
            tasks.retain(|t| t.status == status_filter);
        }

        // Apply limit if provided
        if let Some(limit) = limit {
            tasks.truncate(limit as usize);
        }

        Ok(tasks)
    } else {
        // List all tasks with optional status filter
        let mut tasks = core.executor.list_tasks(None, status).await
            .context("Failed to list tasks")?;

        // Apply limit if provided
        if let Some(limit) = limit {
            tasks.truncate(limit as usize);
        }

        Ok(tasks)
    }
}

// Execute a single node
pub async fn execute_node(
    core: &Arc<AppCore>,
    node: Node,
    input: Value
) -> Result<String> {
    core.executor.submit_node(node, input).await
        .context("Failed to execute node")
}