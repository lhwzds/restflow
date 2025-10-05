use crate::api::{state::AppState, ApiResponse};
use crate::engine::executor::WorkflowExecutor;
use crate::models::Workflow;
use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize)]
pub struct WorkflowIdResponse {
    pub id: String,
}

#[derive(Serialize, Deserialize)]
pub struct ExecutionResponse {
    pub execution_id: String,
    pub workflow_id: String,
}

// GET /api/workflows
pub async fn list_workflows(
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<Workflow>>> {
    match state.storage.workflows.list_workflows() {
        Ok(workflows) => Json(ApiResponse::ok(workflows)),
        Err(e) => Json(ApiResponse::error(format!("Failed to list workflows: {}", e))),
    }
}

// POST /api/workflows
pub async fn create_workflow(
    State(state): State<AppState>,
    Json(workflow): Json<Workflow>,
) -> Json<ApiResponse<WorkflowIdResponse>> {
    match state.storage.workflows.create_workflow(&workflow) {
        Ok(_) => Json(ApiResponse::ok_with_message(
            WorkflowIdResponse { id: workflow.id.clone() },
            format!("Workflow {} saved!", workflow.name),
        )),
        Err(e) => Json(ApiResponse::error(format!("Failed to save workflow: {}", e))),
    }
}

// GET /api/workflows/{id}
pub async fn get_workflow(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<Workflow>> {
    match state.storage.workflows.get_workflow(&id) {
        Ok(workflow) => Json(ApiResponse::ok(workflow)),
        Err(e) => Json(ApiResponse::error(e.to_string())),
    }
}

// PUT /api/workflows/{id}
pub async fn update_workflow(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(workflow): Json<Workflow>,
) -> Json<ApiResponse<()>> {
    match state.storage.workflows.update_workflow(&id, &workflow) {
        Ok(_) => Json(ApiResponse::message(format!("Workflow {} updated!", id))),
        Err(e) => Json(ApiResponse::error(format!("Failed to update workflow: {}", e))),
    }
}

// DELETE /api/workflows/{id}
pub async fn delete_workflow(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<()>> {
    match state.storage.workflows.delete_workflow(&id) {
        Ok(_) => Json(ApiResponse::message(format!("Workflow {} deleted!", id))),
        Err(e) => Json(ApiResponse::error(format!("Failed to delete workflow: {}", e))),
    }
}

// POST /api/workflows/execute
pub async fn execute_workflow(
    State(state): State<AppState>,
    Json(mut workflow): Json<Workflow>,
) -> Json<ApiResponse<Value>> {
    workflow.id = format!("inline-{}", uuid::Uuid::new_v4());

    let mut wf_executor = WorkflowExecutor::new_sync(workflow, Some(state.storage.clone()), state.registry.clone());
    match wf_executor.execute().await {
        Ok(output) => Json(ApiResponse::ok(output)),
        Err(e) => Json(ApiResponse::error(format!("Workflow execution failed: {}", e))),
    }
}

// POST /api/workflows/{workflow_id}/execute
pub async fn execute_workflow_by_id(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
    Json(input): Json<Value>,
) -> Json<ApiResponse<Value>> {
    match state.storage.workflows.get_workflow(&workflow_id) {
        Ok(workflow) => {
            let mut wf_executor = WorkflowExecutor::new_sync(workflow, Some(state.storage.clone()), state.registry.clone());
            wf_executor.set_input(input);
            match wf_executor.execute().await {
                Ok(output) => Json(ApiResponse::ok(output)),
                Err(e) => Json(ApiResponse::error(format!("Workflow execution failed: {}", e))),
            }
        }
        Err(e) => Json(ApiResponse::error(e.to_string())),
    }
}

// POST /api/workflows/{workflow_id}/executions
pub async fn submit_workflow(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
    Json(input): Json<Value>,
) -> Json<ApiResponse<ExecutionResponse>> {
    match state.executor.submit(workflow_id.clone(), input).await {
        Ok(execution_id) => Json(ApiResponse::ok(ExecutionResponse {
            execution_id,
            workflow_id,
        })),
        Err(e) => Json(ApiResponse::error(format!("Failed to submit workflow: {}", e))),
    }
}
