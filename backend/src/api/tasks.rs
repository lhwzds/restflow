use crate::api::state::AppState;
use crate::models::{Node, TaskStatus};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
pub struct TaskListQuery {
    execution_id: Option<String>,
    status: Option<TaskStatus>,
    limit: Option<u32>,
}

// GET /api/execution/status/{execution_id}
pub async fn get_execution_status(
    State(state): State<AppState>,
    Path(execution_id): Path<String>,
) -> Json<Value> {
    match state.executor.get_execution_status(&execution_id).await {
        Ok(tasks) => Json(serde_json::json!({
            "status": "success",
            "data": tasks
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to get execution status: {}", e)
        })),
    }
}

// GET /api/task/status/{task_id}
pub async fn get_task_status(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Json<Value> {
    match state.executor.get_task_status(&task_id).await {
        Ok(task) => Json(serde_json::json!({
            "status": "success",
            "data": task
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": e.to_string()
        })),
    }
}

// GET /api/task/list
pub async fn list_tasks(
    State(state): State<AppState>,
    Query(params): Query<TaskListQuery>,
) -> Json<Value> {
    let _limit = params.limit.unwrap_or(100) as usize;
    let status_filter = params.status.clone();
    
    match state.executor.list_tasks(None, status_filter).await {
        Ok(tasks) => {
            let mut filtered = tasks;
            
            // Filter by execution_id if provided
            if let Some(exec_id) = params.execution_id {
                filtered = filtered.into_iter()
                    .filter(|t| t.execution_id == exec_id)
                    .collect();
            }
            
            Json(serde_json::json!({
                "status": "success",
                "data": filtered
            }))
        }
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to list tasks: {}", e)
        })),
    }
}

// POST /api/node/execute
pub async fn execute_node(
    State(state): State<AppState>,
    Json(node): Json<Node>,
) -> Json<Value> {
    match state.executor.submit_node(node, serde_json::json!({})).await {
        Ok(task_id) => Json(serde_json::json!({
            "status": "success",
            "data": {
                "task_id": task_id,
                "message": "Node execution started"
            }
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to execute node: {}", e)
        })),
    }
}