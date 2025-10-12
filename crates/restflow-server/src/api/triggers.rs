use crate::api::{ApiResponse, state::AppState};
use axum::{
    Json,
    extract::{Path, State},
    http::Method,
};
use restflow_core::engine::trigger_manager::{TriggerStatus, WebhookResponse};
use restflow_core::models::TriggerConfig;
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
            let trigger_info: Vec<_> = triggers
                .iter()
                .map(|trigger| {
                    let webhook_url =
                        if matches!(trigger.trigger_config, TriggerConfig::Webhook { .. }) {
                            Some(format!("/api/triggers/webhook/{}", trigger.id))
                        } else {
                            None
                        };

                    TriggerInfo {
                        trigger_id: trigger.id.clone(),
                        webhook_url,
                        config: trigger.trigger_config.clone(),
                    }
                })
                .collect();

            Json(ApiResponse::ok_with_message(
                ActivateResponse {
                    count: triggers.len(),
                    triggers: trigger_info,
                },
                format!("{} trigger(s) activated successfully", triggers.len()),
            ))
        }
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to activate workflow: {}",
            e
        ))),
    }
}

// PUT /api/workflows/{id}/deactivate
pub async fn deactivate_workflow(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
) -> Json<ApiResponse<()>> {
    match state
        .trigger_manager
        .deactivate_workflow(&workflow_id)
        .await
    {
        Ok(_) => Json(ApiResponse::message(
            "Workflow trigger deactivated successfully",
        )),
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to deactivate workflow: {}",
            e
        ))),
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
                    "No trigger configured for this workflow",
                ))
            } else {
                Json(ApiResponse::ok(status))
            }
        }
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to get trigger status: {}",
            e
        ))),
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
            "Test execution started",
        )),
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to test workflow: {}",
            e
        ))),
    }
}

pub async fn handle_webhook_trigger(
    State(state): State<AppState>,
    Path(webhook_id): Path<String>,
    method: Method,
    headers: axum::http::HeaderMap,
    body: String,
) -> Json<ApiResponse<Value>> {
    let mut header_map = HashMap::new();
    for (key, value) in headers.iter() {
        if let Ok(v) = value.to_str() {
            header_map.insert(key.as_str().to_string(), v.to_string());
        }
    }

    let body_json: Value =
        serde_json::from_str(&body).unwrap_or_else(|_| serde_json::json!({ "raw": body }));

    match state
        .trigger_manager
        .handle_webhook(&webhook_id, method.as_str(), header_map, body_json)
        .await
    {
        Ok(WebhookResponse::Async { execution_id }) => Json(ApiResponse::ok_with_message(
            serde_json::to_value(WebhookAsyncResponse { execution_id }).unwrap(),
            "Workflow execution started",
        )),
        Ok(WebhookResponse::Sync { result }) => Json(ApiResponse::ok(result)),
        Err(e) => Json(ApiResponse::error(format!("{}", e))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_core::AppCore;
    use restflow_core::models::{Node, NodeType, Workflow};
    use std::sync::Arc;
    use tempfile::{TempDir, tempdir};

    async fn create_test_app() -> (Arc<AppCore>, TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let app = Arc::new(AppCore::new(db_path.to_str().unwrap()).await.unwrap());
        (app, temp_dir)
    }

    fn create_test_workflow_with_webhook_trigger(id: &str) -> Workflow {
        Workflow {
            id: id.to_string(),
            name: format!("Test Workflow {}", id),
            nodes: vec![Node {
                id: "webhook_trigger".to_string(),
                node_type: NodeType::WebhookTrigger,
                config: serde_json::json!({
                    "path": format!("/test/{}", id),
                    "method": "POST"
                }),
                position: None,
            }],
            edges: vec![],
        }
    }

    #[tokio::test]
    async fn test_activate_workflow() {
        let (app, _tmp_dir) = create_test_app().await;
        let workflow = create_test_workflow_with_webhook_trigger("wf-001");

        app.storage.workflows.create_workflow(&workflow).unwrap();

        let response = activate_workflow(State(app), Path("wf-001".to_string())).await;
        let body = response.0;

        assert!(body.success);
        let data = body.data.unwrap();
        assert!(data.count > 0);
    }

    #[tokio::test]
    async fn test_deactivate_workflow() {
        let (app, _tmp_dir) = create_test_app().await;
        let workflow = create_test_workflow_with_webhook_trigger("wf-001");

        app.storage.workflows.create_workflow(&workflow).unwrap();

        let _ = activate_workflow(State(app.clone()), Path("wf-001".to_string())).await;

        let response = deactivate_workflow(State(app), Path("wf-001".to_string())).await;
        let body = response.0;

        assert!(body.success);
    }

    #[tokio::test]
    async fn test_get_workflow_trigger_status() {
        let (app, _tmp_dir) = create_test_app().await;
        let workflow = create_test_workflow_with_webhook_trigger("wf-001");

        app.storage.workflows.create_workflow(&workflow).unwrap();

        let response = get_workflow_trigger_status(State(app), Path("wf-001".to_string())).await;
        let body = response.0;

        assert!(body.success);
    }

    #[tokio::test]
    async fn test_test_workflow_trigger() {
        let (app, _tmp_dir) = create_test_app().await;
        let workflow = create_test_workflow_with_webhook_trigger("wf-001");

        app.storage.workflows.create_workflow(&workflow).unwrap();

        let _ = activate_workflow(State(app.clone()), Path("wf-001".to_string())).await;

        let test_data = serde_json::json!({
            "test": "data"
        });

        let response =
            test_workflow_trigger(State(app), Path("wf-001".to_string()), Json(test_data)).await;
        let body = response.0;

        assert!(body.success);
        assert!(body.data.is_some());
    }
}
