//! System configuration Tauri commands

use crate::state::AppState;
use restflow_core::AIModel;
use restflow_core::models::{ModelMetadataDTO, Provider};
use restflow_storage::SystemConfig;
use serde::Serialize;
use specta::Type;
use tauri::State;

/// Get system configuration
#[specta::specta]
#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<SystemConfig, String> {
    state
        .executor()
        .get_config()
        .await
        .map_err(|e| e.to_string())
}

/// Update system configuration
#[specta::specta]
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

    let updated = state
        .executor()
        .get_config()
        .await
        .map_err(|e| e.to_string())?;
    state.refresh_subagent_config(&updated);

    Ok(updated)
}

/// Get available AI models with metadata
#[specta::specta]
#[tauri::command]
pub async fn get_available_models() -> Result<Vec<ModelMetadataDTO>, String> {
    Ok(AIModel::all_with_metadata())
}

/// Catalog entry for provider-scoped model discovery.
#[derive(Debug, Serialize, Type)]
pub struct ProviderModelCatalogItem {
    pub provider: Provider,
    pub models: Vec<ModelMetadataDTO>,
}

/// Get all providers that currently have at least one available model.
#[specta::specta]
#[tauri::command]
pub async fn get_available_providers() -> Result<Vec<Provider>, String> {
    let metadata = AIModel::all_with_metadata();
    let mut providers: Vec<Provider> = metadata.iter().map(|entry| entry.provider).collect();
    providers.sort_by_key(|provider| provider.as_canonical_str());
    providers.dedup();
    Ok(providers)
}

/// Get provider -> models catalog for UI selector workflows.
#[specta::specta]
#[tauri::command]
pub async fn get_model_catalog() -> Result<Vec<ProviderModelCatalogItem>, String> {
    let metadata = AIModel::all_with_metadata();
    let mut providers: Vec<Provider> = metadata.iter().map(|entry| entry.provider).collect();
    providers.sort_by_key(|provider| provider.as_canonical_str());
    providers.dedup();

    let catalog = providers
        .into_iter()
        .map(|provider| ProviderModelCatalogItem {
            provider,
            models: metadata
                .iter()
                .filter(|entry| entry.provider == provider)
                .cloned()
                .collect(),
        })
        .collect();

    Ok(catalog)
}

/// Tool information for the frontend
#[derive(Debug, Serialize, Type)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
}

/// Get available tools for agents
#[specta::specta]
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
