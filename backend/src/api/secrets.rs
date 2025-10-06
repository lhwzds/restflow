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
            let mut secret = Secret::new(
                payload.key,
                String::new(),
                payload.description,
            );
            secret.value = String::new();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppCore;
    use std::sync::Arc;
    use tempfile::{tempdir, TempDir};

    async fn create_test_app() -> (Arc<AppCore>, TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let app = Arc::new(AppCore::new(db_path.to_str().unwrap()).await.unwrap());
        (app, temp_dir)
    }

    #[tokio::test]
    async fn test_list_secrets_empty() {
        let (app, _tmp_dir) = create_test_app().await;

        let result = list_secrets(State(app)).await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert!(response.success);
        assert_eq!(response.data.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_create_secret() {
        let (app, _tmp_dir) = create_test_app().await;

        let request = CreateSecretRequest {
            key: "TEST_API_KEY".to_string(),
            value: "secret_value".to_string(),
            description: Some("Test secret".to_string()),
        };

        let result = create_secret(State(app), Json(request)).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_create_secret_invalid_key() {
        let (app, _tmp_dir) = create_test_app().await;

        let request = CreateSecretRequest {
            key: "invalid-key".to_string(),
            value: "secret_value".to_string(),
            description: None,
        };

        let result = create_secret(State(app), Json(request)).await;

        assert!(result.is_err());
        if let Err((status, _msg)) = result {
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }
    }

    #[tokio::test]
    async fn test_update_secret() {
        let (app, _tmp_dir) = create_test_app().await;

        let create_req = CreateSecretRequest {
            key: "TEST_API_KEY".to_string(),
            value: "secret_value".to_string(),
            description: None,
        };
        create_secret(State(app.clone()), Json(create_req)).await.unwrap();

        let update_req = UpdateSecretRequest {
            value: "new_secret_value".to_string(),
            description: Some("Updated secret".to_string()),
        };

        let result = update_secret(
            State(app),
            Path("TEST_API_KEY".to_string()),
            Json(update_req)
        ).await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert!(response.success);
        assert!(response.message.unwrap().contains("updated"));
    }

    #[tokio::test]
    async fn test_update_nonexistent_secret() {
        let (app, _tmp_dir) = create_test_app().await;

        let update_req = UpdateSecretRequest {
            value: "new_value".to_string(),
            description: None,
        };

        let result = update_secret(
            State(app),
            Path("NONEXISTENT".to_string()),
            Json(update_req)
        ).await;

        assert!(result.is_err());
        if let Err((status, msg)) = result {
            assert_eq!(status, StatusCode::NOT_FOUND);
            assert!(msg.contains("not found"));
        }
    }

    #[tokio::test]
    async fn test_delete_secret() {
        let (app, _tmp_dir) = create_test_app().await;

        let create_req = CreateSecretRequest {
            key: "TEST_API_KEY".to_string(),
            value: "secret_value".to_string(),
            description: None,
        };
        create_secret(State(app.clone()), Json(create_req)).await.unwrap();

        let result = delete_secret(
            State(app.clone()),
            Path("TEST_API_KEY".to_string())
        ).await;

        assert!(result.is_ok());
        let response = result.unwrap().0;
        assert!(response.success);
        assert!(response.message.unwrap().contains("deleted"));

        let list_result = list_secrets(State(app)).await.unwrap().0;
        assert_eq!(list_result.data.unwrap().len(), 0);
    }

    #[test]
    fn test_is_valid_secret_key() {
        assert!(is_valid_secret_key("API_KEY"));
        assert!(is_valid_secret_key("TEST_KEY_123"));
        assert!(is_valid_secret_key("A1_B2_C3"));

        assert!(!is_valid_secret_key(""));
        assert!(!is_valid_secret_key("api_key"));
        assert!(!is_valid_secret_key("API-KEY"));
        assert!(!is_valid_secret_key("API KEY"));
    }
}
