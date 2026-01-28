//! Agent-related Tauri commands

use crate::state::AppState;
use restflow_core::AgentExecuteResponse;
use restflow_core::models::AgentNode;
use restflow_core::storage::agent::StoredAgent;
use serde::Deserialize;
use tauri::State;

/// List all agents
#[tauri::command]
pub async fn list_agents(state: State<'_, AppState>) -> Result<Vec<StoredAgent>, String> {
    state
        .core
        .storage
        .agents
        .list_agents()
        .map_err(|e| e.to_string())
}

/// Get an agent by ID
#[tauri::command]
pub async fn get_agent(state: State<'_, AppState>, id: String) -> Result<StoredAgent, String> {
    state
        .core
        .storage
        .agents
        .get_agent(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Agent not found".to_string())
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
    state
        .core
        .storage
        .agents
        .create_agent(request.name, request.agent)
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
    state
        .core
        .storage
        .agents
        .update_agent(id, request.name, request.agent)
        .map_err(|e| e.to_string())
}

/// Delete an agent by ID
#[tauri::command]
pub async fn delete_agent(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .core
        .storage
        .agents
        .delete_agent(id)
        .map_err(|e| e.to_string())
}

/// Agent execution request
#[derive(Debug, Deserialize)]
pub struct ExecuteAgentRequest {
    pub prompt: String,
    #[serde(default)]
    pub tools: Option<Vec<String>>,
}

/// Execute an agent with a prompt
/// Note: Full agent execution will be implemented when integrating with restflow-ai
#[tauri::command]
pub async fn execute_agent(
    state: State<'_, AppState>,
    id: String,
    request: ExecuteAgentRequest,
) -> Result<AgentExecuteResponse, String> {
    let _agent = state
        .core
        .storage
        .agents
        .get_agent(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Agent not found".to_string())?;

    // TODO: Implement full agent execution using restflow-ai AgentExecutor
    // For now, return a placeholder response
    Ok(AgentExecuteResponse {
        response: format!(
            "Agent execution not yet implemented. Prompt: {}",
            request.prompt
        ),
        execution_details: None,
    })
}

/// Execute an inline agent (without saving)
#[tauri::command]
pub async fn execute_agent_inline(
    _state: State<'_, AppState>,
    _agent: AgentNode,
    prompt: String,
    _tools: Option<Vec<String>>,
) -> Result<AgentExecuteResponse, String> {
    // TODO: Implement full agent execution using restflow-ai AgentExecutor
    Ok(AgentExecuteResponse {
        response: format!(
            "Inline agent execution not yet implemented. Prompt: {}",
            prompt
        ),
        execution_details: None,
    })
}
