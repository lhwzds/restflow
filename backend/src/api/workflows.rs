use crate::api_response::{error, not_found, success, success_with_message};
use crate::engine::executor::{AsyncWorkflowExecutor, WorkflowExecutor};
use crate::engine::trigger_manager::TriggerManager;
use crate::models::Workflow;
use crate::storage::Storage;
use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::Value;
use std::sync::Arc;

pub type AppState = (Arc<Storage>, Arc<AsyncWorkflowExecutor>, Arc<TriggerManager>);

// GET /api/workflow/list
pub async fn list_workflows(
    State((storage, _, _)): State<AppState>,
) -> Json<Value> {
    storage.workflows.list_workflows()
        .map(success)
        .unwrap_or_else(|e| error(format!("Failed to list workflows: {}", e)))
}

// POST /api/workflow/create
pub async fn create_workflow(
    State((storage, _, _)): State<AppState>,
    Json(workflow): Json<Workflow>,
) -> Json<Value> {
    storage.workflows.create_workflow(&workflow)
        .map(|_| success_with_message(
            serde_json::json!({"id": workflow.id}),
            format!("Workflow {} saved!", workflow.name)
        ))
        .unwrap_or_else(|e| error(format!("Failed to save workflow: {}", e)))
}

// GET /api/workflow/get/{id}
pub async fn get_workflow(
    State((storage, _, _)): State<AppState>,
    Path(id): Path<String>,
) -> Json<Value> {
    storage.workflows.get_workflow(&id)
        .map(|opt| opt
            .map(|w| Json(serde_json::json!(w)))
            .unwrap_or_else(|| not_found("Workflow not found".to_string())))
        .unwrap_or_else(|e| error(format!("Failed to get workflow: {}", e)))
}

// PUT /api/workflow/update/{id}
pub async fn update_workflow(
    State((storage, _, _)): State<AppState>,
    Path(id): Path<String>,
    Json(workflow): Json<Workflow>,
) -> Json<Value> {
    storage.workflows.update_workflow(&id, &workflow)
        .map(|_| success_with_message(
            serde_json::json!({}),
            format!("Workflow {} updated!", id)
        ))
        .unwrap_or_else(|e| error(format!("Failed to update workflow: {}", e)))
}

// DELETE /api/workflow/delete/{id}
pub async fn delete_workflow(
    State((storage, _, _)): State<AppState>,
    Path(id): Path<String>,
) -> Json<Value> {
    storage.workflows.delete_workflow(&id)
        .map(|_| success_with_message(
            serde_json::json!({}),
            format!("Workflow {} deleted!", id)
        ))
        .unwrap_or_else(|e| error(format!("Failed to delete workflow: {}", e)))
}

// POST /api/execution/sync/run
pub async fn execute_workflow(
    State((_, _executor, _)): State<AppState>,
    Json(mut workflow): Json<Workflow>,
) -> Json<Value> {
    workflow.id = format!("inline-{}", uuid::Uuid::new_v4());
    
    let mut wf_executor = WorkflowExecutor::new(workflow);
    match wf_executor.execute().await {
        Ok(output) => success(output),
        Err(e) => error(format!("Workflow execution failed: {}", e)),
    }
}

// POST /api/execution/sync/run-workflow/{workflow_id}
pub async fn execute_workflow_by_id(
    State((storage, _, _)): State<AppState>,
    Path(workflow_id): Path<String>,
    Json(input): Json<Value>,
) -> Json<Value> {
    match storage.workflows.get_workflow(&workflow_id) {
        Ok(Some(workflow)) => {
            let mut wf_executor = WorkflowExecutor::new(workflow);
            wf_executor.set_input(input);
            match wf_executor.execute().await {
                Ok(output) => success(output),
                Err(e) => error(format!("Workflow execution failed: {}", e)),
            }
        }
        Ok(None) => not_found(format!("Workflow {} not found", workflow_id)),
        Err(e) => error(format!("Failed to get workflow: {}", e)),
    }
}

// POST /api/execution/async/submit/{workflow_id}
pub async fn submit_workflow(
    State((_, executor, _)): State<AppState>,
    Path(workflow_id): Path<String>,
    Json(input): Json<Value>,
) -> Json<Value> {
    match executor.submit(workflow_id.clone(), input).await {
        Ok(execution_id) => success(serde_json::json!({
            "execution_id": execution_id,
            "workflow_id": workflow_id,
        })),
        Err(e) => error(format!("Failed to submit workflow: {}", e)),
    }
}