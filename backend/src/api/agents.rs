use crate::api::{state::AppState, ApiResponse};
use crate::node::agent::AgentNode;
use crate::storage::agent::StoredAgent;
use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    pub agent: AgentNode,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateAgentRequest {
    pub name: Option<String>,
    pub agent: Option<AgentNode>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecuteAgentRequest {
    pub input: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentExecuteResponse {
    pub response: String,
}

// GET /api/agents
pub async fn list_agents(State(state): State<AppState>) -> Json<ApiResponse<Vec<StoredAgent>>> {
    match state.storage.agents.list_agents() {
        Ok(agents) => Json(ApiResponse::ok(agents)),
        Err(e) => Json(ApiResponse::error(format!("Failed to list agents: {}", e))),
    }
}

// GET /api/agents/{id}
pub async fn get_agent(State(state): State<AppState>, Path(id): Path<String>) -> Json<ApiResponse<StoredAgent>> {
    match state.storage.agents.get_agent(id.clone()) {
        Ok(Some(agent)) => Json(ApiResponse::ok(agent)),
        Ok(None) => Json(ApiResponse::error(format!("Agent {} not found", id))),
        Err(e) => Json(ApiResponse::error(format!("Failed to get agent: {}", e))),
    }
}

// POST /api/agents
pub async fn create_agent(
    State(state): State<AppState>,
    Json(request): Json<CreateAgentRequest>,
) -> Json<ApiResponse<StoredAgent>> {
    match state
        .storage
        .agents
        .create_agent(request.name, request.agent)
    {
        Ok(stored_agent) => Json(ApiResponse::ok_with_message(
            stored_agent,
            "Agent created successfully"
        )),
        Err(e) => Json(ApiResponse::error(format!("Failed to create agent: {}", e))),
    }
}

// PUT /api/agents/{id}
pub async fn update_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateAgentRequest>,
) -> Json<ApiResponse<StoredAgent>> {
    match state
        .storage
        .agents
        .update_agent(id.clone(), request.name, request.agent)
    {
        Ok(agent) => Json(ApiResponse::ok_with_message(
            agent,
            "Agent updated successfully"
        )),
        Err(e) => Json(ApiResponse::error(e.to_string())),
    }
}

// DELETE /api/agents/{id}
pub async fn delete_agent(State(state): State<AppState>, Path(id): Path<String>) -> Json<ApiResponse<()>> {
    match state.storage.agents.delete_agent(id.clone()) {
        Ok(()) => Json(ApiResponse::message(format!("Agent {} deleted successfully", id))),
        Err(e) => Json(ApiResponse::error(e.to_string())),
    }
}

// POST /api/agents/{id}/execute
pub async fn execute_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ExecuteAgentRequest>,
) -> Json<ApiResponse<AgentExecuteResponse>> {
    // Get the agent
    let agent = match state.storage.agents.get_agent(id.clone()) {
        Ok(Some(agent)) => agent,
        Ok(None) => {
            return Json(ApiResponse::error(format!("Agent {} not found", id)));
        }
        Err(e) => {
            return Json(ApiResponse::error(format!("Failed to get agent: {}", e)));
        }
    };

    // Execute the agent with secret storage access
    match agent.agent.execute(&request.input, Some(&state.storage.secrets)).await {
        Ok(response) => Json(ApiResponse::ok(AgentExecuteResponse { response })),
        Err(e) => Json(ApiResponse::error(format!("Failed to execute agent: {}", e))),
    }
}

// POST /api/agents/execute-inline
pub async fn execute_agent_inline(
    State(state): State<AppState>,
    Json(agent_with_input): Json<Value>
) -> Json<ApiResponse<AgentExecuteResponse>> {
    // Parse the agent configuration
    let agent = match serde_json::from_value::<AgentNode>(agent_with_input["agent"].clone()) {
        Ok(a) => a,
        Err(e) => {
            return Json(ApiResponse::error(format!("Invalid agent configuration: {}", e)));
        }
    };

    let input = agent_with_input["input"].as_str().unwrap_or("").to_string();

    // Execute the agent with secret storage access
    match agent.execute(&input, Some(&state.storage.secrets)).await {
        Ok(response) => Json(ApiResponse::ok(AgentExecuteResponse { response })),
        Err(e) => Json(ApiResponse::error(format!("Failed to execute agent: {}", e))),
    }
}
