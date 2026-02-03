use crate::daemon::http::ApiError;
use crate::services::config as config_service;
use crate::storage::SystemConfig;
use crate::AppCore;
use axum::{extract::Extension, routing::{get, put}, Json, Router};
use std::sync::Arc;

pub fn router() -> Router {
    Router::new().route("/", get(get_config).put(update_config))
}

async fn get_config(
    Extension(core): Extension<Arc<AppCore>>,
) -> Result<Json<SystemConfig>, ApiError> {
    let config = config_service::get_config(&core).await?;
    Ok(Json(config))
}

async fn update_config(
    Extension(core): Extension<Arc<AppCore>>,
    Json(config): Json<SystemConfig>,
) -> Result<Json<SystemConfig>, ApiError> {
    config_service::update_config(&core, config.clone()).await?;
    Ok(Json(config))
}
