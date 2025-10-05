use crate::{api::{state::AppState, ApiResponse}, models::Secret, services};
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
) -> Result<Json<ApiResponse<Vec<Secret>>>, (StatusCode, String)> {
    match services::secrets::list_secrets(&state).await {
        Ok(secrets) => Ok(Json(ApiResponse::ok(secrets))),
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

    match services::secrets::set_secret(&state, &payload.key, &payload.value, payload.description.clone()).await {
        Ok(_) => {
            // Return newly created secret without the actual value for security
            let mut secret = Secret::new(
                payload.key,
                String::new(),  // Don't return actual value
                payload.description,
            );
            secret.value = String::new();  // Clear value for security
            Ok((StatusCode::CREATED, Json(ApiResponse::ok(secret))))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

/// Update an existing secret
pub async fn update_secret(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(payload): Json<UpdateSecretRequest>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, String)> {
    match services::secrets::has_secret(&state, &key).await {
        Ok(false) => return Err((StatusCode::NOT_FOUND, "Secret not found".to_string())),
        Err(e) => return Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
        _ => {}
    }

    match services::secrets::set_secret(&state, &key, &payload.value, payload.description).await {
        Ok(_) => Ok(Json(ApiResponse::message("Secret updated successfully"))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

/// Delete a secret
pub async fn delete_secret(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, String)> {
    match services::secrets::delete_secret(&state, &key).await {
        Ok(_) => Ok(Json(ApiResponse::message("Secret deleted successfully"))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

/// Validate secret key format (uppercase, numbers, underscores)
fn is_valid_secret_key(key: &str) -> bool {
    !key.is_empty() && key.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}
