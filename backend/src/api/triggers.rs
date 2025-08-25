use crate::engine::trigger_manager::{TriggerManager, WebhookResponse};
use crate::storage::Storage;
use crate::engine::executor::AsyncWorkflowExecutor;
use axum::{
    extract::{Path, State},
    http::Method,
    Json,
};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

pub type AppState = (Arc<Storage>, Arc<AsyncWorkflowExecutor>, Arc<TriggerManager>);

// PUT /api/workflow/{id}/activate
pub async fn activate_workflow(
    State((_, _, trigger_manager)): State<AppState>,
    Path(workflow_id): Path<String>,
) -> Json<Value> {
    match trigger_manager.activate_workflow(&workflow_id).await {
        Ok(trigger) => {
            let webhook_url = if matches!(trigger.trigger_config, crate::models::TriggerConfig::Webhook { .. }) {
                Some(format!("/api/triggers/webhook/{}", trigger.id))
            } else {
                None
            };
            
            Json(serde_json::json!({
                "status": "success",
                "trigger_id": trigger.id,
                "webhook_url": webhook_url,
                "message": "Workflow trigger activated successfully"
            }))
        }
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to activate workflow: {}", e)
        })),
    }
}

// PUT /api/workflow/{id}/deactivate
pub async fn deactivate_workflow(
    State((_, _, trigger_manager)): State<AppState>,
    Path(workflow_id): Path<String>,
) -> Json<Value> {
    match trigger_manager.deactivate_workflow(&workflow_id).await {
        Ok(_) => Json(serde_json::json!({
            "status": "success",
            "message": "Workflow trigger deactivated successfully"
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to deactivate workflow: {}", e)
        })),
    }
}

// GET /api/workflow/{id}/trigger-status
pub async fn get_workflow_trigger_status(
    State((_, _, trigger_manager)): State<AppState>,
    Path(workflow_id): Path<String>,
) -> Json<Value> {
    match trigger_manager.get_trigger_status(&workflow_id).await {
        Ok(Some(status)) => Json(serde_json::json!({
            "status": "success",
            "data": status
        })),
        Ok(None) => Json(serde_json::json!({
            "status": "success",
            "data": null,
            "message": "No trigger configured for this workflow"
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to get trigger status: {}", e)
        })),
    }
}

// POST /api/workflow/{id}/test
pub async fn test_workflow_trigger(
    State((_, executor, _)): State<AppState>,
    Path(workflow_id): Path<String>,
    Json(payload): Json<Value>,
) -> Json<Value> {
    match executor.submit(workflow_id, payload).await {
        Ok(execution_id) => Json(serde_json::json!({
            "status": "success",
            "execution_id": execution_id,
            "message": "Test execution started"
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to test workflow: {}", e)
        })),
    }
}

// Webhook handler - matches any HTTP method
pub async fn handle_webhook_trigger(
    State((_, _, trigger_manager)): State<AppState>,
    Path(webhook_id): Path<String>,
    method: Method,
    headers: axum::http::HeaderMap,
    body: String,
) -> Json<Value> {
    // Convert headers to HashMap
    let mut header_map = HashMap::new();
    for (key, value) in headers.iter() {
        if let Ok(v) = value.to_str() {
            header_map.insert(key.as_str().to_string(), v.to_string());
        }
    }
    
    // Parse body as JSON, or wrap in object if not valid JSON
    let body_json: Value = serde_json::from_str(&body)
        .unwrap_or_else(|_| serde_json::json!({ "raw": body }));
    
    match trigger_manager.handle_webhook(&webhook_id, method.as_str(), header_map, body_json).await {
        Ok(WebhookResponse::Async { execution_id }) => {
            Json(serde_json::json!({
                "status": "success",
                "execution_id": execution_id,
                "message": "Workflow execution started"
            }))
        }
        Ok(WebhookResponse::Sync { result }) => {
            Json(result)
        }
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("{}", e)
        })),
    }
}