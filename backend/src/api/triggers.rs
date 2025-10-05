use crate::api::{state::AppState, ApiResponse};
use crate::engine::trigger_manager::{TriggerStatus, WebhookResponse};
use crate::models::TriggerConfig;
use axum::{
    extract::{Path, State},
    http::Method,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
pub struct TriggerInfo {
    pub trigger_id: String,
    pub webhook_url: Option<String>,
    pub config: TriggerConfig,
}

#[derive(Serialize, Deserialize)]
pub struct ActivateResponse {
    pub triggers: Vec<TriggerInfo>,
    pub count: usize,
}

#[derive(Serialize, Deserialize)]
pub struct TestTriggerResponse {
    pub execution_id: String,
}

#[derive(Serialize, Deserialize)]
pub struct WebhookAsyncResponse {
    pub execution_id: String,
}

// PUT /api/workflows/{id}/activate
pub async fn activate_workflow(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
) -> Json<ApiResponse<ActivateResponse>> {
    match state.trigger_manager.activate_workflow(&workflow_id).await {
        Ok(triggers) => {
            let trigger_info: Vec<_> = triggers.iter().map(|trigger| {
                let webhook_url = if matches!(trigger.trigger_config, TriggerConfig::Webhook { .. }) {
                    Some(format!("/api/triggers/webhook/{}", trigger.id))
                } else {
                    None
                };

                TriggerInfo {
                    trigger_id: trigger.id.clone(),
                    webhook_url,
                    config: trigger.trigger_config.clone(),
                }
            }).collect();

            Json(ApiResponse::ok_with_message(
                ActivateResponse {
                    count: triggers.len(),
                    triggers: trigger_info,
                },
                format!("{} trigger(s) activated successfully", triggers.len())
            ))
        }
        Err(e) => Json(ApiResponse::error(format!("Failed to activate workflow: {}", e))),
    }
}

// PUT /api/workflows/{id}/deactivate
pub async fn deactivate_workflow(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
) -> Json<ApiResponse<()>> {
    match state.trigger_manager.deactivate_workflow(&workflow_id).await {
        Ok(_) => Json(ApiResponse::message("Workflow trigger deactivated successfully")),
        Err(e) => Json(ApiResponse::error(format!("Failed to deactivate workflow: {}", e))),
    }
}

// GET /api/workflows/{id}/trigger-status
pub async fn get_workflow_trigger_status(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
) -> Json<ApiResponse<Option<TriggerStatus>>> {
    match state.trigger_manager.get_trigger_status(&workflow_id).await {
        Ok(status) => {
            if status.is_none() {
                Json(ApiResponse::ok_with_message(
                    None,
                    "No trigger configured for this workflow"
                ))
            } else {
                Json(ApiResponse::ok(status))
            }
        }
        Err(e) => Json(ApiResponse::error(format!("Failed to get trigger status: {}", e))),
    }
}

// POST /api/workflows/{id}/test
pub async fn test_workflow_trigger(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
    Json(payload): Json<Value>,
) -> Json<ApiResponse<TestTriggerResponse>> {
    match state.executor.submit(workflow_id, payload).await {
        Ok(execution_id) => Json(ApiResponse::ok_with_message(
            TestTriggerResponse { execution_id },
            "Test execution started"
        )),
        Err(e) => Json(ApiResponse::error(format!("Failed to test workflow: {}", e))),
    }
}

// Webhook handler - matches any HTTP method
pub async fn handle_webhook_trigger(
    State(state): State<AppState>,
    Path(webhook_id): Path<String>,
    method: Method,
    headers: axum::http::HeaderMap,
    body: String,
) -> Json<ApiResponse<Value>> {
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
            Json(ApiResponse::ok_with_message(
                serde_json::to_value(WebhookAsyncResponse { execution_id }).unwrap(),
                "Workflow execution started"
            ))
        }
        Ok(WebhookResponse::Sync { result }) => {
            Json(ApiResponse::ok(result))
        }
        Err(e) => Json(ApiResponse::error(format!("{}", e))),
    }
}
