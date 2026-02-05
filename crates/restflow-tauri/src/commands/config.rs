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

/// Check Python runtime status
#[tauri::command]
pub async fn check_python_status(state: State<'_, AppState>) -> Result<PythonStatus, String> {
    let info = state
        .executor()
        .get_system_info()
        .await
        .map_err(|e| e.to_string())?;
    let is_ready = info
        .get("python_ready")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    if is_ready {
        Ok(PythonStatus {
            ready: true,
            message: "Python runtime is ready".to_string(),
        })
    } else {
        Ok(PythonStatus {
            ready: false,
            message: "Python runtime not initialized".to_string(),
        })
    }
}

/// Initialize Python runtime
#[tauri::command]
pub async fn init_python(state: State<'_, AppState>) -> Result<PythonStatus, String> {
    match state.executor().init_python().await {
        Ok(true) => Ok(PythonStatus {
            ready: true,
            message: "Python runtime initialized successfully".to_string(),
        }),
        Ok(false) => Ok(PythonStatus {
            ready: false,
            message: "Python runtime not initialized".to_string(),
        }),
        Err(e) => Ok(PythonStatus {
            ready: false,
            message: format!("Failed to initialize Python: {}", e),
        }),
    }
}

#[derive(serde::Serialize)]
pub struct PythonStatus {
    pub ready: bool,
    pub message: String,
}
