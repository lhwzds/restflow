//! System configuration Tauri commands

use crate::state::AppState;
use restflow_core::AIModel;
use restflow_core::models::ModelMetadataDTO;
use restflow_storage::SystemConfig;
use serde::Serialize;
use tauri::State;

/// Get system configuration
#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<SystemConfig, String> {
    state
        .executor()
        .get_config()
        .await
        .map_err(|e| e.to_string())
}

/// Update system configuration
#[tauri::command]
pub async fn update_config(
    state: State<'_, AppState>,
    config: SystemConfig,
) -> Result<SystemConfig, String> {
    state
        .executor()
        .set_config(config)
        .await
        .map_err(|e| e.to_string())?;

    // Return the updated config
    state
        .executor()
        .get_config()
        .await
        .map_err(|e| e.to_string())
}

/// Get available AI models with metadata
#[tauri::command]
pub async fn get_available_models() -> Result<Vec<ModelMetadataDTO>, String> {
    Ok(AIModel::all_with_metadata())
}

/// Tool information for the frontend
#[derive(Debug, Serialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
}

/// Get available tools for agents
#[tauri::command]
pub async fn get_available_tools(state: State<'_, AppState>) -> Result<Vec<ToolInfo>, String> {
    let tool_names = state
        .executor()
        .get_available_tools()
        .await
        .map_err(|e| e.to_string())?;

    Ok(tool_names
        .into_iter()
        .map(|name| ToolInfo {
            description: format!("Tool: {}", name),
            name,
        })
        .collect())
}
