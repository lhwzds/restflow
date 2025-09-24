use crate::api::state::AppState;
use crate::node::agent::AgentNode;
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

// GET /api/agents
pub async fn list_agents(State(state): State<AppState>) -> Json<Value> {
    match state.storage.agents.list_agents() {
        Ok(agents) => Json(serde_json::json!({
            "status": "success",
            "data": agents
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to list agents: {}", e)
        })),
    }
}

// GET /api/agents/{id}
pub async fn get_agent(State(state): State<AppState>, Path(id): Path<String>) -> Json<Value> {
    match state.storage.agents.get_agent(id.clone()) {
        Ok(Some(agent)) => Json(serde_json::json!({
            "status": "success",
            "data": agent
        })),
        Ok(None) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Agent {} not found", id)
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to get agent: {}", e)
        })),
    }
}

// POST /api/agents
pub async fn create_agent(
    State(state): State<AppState>,
    Json(request): Json<CreateAgentRequest>,
) -> Json<Value> {
    match state
        .storage
        .agents
        .insert_agent(request.name, request.agent)
    {
        Ok(stored_agent) => Json(serde_json::json!({
            "status": "success",
            "message": "Agent created successfully",
            "data": stored_agent
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to create agent: {}", e)
        })),
    }
}

// PUT /api/agents/{id}
pub async fn update_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateAgentRequest>,
) -> Json<Value> {
    match state
        .storage
        .agents
        .update_agent(id.clone(), request.name, request.agent)
    {
        Ok(agent) => Json(serde_json::json!({
            "status": "success",
            "message": "Agent updated successfully",
            "data": agent
        })),
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("not found") {
                Json(serde_json::json!({
                    "status": "error",
                    "message": error_msg
                }))
            } else {
                Json(serde_json::json!({
                    "status": "error",
                    "message": format!("Failed to update agent: {}", e)
                }))
            }
        }
    }
}

// DELETE /api/agents/{id}
pub async fn delete_agent(State(state): State<AppState>, Path(id): Path<String>) -> Json<Value> {
    match state.storage.agents.delete_agent(id.clone()) {
        Ok(()) => Json(serde_json::json!({
            "status": "success",
            "message": format!("Agent {} deleted successfully", id)
        })),
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("not found") {
                Json(serde_json::json!({
                    "status": "error",
                    "message": error_msg
                }))
            } else {
                Json(serde_json::json!({
                    "status": "error",
                    "message": format!("Failed to delete agent: {}", e)
                }))
            }
        }
    }
}

// POST /api/agents/{id}/execute
pub async fn execute_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ExecuteAgentRequest>,
) -> Json<Value> {
    // Get the agent
    let agent = match state.storage.agents.get_agent(id.clone()) {
        Ok(Some(agent)) => agent,
        Ok(None) => {
            return Json(serde_json::json!({
                "status": "error",
                "message": format!("Agent {} not found", id)
            }));
        }
        Err(e) => {
            return Json(serde_json::json!({
                "status": "error",
                "message": format!("Failed to get agent: {}", e)
            }));
        }
    };

    // Execute the agent with secret storage access
    match agent.agent.execute(&request.input, Some(&state.storage.secrets)).await {
        Ok(response) => Json(serde_json::json!({
            "status": "success",
            "data": {
                "response": response
            }
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to execute agent: {}", e)
        })),
    }
}

// POST /api/agents/execute-inline
pub async fn execute_agent_inline(
    State(state): State<AppState>,
    Json(agent_with_input): Json<Value>
) -> Json<Value> {
    // Parse the agent configuration
    let agent = match serde_json::from_value::<AgentNode>(agent_with_input["agent"].clone()) {
        Ok(a) => a,
        Err(e) => {
            return Json(serde_json::json!({
                "status": "error",
                "message": format!("Invalid agent configuration: {}", e)
            }));
        }
    };

    let input = agent_with_input["input"].as_str().unwrap_or("").to_string();

    // Execute the agent with secret storage access
    match agent.execute(&input, Some(&state.storage.secrets)).await {
        Ok(response) => Json(serde_json::json!({
            "status": "success",
            "data": {
                "response": response
            }
        })),
        Err(e) => Json(serde_json::json!({
            "status": "error",
            "message": format!("Failed to execute agent: {}", e)
        })),
    }
}
