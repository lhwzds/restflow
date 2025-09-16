use crate::{api::state::AppState, services};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateSecretRequest {
    pub key: String,
    pub value: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateSecretRequest {
    pub value: String,
    pub description: Option<String>,
}

/// List all secrets (keys only, no values)
pub async fn list_secrets(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    match services::secrets::list_secrets(&state).await {
        Ok(secrets) => Ok(Json(secrets)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

/// Create a new secret
pub async fn create_secret(
    State(state): State<AppState>,
    Json(payload): Json<CreateSecretRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if !is_valid_secret_key(&payload.key) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid key format. Use uppercase letters, numbers, and underscores only.".to_string(),
        ));
    }

    match services::secrets::set_secret(&state, &payload.key, &payload.value, payload.description).await {
        Ok(_) => Ok((StatusCode::CREATED, "Secret created successfully")),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

/// Update an existing secret
pub async fn update_secret(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(payload): Json<UpdateSecretRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    match services::secrets::has_secret(&state, &key).await {
        Ok(false) => return Err((StatusCode::NOT_FOUND, "Secret not found".to_string())),
        Err(e) => return Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
        _ => {}
    }

    match services::secrets::set_secret(&state, &key, &payload.value, payload.description).await {
        Ok(_) => Ok("Secret updated successfully"),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

/// Delete a secret
pub async fn delete_secret(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    match services::secrets::delete_secret(&state, &key).await {
        Ok(_) => Ok("Secret deleted successfully"),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

/// Validate secret key format (uppercase, numbers, underscores)
fn is_valid_secret_key(key: &str) -> bool {
    !key.is_empty() && key.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}