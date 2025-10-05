use crate::api::state::AppState;
use crate::engine::trigger_manager::WebhookResponse;
use axum::{
    extract::{Path, State},
    http::Method,
    Json,
};
use serde_json::Value;
use std::collections::HashMap;

// PUT /api/workflows/{id}/activate
pub async fn activate_workflow(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
) -> Json<Value> {
    match state.trigger_manager.activate_workflow(&workflow_id).await {
        Ok(triggers) => {
            let trigger_info: Vec<_> = triggers.iter().map(|trigger| {
                let webhook_url = if matches!(trigger.trigger_config, crate::models::TriggerConfig::Webhook { .. }) {
                    Some(format!("/api/triggers/webhook/{}", trigger.id))
                } else {
                    None
                };
                
                serde_json::json!({
                    "trigger_id": trigger.id,
                    "webhook_url": webhook_url,
                    "config": trigger.trigger_config
                })
            }).collect();
            
            Json(serde_json::json!({
                "status": "success",
                "triggers": trigger_info,
                "count": triggers.len(),
                "message": format!("{} trigger(s) activated successfully", triggers.len())
            }))
        }
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to activate workflow: {}", e)
        })),
    }
}

// PUT /api/workflows/{id}/deactivate
pub async fn deactivate_workflow(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
) -> Json<Value> {
    match state.trigger_manager.deactivate_workflow(&workflow_id).await {
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

// GET /api/workflows/{id}/trigger-status
pub async fn get_workflow_trigger_status(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
) -> Json<Value> {
    match state.trigger_manager.get_trigger_status(&workflow_id).await {
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

// POST /api/workflows/{id}/test
pub async fn test_workflow_trigger(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
    Json(payload): Json<Value>,
) -> Json<Value> {
    match state.executor.submit(workflow_id, payload).await {
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
    State(state): State<AppState>,
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
    
    match state.trigger_manager.handle_webhook(&webhook_id, method.as_str(), header_map, body_json).await {
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