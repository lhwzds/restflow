//! Hook management Tauri commands.

use crate::state::AppState;
use restflow_core::models::Hook;
use tauri::State;

/// List all hooks.
#[tauri::command]
pub async fn list_hooks(state: State<'_, AppState>) -> Result<Vec<Hook>, String> {
    state
        .executor()
        .list_hooks()
        .await
        .map_err(|e| e.to_string())
}

/// Create a new hook.
#[tauri::command]
pub async fn create_hook(state: State<'_, AppState>, mut hook: Hook) -> Result<Hook, String> {
    let now = chrono::Utc::now().timestamp_millis();

    if hook.id.trim().is_empty() {
        hook.id = uuid::Uuid::new_v4().to_string();
    }
    if hook.created_at <= 0 {
        hook.created_at = now;
    }
    hook.updated_at = now;

    state
        .executor()
        .create_hook(hook)
        .await
        .map_err(|e| e.to_string())
}

/// Update an existing hook.
#[tauri::command]
pub async fn update_hook(
    state: State<'_, AppState>,
    id: String,
    mut hook: Hook,
) -> Result<Hook, String> {
    let existing = find_hook_by_id(&state, &id).await?;

    hook.id = id.clone();
    hook.created_at = existing.created_at;
    hook.updated_at = chrono::Utc::now().timestamp_millis();

    state
        .executor()
        .update_hook(id, hook)
        .await
        .map_err(|e| e.to_string())
}

/// Delete a hook.
#[tauri::command]
pub async fn delete_hook(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    state
        .executor()
        .delete_hook(id)
        .await
        .map_err(|e| e.to_string())
}

/// Execute a hook once with synthetic context for verification.
#[tauri::command]
pub async fn test_hook(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .executor()
        .test_hook(id)
        .await
        .map_err(|e| e.to_string())
}

async fn find_hook_by_id(state: &AppState, id: &str) -> Result<Hook, String> {
    state
        .executor()
        .list_hooks()
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|hook| hook.id == id)
        .ok_or_else(|| format!("Hook '{}' not found", id))
}
