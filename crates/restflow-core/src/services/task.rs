use crate::{
    AppCore,
    models::{Node, Task, TaskStatus},
};
use anyhow::{Context, Result};
use serde_json::Value;
use std::sync::Arc;

pub async fn get_task_status(core: &Arc<AppCore>, task_id: &str) -> Result<Task> {
    core.executor
        .get_task_status(task_id)
        .await
        .with_context(|| format!("Failed to get status for task {}", task_id))
}

pub async fn get_execution_status(core: &Arc<AppCore>, execution_id: &str) -> Result<Vec<Task>> {
    core.executor
        .get_execution_status(execution_id)
        .await
        .with_context(|| format!("Failed to get execution status for {}", execution_id))
}

pub async fn list_tasks(
    core: &Arc<AppCore>,
    execution_id: Option<String>,
    status: Option<TaskStatus>,
    limit: Option<u32>,
) -> Result<Vec<Task>> {
    if let Some(exec_id) = execution_id {
        let mut tasks = core
            .executor
            .get_execution_status(&exec_id)
            .await
            .with_context(|| format!("Failed to get tasks for execution {}", exec_id))?;

        if let Some(status_filter) = status {
            tasks.retain(|t| t.status == status_filter);
        }

        if let Some(limit) = limit {
            tasks.truncate(limit as usize);
        }

        Ok(tasks)
    } else {
        let mut tasks = core
            .executor
            .list_tasks(None, status)
            .await
            .context("Failed to list tasks")?;

        if let Some(limit) = limit {
            tasks.truncate(limit as usize);
        }

        Ok(tasks)
    }
}

pub async fn execute_node(core: &Arc<AppCore>, node: Node, input: Value) -> Result<String> {
    core.executor
        .submit_node(node, input)
        .await
        .context("Failed to execute node")
}

pub async fn list_execution_history(
    core: &Arc<AppCore>,
    workflow_id: &str,
    limit: usize,
) -> Result<Vec<crate::models::ExecutionSummary>> {
    use crate::models::ExecutionSummary;
    use std::collections::HashMap;

    let tasks = core
        .executor
        .list_tasks(Some(workflow_id), None)
        .await
        .with_context(|| format!("Failed to list tasks for workflow {}", workflow_id))?;

    // Group tasks by execution_id to aggregate execution-level statistics
    let mut executions: HashMap<String, Vec<crate::models::Task>> = HashMap::new();
    for task in tasks {
        executions
            .entry(task.execution_id.clone())
            .or_default()
            .push(task);
    }

    let mut summaries: Vec<ExecutionSummary> = executions
        .into_iter()
        .map(|(execution_id, tasks)| {
            ExecutionSummary::from_tasks(execution_id, workflow_id.to_string(), &tasks)
        })
        .collect();

    summaries.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    summaries.truncate(limit);

    Ok(summaries)
}
