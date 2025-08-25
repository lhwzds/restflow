use crate::api::state::AppState;
use crate::storage::SystemConfig;
use axum::{
    extract::State,
    Json,
};
use serde_json::Value;

// GET /api/config
pub async fn get_config(
    State(state): State<AppState>,
) -> Json<Value> {
    match state.storage.config.get_config() {
        Ok(Some(config)) => Json(serde_json::json!({
            "status": "success",
            "data": config
        })),
        Ok(None) => Json(serde_json::json!({
            "status": "success",
            "data": SystemConfig::default()
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to get config: {}", e)
        })),
    }
}

// PUT /api/config
pub async fn update_config(
    State(state): State<AppState>,
    Json(config): Json<SystemConfig>,
) -> Json<Value> {
    match state.storage.config.update_config(config) {
        Ok(_) => Json(serde_json::json!({
            "status": "success",
            "message": "Configuration updated successfully"
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to update config: {}", e)
        })),
    }
}