//! Webhook HTTP Handler
//!
//! This module provides the HTTP handlers for webhook endpoints that allow
//! external systems to trigger agent task executions.

use axum::{
    Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
    routing::{get, post},
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use restflow_core::models::{WebhookRateLimiter, WebhookRequest, WebhookResponse};
use restflow_core::storage::BackgroundAgentStorage;

/// Shared state for webhook handlers
pub struct WebhookState {
    /// Storage for agent tasks
    pub storage: Arc<BackgroundAgentStorage>,
    /// Rate limiter for webhook requests
    pub rate_limiter: Arc<RwLock<WebhookRateLimiter>>,
    /// Callback to trigger task execution (returns run_id)
    pub trigger_callback: Arc<dyn Fn(String, Option<String>) -> String + Send + Sync>,
}

impl Clone for WebhookState {
    fn clone(&self) -> Self {
        Self {
            storage: Arc::clone(&self.storage),
            rate_limiter: Arc::clone(&self.rate_limiter),
            trigger_callback: Arc::clone(&self.trigger_callback),
        }
    }
}

impl WebhookState {
    /// Create a new webhook state
    pub fn new(
        storage: Arc<BackgroundAgentStorage>,
        trigger_callback: impl Fn(String, Option<String>) -> String + Send + Sync + 'static,
    ) -> Self {
        Self {
            storage,
            rate_limiter: Arc::new(RwLock::new(WebhookRateLimiter::new())),
            trigger_callback: Arc::new(trigger_callback),
        }
    }
}

/// Create the webhook router
pub fn webhook_router(state: WebhookState) -> Router {
    Router::new()
        .route("/hooks/trigger/{task_id}", post(trigger_task))
        .route("/hooks/health", get(health_check))
        .with_state(state)
}

/// Health check endpoint
async fn health_check() -> &'static str {
    "OK"
}

