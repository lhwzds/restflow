use crate::AppCore;
use crate::daemon::http::ApiError;
use crate::models::{AIModel, AgentNode};
use crate::services::agent as agent_service;
use crate::storage::agent::StoredAgent;
use axum::{
    Json, Router,
    extract::{Extension, Path},
    http::StatusCode,
    routing::{get, post},
};
use serde::Deserialize;
use std::sync::Arc;

fn parse_model(s: &str) -> Option<AIModel> {
    serde_json::from_value(serde_json::Value::String(s.to_string())).ok()
}

pub fn router() -> Router {
    Router::new()
        .route("/", get(list_agents).post(create_agent))
        .route(
            "/{id}",
            get(get_agent).put(update_agent).delete(delete_agent),
        )
        .route("/{id}/execute", post(execute_agent))
}

async fn list_agents(
    Extension(core): Extension<Arc<AppCore>>,
) -> Result<Json<Vec<StoredAgent>>, ApiError> {
    let agents = agent_service::list_agents(&core).await?;
    Ok(Json(agents))
}

async fn get_agent(
    Extension(core): Extension<Arc<AppCore>>,
    Path(id): Path<String>,
) -> Result<Json<StoredAgent>, ApiError> {
    let agent = agent_service::get_agent(&core, &id).await?;
    Ok(Json(agent))
}

#[derive(Debug, Deserialize)]
struct CreateAgentRequest {
    name: String,
    agent: Option<AgentNode>,
    model: Option<String>,
    prompt: Option<String>,
    tools: Option<Vec<String>>,
    temperature: Option<f64>,
}

fn build_agent_node(
    agent: Option<AgentNode>,
    model: Option<String>,
    prompt: Option<String>,
    tools: Option<Vec<String>>,
    temperature: Option<f64>,
) -> AgentNode {
    if let Some(agent) = agent {
        return agent;
    }

    let mut agent_node = AgentNode::new();
    if let Some(model_str) = model {
        agent_node.model = parse_model(&model_str);
    }
    if let Some(prompt) = prompt {
        agent_node = agent_node.with_prompt(prompt);
    }
    if let Some(tools) = tools {
        agent_node.tools = Some(tools);
    }
    if let Some(temperature) = temperature {
        agent_node.temperature = Some(temperature);
    }

    agent_node
}

async fn create_agent(
    Extension(core): Extension<Arc<AppCore>>,
    Json(req): Json<CreateAgentRequest>,
) -> Result<Json<StoredAgent>, ApiError> {
    let agent_node = build_agent_node(req.agent, req.model, req.prompt, req.tools, req.temperature);
    let created = agent_service::create_agent(&core, req.name, agent_node).await?;
    Ok(Json(created))
}

#[derive(Debug, Deserialize)]
struct UpdateAgentRequest {
    name: Option<String>,
    agent: Option<AgentNode>,
    model: Option<String>,
    prompt: Option<String>,
    tools: Option<Vec<String>>,
    temperature: Option<f64>,
}

async fn update_agent(
    Extension(core): Extension<Arc<AppCore>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateAgentRequest>,
) -> Result<Json<StoredAgent>, ApiError> {
    let agent = if req.agent.is_some()
        || req.model.is_some()
        || req.prompt.is_some()
        || req.tools.is_some()
        || req.temperature.is_some()
    {
        let mut existing = agent_service::get_agent(&core, &id).await?;
        let mut agent_node = existing.agent;

        if let Some(agent) = req.agent {
            agent_node = agent;
        }
        if let Some(model_str) = req.model {
            agent_node.model = parse_model(&model_str);
        }
        if let Some(prompt) = req.prompt {
            agent_node.prompt = Some(prompt);
        }
        if let Some(tools) = req.tools {
            agent_node.tools = Some(tools);
        }
        if let Some(temperature) = req.temperature {
            agent_node.temperature = Some(temperature);
        }

        Some(agent_node)
    } else {
        None
    };

    let updated = agent_service::update_agent(&core, &id, req.name, agent).await?;
    Ok(Json(updated))
}

async fn delete_agent(
    Extension(core): Extension<Arc<AppCore>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    agent_service::delete_agent(&core, &id).await?;
    Ok(Json(serde_json::json!({ "deleted": true, "id": id })))
}

async fn execute_agent(Path(id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::new(
        StatusCode::NOT_IMPLEMENTED,
        format!(
            "Agent execution is not supported for daemon HTTP API (agent {}).",
            id
        ),
    ))
}
