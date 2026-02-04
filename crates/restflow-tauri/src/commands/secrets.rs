//! Secret management Tauri commands

use crate::state::AppState;
use serde::Deserialize;
use tauri::State;

/// List all secrets (without values)
#[tauri::command]
pub async fn list_secrets(state: State<'_, AppState>) -> Result<Vec<SecretInfo>, String> {
    let secrets = state
        .executor()
        .list_secrets()
        .await
        .map_err(|e| e.to_string())?;

    // Return secrets without actual values for security (already cleared by storage)
    Ok(secrets
        .into_iter()
        .map(|s| SecretInfo {
            key: s.key,
            description: s.description,
            created_at: s.created_at,
            updated_at: s.updated_at,
        })
        .collect())
}

/// Secret info without the actual value
#[derive(serde::Serialize)]
pub struct SecretInfo {
    pub key: String,
    pub description: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Create secret request
#[derive(Debug, Deserialize)]
pub struct CreateSecretRequest {
    pub key: String,
    pub value: String,
    pub description: Option<String>,
}

/// Create a new secret
#[tauri::command]
pub async fn create_secret(
    state: State<'_, AppState>,
    request: CreateSecretRequest,
) -> Result<SecretInfo, String> {
    // Check if secret already exists
    let existing = state
        .executor()
        .get_secret(request.key.clone())
        .await
        .map_err(|e| e.to_string())?;

    if existing.is_some() {
        return Err(format!("Secret '{}' already exists", request.key));
    }

    state
        .executor()
        .set_secret(request.key.clone(), request.value, request.description.clone())
        .await
        .map_err(|e| e.to_string())?;

    let now = chrono::Utc::now().timestamp_millis();
    Ok(SecretInfo {
        key: request.key,
        description: request.description,
        created_at: now,
        updated_at: now,
    })
}

/// Update secret request
#[derive(Debug, Deserialize)]
pub struct UpdateSecretRequest {
    pub value: String,
    pub description: Option<String>,
}

/// Update an existing secret
#[tauri::command]
pub async fn update_secret(
    state: State<'_, AppState>,
    key: String,
    request: UpdateSecretRequest,
) -> Result<SecretInfo, String> {
    // Check if secret exists
    let existing = state
        .executor()
        .get_secret(key.clone())
        .await
        .map_err(|e| e.to_string())?;

    if existing.is_none() {
        return Err(format!("Secret '{}' not found", key));
    }

    state
        .executor()
        .set_secret(key.clone(), request.value, request.description.clone())
        .await
        .map_err(|e| e.to_string())?;

    // Get the updated secret info from list
    let secrets = state
        .executor()
        .list_secrets()
        .await
        .map_err(|e| e.to_string())?;

    let secret = secrets
        .into_iter()
        .find(|s| s.key == key)
        .ok_or_else(|| format!("Secret '{}' not found after update", key))?;

    Ok(SecretInfo {
        key: secret.key,
        description: secret.description,
        created_at: secret.created_at,
        updated_at: secret.updated_at,
    })
}

/// Delete a secret by key
#[tauri::command]
pub async fn delete_secret(state: State<'_, AppState>, key: String) -> Result<(), String> {
    state
        .executor()
        .delete_secret(key)
        .await
        .map_err(|e| e.to_string())
}

/// Check if a secret exists
#[tauri::command]
pub async fn has_secret(state: State<'_, AppState>, key: String) -> Result<bool, String> {
    let secret = state
        .executor()
        .get_secret(key)
        .await
        .map_err(|e| e.to_string())?;

    Ok(secret.is_some())
}
