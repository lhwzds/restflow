use crate::api::{ApiResponse, state::AppState};
use axum::{Json, extract::State};
use restflow_workflow::storage::SystemConfig;

// GET /api/config
pub async fn get_config(State(state): State<AppState>) -> Json<ApiResponse<SystemConfig>> {
    match state.storage.config.get_config() {
        Ok(Some(config)) => Json(ApiResponse::ok(config)),
        Ok(None) => Json(ApiResponse::ok(SystemConfig::default())),
        Err(e) => Json(ApiResponse::error(format!("Failed to get config: {}", e))),
    }
}

// PUT /api/config
pub async fn update_config(
    State(state): State<AppState>,
    Json(config): Json<SystemConfig>,
) -> Json<ApiResponse<()>> {
    match state.storage.config.update_config(config) {
        Ok(_) => Json(ApiResponse::message("Configuration updated successfully")),
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to update config: {}",
            e
        ))),
    }
}
