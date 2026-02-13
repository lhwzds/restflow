//! Agent-related Tauri commands

use crate::state::AppState;
use restflow_core::models::AgentNode;
use restflow_core::storage::agent::StoredAgent;
use serde::Deserialize;
use tauri::State;

/// List all agents
#[tauri::command]
pub async fn list_agents(state: State<'_, AppState>) -> Result<Vec<StoredAgent>, String> {
    state
        .executor()
        .list_agents()
        .await
        .map_err(|e| e.to_string())
}

/// Get an agent by ID
#[tauri::command]
pub async fn get_agent(state: State<'_, AppState>, id: String) -> Result<StoredAgent, String> {
    state
        .executor()
        .get_agent(id)
        .await
        .map_err(|e| e.to_string())
}

/// Create agent request
#[derive(Debug, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    pub agent: AgentNode,
}

/// Create a new agent
#[tauri::command]
pub async fn create_agent(
    state: State<'_, AppState>,
    request: CreateAgentRequest,
) -> Result<StoredAgent, String> {
    request
        .agent
        .validate()
        .map_err(restflow_core::models::encode_validation_error)?;
    state
        .executor()
        .create_agent(request.name, request.agent)
        .await
        .map_err(|e| e.to_string())
}

/// Update agent request
#[derive(Debug, Deserialize)]
pub struct UpdateAgentRequest {
    pub name: Option<String>,
    pub agent: Option<AgentNode>,
}

/// Update an existing agent
#[tauri::command]
pub async fn update_agent(
    state: State<'_, AppState>,
    id: String,
    request: UpdateAgentRequest,
) -> Result<StoredAgent, String> {
    if let Some(agent) = request.agent.as_ref() {
        agent
            .validate()
            .map_err(restflow_core::models::encode_validation_error)?;
    }
    state
        .executor()
        .update_agent(id, request.name, request.agent)
        .await
        .map_err(|e| e.to_string())
}

/// Delete an agent by ID
#[tauri::command]
pub async fn delete_agent(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .executor()
        .delete_agent(id)
        .await
        .map_err(|e| e.to_string())
}
