use crate::api::{state::AppState, ApiResponse};
use crate::models::{Node, Task, TaskStatus};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct TaskListQuery {
    execution_id: Option<String>,
    status: Option<TaskStatus>,
    limit: Option<u32>,
}

#[derive(Serialize, Deserialize)]
pub struct ExecuteNodeResponse {
    pub task_id: String,
    pub message: String,
}

pub async fn get_execution_status(
    State(state): State<AppState>,
    Path(execution_id): Path<String>,
) -> Json<ApiResponse<Vec<Task>>> {
    match state.executor.get_execution_status(&execution_id).await {
        Ok(tasks) => Json(ApiResponse::ok(tasks)),
        Err(e) => Json(ApiResponse::error(format!("Failed to get execution status: {}", e))),
    }
}

pub async fn get_task_status(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Json<ApiResponse<Task>> {
    match state.executor.get_task_status(&task_id).await {
        Ok(task) => Json(ApiResponse::ok(task)),
        Err(e) => Json(ApiResponse::error(e.to_string())),
    }
}

pub async fn list_tasks(
    State(state): State<AppState>,
    Query(params): Query<TaskListQuery>,
) -> Json<ApiResponse<Vec<Task>>> {
    let _limit = params.limit.unwrap_or(100) as usize;
    let status_filter = params.status.clone();

    match state.executor.list_tasks(None, status_filter).await {
        Ok(tasks) => {
            let filtered = if let Some(exec_id) = params.execution_id {
                tasks
                    .into_iter()
                    .filter(|t| t.execution_id == exec_id)
                    .collect::<Vec<_>>()
            } else {
                tasks
            };

            Json(ApiResponse::ok(filtered))
        }
        Err(e) => Json(ApiResponse::error(format!("Failed to list tasks: {}", e))),
    }
}

pub async fn execute_node(
    State(state): State<AppState>,
    Json(node): Json<Node>,
) -> Json<ApiResponse<ExecuteNodeResponse>> {
    match state.executor.submit_node(node, serde_json::json!({})).await {
        Ok(task_id) => Json(ApiResponse::ok(ExecuteNodeResponse {
            task_id,
            message: "Node execution started".to_string(),
        })),
        Err(e) => Json(ApiResponse::error(format!("Failed to execute node: {}", e))),
    }
}
