use crate::api_response::{error, success};
use crate::engine::executor::AsyncWorkflowExecutor;
use crate::engine::trigger_manager::TriggerManager;
use crate::storage::{Storage, SystemConfig};
use axum::{
    extract::State,
    Json,
};
use serde_json::Value;
use std::sync::Arc;

pub type AppState = (Arc<Storage>, Arc<AsyncWorkflowExecutor>, Arc<TriggerManager>);

// GET /api/config
pub async fn get_config(
    State((storage, _, _)): State<AppState>,
) -> Json<Value> {
    match storage.config.get_config() {
        Ok(Some(config)) => success(config),
        Ok(None) => success(SystemConfig::default()),
        Err(e) => error(format!("Failed to get config: {}", e)),
    }
}

// PUT /api/config
pub async fn update_config(
    State((storage, _, _)): State<AppState>,
    Json(config): Json<SystemConfig>,
) -> Json<Value> {
    match storage.config.update_config(config) {
        Ok(_) => Json(serde_json::json!({
            "status": "success",
            "message": "Configuration updated successfully"
        })),
        Err(e) => error(format!("Failed to update config: {}", e)),
    }
}