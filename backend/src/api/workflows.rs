use crate::api::state::AppState;
use crate::engine::executor::WorkflowExecutor;
use crate::models::Workflow;
use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::Value;

// GET /api/workflow/list
pub async fn list_workflows(
    State(state): State<AppState>,
) -> Json<Value> {
    match state.storage.workflows.list_workflows() {
        Ok(workflows) => Json(serde_json::json!({
            "status": "success",
            "data": workflows
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to list workflows: {}", e)
        }))
    }
}

// POST /api/workflow/create
pub async fn create_workflow(
    State(state): State<AppState>,
    Json(workflow): Json<Workflow>,
) -> Json<Value> {
    match state.storage.workflows.create_workflow(&workflow) {
        Ok(_) => Json(serde_json::json!({
            "status": "success",
            "message": format!("Workflow {} saved!", workflow.name),
            "data": {"id": workflow.id}
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to save workflow: {}", e)
        }))
    }
}

// GET /api/workflow/get/{id}
pub async fn get_workflow(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<Value> {
    match state.storage.workflows.get_workflow(&id) {
        Ok(workflow) => Json(serde_json::json!(workflow)),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": e.to_string()
        }))
    }
}

// PUT /api/workflow/update/{id}
pub async fn update_workflow(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(workflow): Json<Workflow>,
) -> Json<Value> {
    match state.storage.workflows.update_workflow(&id, &workflow) {
        Ok(_) => Json(serde_json::json!({
            "status": "success",
            "message": format!("Workflow {} updated!", id),
            "data": {}
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to update workflow: {}", e)
        }))
    }
}

// DELETE /api/workflow/delete/{id}
pub async fn delete_workflow(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<Value> {
    match state.storage.workflows.delete_workflow(&id) {
        Ok(_) => Json(serde_json::json!({
            "status": "success",
            "message": format!("Workflow {} deleted!", id),
            "data": {}
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to delete workflow: {}", e)
        }))
    }
}

// POST /api/execution/sync/run
pub async fn execute_workflow(
    State(state): State<AppState>,
    Json(mut workflow): Json<Workflow>,
) -> Json<Value> {
    workflow.id = format!("inline-{}", uuid::Uuid::new_v4());

    let mut wf_executor = WorkflowExecutor::new_sync(workflow, Some(state.storage.clone()));
    match wf_executor.execute().await {
        Ok(output) => Json(serde_json::json!({
            "status": "success",
            "data": output
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Workflow execution failed: {}", e)
        })),
    }
}

// POST /api/execution/sync/run-workflow/{workflow_id}
pub async fn execute_workflow_by_id(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
    Json(input): Json<Value>,
) -> Json<Value> {
    match state.storage.workflows.get_workflow(&workflow_id) {
        Ok(workflow) => {
            let mut wf_executor = WorkflowExecutor::new_sync(workflow, Some(state.storage.clone()));
            wf_executor.set_input(input);
            match wf_executor.execute().await {
                Ok(output) => Json(serde_json::json!({
                    "status": "success",
                    "data": output
                })),
                Err(e) => Json(serde_json::json!({
                    "status": "error",
                    "message": format!("Workflow execution failed: {}", e)
                })),
            }
        }
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": e.to_string()
        })),
    }
}

// POST /api/execution/async/submit/{workflow_id}
pub async fn submit_workflow(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
    Json(input): Json<Value>,
) -> Json<Value> {
    match state.executor.submit(workflow_id.clone(), input).await {
        Ok(execution_id) => Json(serde_json::json!({
            "status": "success",
            "data": {
                "execution_id": execution_id,
                "workflow_id": workflow_id,
            }
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to submit workflow: {}", e)
        })),
    }
}