/// Trigger a task via webhook
async fn trigger_task(
    State(state): State<WebhookState>,
    Path(task_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<WebhookRequest>,
) -> Result<Json<WebhookResponse>, (StatusCode, Json<WebhookResponse>)> {
    info!(task_id = %task_id, source = ?request.source, "Webhook trigger received");

    // 1. Load task
    let task = match state.storage.get_task(&task_id) {
        Ok(Some(task)) => task,
        Ok(None) => {
            warn!(task_id = %task_id, "Background agent not found");
            return Err((
                StatusCode::NOT_FOUND,
                Json(WebhookResponse::error("Background agent not found")),
            ));
        }
        Err(e) => {
            warn!(task_id = %task_id, error = %e, "Failed to load task");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(WebhookResponse::error(format!("Storage error: {}", e))),
            ));
        }
    };

    // 2. Check webhook configuration
    let webhook = match &task.webhook {
        Some(w) if w.enabled => w,
        Some(_) => {
            warn!(task_id = %task_id, "Webhook disabled for task");
            return Err((
                StatusCode::FORBIDDEN,
                Json(WebhookResponse::error("Webhook disabled for this task")),
            ));
        }
        None => {
            warn!(task_id = %task_id, "No webhook configured for task");
            return Err((
                StatusCode::FORBIDDEN,
                Json(WebhookResponse::error(
                    "Webhook not configured for this task",
                )),
            ));
        }
    };

    // 3. Validate authorization token
    let auth_header = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let expected = format!("Bearer {}", webhook.token);
    if auth_header != expected {
        warn!(task_id = %task_id, "Invalid authorization token");
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(WebhookResponse::error("Invalid authorization token")),
        ));
    }

    // 4. Check rate limit
    if let Some(limit) = webhook.rate_limit {
        let mut limiter = state.rate_limiter.write().await;
        if !limiter.check_and_record(&task_id, limit) {
            warn!(task_id = %task_id, limit = limit, "Rate limit exceeded");
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                Json(WebhookResponse::error(format!(
                    "Rate limit exceeded ({} requests per minute)",
                    limit
                ))),
            ));
        }
    }

    // 5. Trigger task execution
    let input = request.input.or(task.input.clone());
    let run_id = (state.trigger_callback)(task_id.clone(), input);

    info!(
        task_id = %task_id,
        run_id = %run_id,
        source = ?request.source,
        "Task triggered via webhook"
    );

    Ok(Json(WebhookResponse::success(task_id, run_id)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use restflow_core::models::{BackgroundAgent, TaskSchedule, WebhookConfig};
    use tempfile::TempDir;
    use tower::ServiceExt;

    fn create_test_storage() -> (Arc<BackgroundAgentStorage>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = std::sync::Arc::new(redb::Database::create(db_path).unwrap());
        (Arc::new(BackgroundAgentStorage::new(db).unwrap()), temp_dir)
    }

    fn create_test_task(id: &str, webhook_enabled: bool, token: &str) -> BackgroundAgent {
        let mut task = BackgroundAgent::new(
            id.to_string(),
            "Test Task".to_string(),
            "agent-1".to_string(),
            TaskSchedule::default(),
        );
        task.webhook = Some(WebhookConfig {
            enabled: webhook_enabled,
            token: token.to_string(),
            rate_limit: Some(60),
        });
        task
    }

    fn create_test_state(storage: Arc<BackgroundAgentStorage>) -> WebhookState {
        WebhookState::new(storage, |task_id, _input| format!("run-{}", task_id))
    }

    #[tokio::test]
    async fn test_health_check() {
        let (storage, _tmp) = create_test_storage();
        let state = create_test_state(storage);
        let app = webhook_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/hooks/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_trigger_task_success() {
        let (storage, _tmp) = create_test_storage();
        let task = create_test_task("task-1", true, "secret-token");
        storage.save_task(&task).unwrap();

        let state = create_test_state(storage);
        let app = webhook_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/hooks/trigger/task-1")
                    .header("Authorization", "Bearer secret-token")
                    .header("Content-Type", "application/json")
                    .body(Body::from(r#"{"source": "test"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let resp: WebhookResponse = serde_json::from_slice(&body).unwrap();
        assert!(resp.accepted);
        assert_eq!(resp.task_id, Some("task-1".to_string()));
    }

    #[tokio::test]
    async fn test_trigger_task_not_found() {
        let (storage, _tmp) = create_test_storage();
        let state = create_test_state(storage);
        let app = webhook_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/hooks/trigger/nonexistent")
                    .header("Authorization", "Bearer token")
                    .header("Content-Type", "application/json")
                    .body(Body::from(r#"{}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_trigger_task_unauthorized() {
        let (storage, _tmp) = create_test_storage();
        let task = create_test_task("task-1", true, "secret-token");
        storage.save_task(&task).unwrap();

        let state = create_test_state(storage);
        let app = webhook_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/hooks/trigger/task-1")
                    .header("Authorization", "Bearer wrong-token")
                    .header("Content-Type", "application/json")
                    .body(Body::from(r#"{}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_trigger_task_webhook_disabled() {
        let (storage, _tmp) = create_test_storage();
        let task = create_test_task("task-1", false, "secret-token");
        storage.save_task(&task).unwrap();

        let state = create_test_state(storage);
        let app = webhook_router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/hooks/trigger/task-1")
                    .header("Authorization", "Bearer secret-token")
                    .header("Content-Type", "application/json")
                    .body(Body::from(r#"{}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_trigger_task_rate_limited() {
        let (storage, _tmp) = create_test_storage();
        let mut task = create_test_task("task-1", true, "secret-token");
        task.webhook.as_mut().unwrap().rate_limit = Some(1);
        storage.save_task(&task).unwrap();

        let state = create_test_state(Arc::clone(&storage));
        let app = webhook_router(state);

        // First request should succeed
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/hooks/trigger/task-1")
                    .header("Authorization", "Bearer secret-token")
                    .header("Content-Type", "application/json")
                    .body(Body::from(r#"{}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Second request should be rate limited
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/hooks/trigger/task-1")
                    .header("Authorization", "Bearer secret-token")
                    .header("Content-Type", "application/json")
                    .body(Body::from(r#"{}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }
}
