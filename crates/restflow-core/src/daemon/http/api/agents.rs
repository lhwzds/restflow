use crate::daemon::http::ApiError;
use crate::models::AgentNode;
use crate::services::agent as agent_service;
use crate::AppCore;
use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub fn router() -> Router {
    Router::new()
        .route("/", get(list_agents).post(create_agent))
        .route("/:id", get(get_agent).put(update_agent).delete(delete_agent))
        .route("/:id/execute", post(execute_agent))
}

#[derive(Debug, Serialize)]
struct AgentResponse {
    id: String,
    name: String,
    model: Option<String>,
    prompt: Option<String>,
    tools: Option<Vec<String>>,
    created_at: Option<i64>,
    updated_at: Option<i64>,
}

impl From<crate::storage::agent::StoredAgent> for AgentResponse {
    fn from(stored: crate::storage::agent::StoredAgent) -> Self {
        Self {
            id: stored.id,
            name: stored.name,
            model: stored.agent.model.map(|m| m.as_str().to_string()),
            prompt: stored.agent.prompt,
            tools: stored.agent.tools,
            created_at: stored.created_at,
            updated_at: stored.updated_at,
        }
    }
}

async fn list_agents(
    Extension(core): Extension<Arc<AppCore>>,
) -> Result<Json<Vec<AgentResponse>>, ApiError> {
    let agents = agent_service::list_agents(&core).await?;
    let response: Vec<AgentResponse> = agents.into_iter().map(AgentResponse::from).collect();
    Ok(Json(response))
}

async fn get_agent(
    Extension(core): Extension<Arc<AppCore>>,
    Path(id): Path<String>,
) -> Result<Json<AgentResponse>, ApiError> {
    let agent = agent_service::get_agent(&core, &id).await?;
    Ok(Json(AgentResponse::from(agent)))
}

#[derive(Debug, Deserialize)]
struct CreateAgentRequest {
    name: String,
    model: Option<String>,
    prompt: Option<String>,
    tools: Option<Vec<String>>,
}

async fn create_agent(
    Extension(core): Extension<Arc<AppCore>>,
    Json(req): Json<CreateAgentRequest>,
) -> Result<Json<AgentResponse>, ApiError> {
    let mut agent_node = AgentNode::new();
    
    if let Some(model_str) = req.model {
        agent_node.model = crate::models::Model::from_string(&model_str);
    }
    if let Some(prompt) = req.prompt {
        agent_node = agent_node.with_prompt(prompt);
    }
    if let Some(tools) = req.tools {
        agent_node.tools = Some(tools);
    }

    let created = agent_service::create_agent(&core, req.name, agent_node).await?;
    Ok(Json(AgentResponse::from(created)))
}

#[derive(Debug, Deserialize)]
struct UpdateAgentRequest {
    name: Option<String>,
    model: Option<String>,
    prompt: Option<String>,
    tools: Option<Vec<String>>,
}

async fn update_agent(
    Extension(core): Extension<Arc<AppCore>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateAgentRequest>,
) -> Result<Json<AgentResponse>, ApiError> {
    let mut existing = agent_service::get_agent(&core, &id).await?;

    if let Some(model_str) = req.model {
        existing.agent.model = crate::models::Model::from_string(&model_str);
    }
    if let Some(prompt) = req.prompt {
        existing.agent.prompt = Some(prompt);
    }
    if let Some(tools) = req.tools {
        existing.agent.tools = Some(tools);
    }

    let updated = agent_service::update_agent(&core, &id, req.name, Some(existing.agent)).await?;
    Ok(Json(AgentResponse::from(updated)))
}

async fn delete_agent(
    Extension(core): Extension<Arc<AppCore>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    agent_service::delete_agent(&core, &id).await?;
    Ok(Json(serde_json::json!({ "deleted": true, "id": id })))
}

async fn execute_agent(
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::new(
        StatusCode::NOT_IMPLEMENTED,
        format!("Agent execution is not supported for daemon HTTP API (agent {}).", id),
    ))
}
