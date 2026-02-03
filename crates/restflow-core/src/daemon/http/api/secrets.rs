use crate::daemon::http::ApiError;
use crate::services::secrets as secrets_service;
use crate::AppCore;
use axum::{
    extract::{Extension, Path},
    routing::{delete, get, put},
    Json, Router,
};
use std::sync::Arc;

pub fn router() -> Router {
    Router::new()
        .route("/", get(list_secrets))
        .route("/:key", put(set_secret).delete(delete_secret))
}

async fn list_secrets(
    Extension(core): Extension<Arc<AppCore>>,
) -> Result<Json<Vec<crate::models::Secret>>, ApiError> {
    let secrets = secrets_service::list_secrets(&core).await?;
    Ok(Json(secrets))
}

#[derive(Debug, serde::Deserialize)]
struct SetSecretRequest {
    value: String,
    description: Option<String>,
}

async fn set_secret(
    Extension(core): Extension<Arc<AppCore>>,
    Path(key): Path<String>,
    Json(req): Json<SetSecretRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    secrets_service::set_secret(&core, &key, &req.value, req.description).await?;
    Ok(Json(serde_json::json!({ "ok": true, "key": key })))
}

async fn delete_secret(
    Extension(core): Extension<Arc<AppCore>>,
    Path(key): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    secrets_service::delete_secret(&core, &key).await?;
    Ok(Json(serde_json::json!({ "deleted": true, "key": key })))
}
