use crate::api_response::{error, not_found, success};
use crate::engine::executor::AsyncWorkflowExecutor;
use crate::engine::trigger_manager::TriggerManager;
use crate::models::{Node, TaskStatus};
use crate::storage::Storage;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

pub type AppState = (Arc<Storage>, Arc<AsyncWorkflowExecutor>, Arc<TriggerManager>);

#[derive(Deserialize)]
pub struct TaskListQuery {
    execution_id: Option<String>,
    status: Option<TaskStatus>,
    limit: Option<u32>,
}

// GET /api/execution/status/{execution_id}
pub async fn get_execution_status(
    State((_, executor, _)): State<AppState>,
    Path(execution_id): Path<String>,
) -> Json<Value> {
    match executor.get_execution_status(&execution_id).await {
        Ok(tasks) => success(tasks),
        Err(e) => error(format!("Failed to get execution status: {}", e)),
    }
}

// GET /api/task/status/{task_id}
pub async fn get_task_status(
    State((_, executor, _)): State<AppState>,
    Path(task_id): Path<String>,
) -> Json<Value> {
    match executor.get_task_status(&task_id).await {
        Ok(Some(task)) => success(task),
        Ok(None) => not_found("Task not found".to_string()),
        Err(e) => error(format!("Failed to get task status: {}", e)),
    }
}

// GET /api/task/list
pub async fn list_tasks(
    State((_, executor, _)): State<AppState>,
    Query(params): Query<TaskListQuery>,
) -> Json<Value> {
    let _limit = params.limit.unwrap_or(100) as usize;
    let status_filter = params.status.clone();
    
    match executor.list_tasks(None, status_filter).await {
        Ok(tasks) => {
            let mut filtered = tasks;
            
            // Filter by execution_id if provided
            if let Some(exec_id) = params.execution_id {
                filtered = filtered.into_iter()
                    .filter(|t| t.execution_id == exec_id)
                    .collect();
            }
            
            success(filtered)
        }
        Err(e) => error(format!("Failed to list tasks: {}", e)),
    }
}

// POST /api/node/execute
pub async fn execute_node(
    State((_, executor, _)): State<AppState>,
    Json(node): Json<Node>,
) -> Json<Value> {
    match executor.submit_node(node, serde_json::json!({})).await {
        Ok(task_id) => success(serde_json::json!({
            "task_id": task_id,
            "message": "Node execution started"
        })),
        Err(e) => error(format!("Failed to execute node: {}", e)),
    }
}