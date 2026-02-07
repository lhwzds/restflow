//! Hook management Tauri commands.

use crate::state::AppState;
use restflow_core::hooks::{AgentTaskHookScheduler, HookExecutor};
use restflow_core::models::{Hook, HookContext, HookEvent};
use std::sync::Arc;
use tauri::State;

/// List all hooks.
#[tauri::command]
pub async fn list_hooks(state: State<'_, AppState>) -> Result<Vec<Hook>, String> {
    let core = state.core.as_ref().ok_or("AppCore not available")?;
    core.storage.hooks.list().map_err(|e| e.to_string())
}

/// Create a new hook.
#[tauri::command]
pub async fn create_hook(state: State<'_, AppState>, mut hook: Hook) -> Result<Hook, String> {
    let core = state.core.as_ref().ok_or("AppCore not available")?;
    let now = chrono::Utc::now().timestamp_millis();

    if hook.id.trim().is_empty() {
        hook.id = uuid::Uuid::new_v4().to_string();
    }
    if hook.created_at <= 0 {
        hook.created_at = now;
    }
    hook.updated_at = now;

    core.storage
        .hooks
        .create(&hook)
        .map_err(|e| e.to_string())?;
    Ok(hook)
}

/// Update an existing hook.
#[tauri::command]
pub async fn update_hook(
    state: State<'_, AppState>,
    id: String,
    mut hook: Hook,
) -> Result<Hook, String> {
    let core = state.core.as_ref().ok_or("AppCore not available")?;

    let existing = core
        .storage
        .hooks
        .get(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Hook '{}' not found", id))?;

    hook.id = id.clone();
    hook.created_at = existing.created_at;
    hook.updated_at = chrono::Utc::now().timestamp_millis();

    core.storage
        .hooks
        .update(&id, &hook)
        .map_err(|e| e.to_string())?;

    Ok(hook)
}

/// Delete a hook.
#[tauri::command]
pub async fn delete_hook(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let core = state.core.as_ref().ok_or("AppCore not available")?;
    core.storage.hooks.delete(&id).map_err(|e| e.to_string())
}

/// Execute a hook once with synthetic context for verification.
#[tauri::command]
pub async fn test_hook(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let core = state.core.as_ref().ok_or("AppCore not available")?;

    let hook = core
        .storage
        .hooks
        .get(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Hook '{}' not found", id))?;

    let scheduler = Arc::new(AgentTaskHookScheduler::new(
        core.storage.agent_tasks.clone(),
    ));
    let executor = HookExecutor::new(Vec::new())
        .with_channel_router(state.channel_router())
        .with_task_scheduler(scheduler);

    let context = sample_context(&hook.event);
    executor
        .execute_hook(&hook, &context)
        .await
        .map_err(|e| e.to_string())
}

fn sample_context(event: &HookEvent) -> HookContext {
    let now = chrono::Utc::now().timestamp_millis();

    match event {
        HookEvent::TaskFailed | HookEvent::TaskCancelled => HookContext {
            event: event.clone(),
            task_id: "hook-test-task".to_string(),
            task_name: "hook test task".to_string(),
            agent_id: "hook-test-agent".to_string(),
            success: Some(false),
            output: None,
            error: Some("Hook test error".to_string()),
            duration_ms: Some(321),
            timestamp: now,
        },
        _ => HookContext {
            event: event.clone(),
            task_id: "hook-test-task".to_string(),
            task_name: "hook test task".to_string(),
            agent_id: "hook-test-agent".to_string(),
            success: Some(true),
            output: Some("Hook test output".to_string()),
            error: None,
            duration_ms: Some(321),
            timestamp: now,
        },
    }
}
