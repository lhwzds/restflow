use crate::api::{ApiResponse, state::AppState};
use axum::{
    Json,
    extract::{Path, State},
};
use restflow_workflow::node::agent::AgentNode;
use restflow_workflow::storage::agent::StoredAgent;
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
pub async fn get_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<StoredAgent>> {
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
            "Agent created successfully",
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
            "Agent updated successfully",
        )),
        Err(e) => Json(ApiResponse::error(e.to_string())),
    }
}

// DELETE /api/agents/{id}
pub async fn delete_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<()>> {
    match state.storage.agents.delete_agent(id.clone()) {
        Ok(()) => Json(ApiResponse::message(format!(
            "Agent {} deleted successfully",
            id
        ))),
        Err(e) => Json(ApiResponse::error(e.to_string())),
    }
}

// POST /api/agents/{id}/execute
pub async fn execute_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ExecuteAgentRequest>,
) -> Json<ApiResponse<AgentExecuteResponse>> {
    let agent = match state.storage.agents.get_agent(id.clone()) {
        Ok(Some(agent)) => agent,
        Ok(None) => {
            return Json(ApiResponse::error(format!("Agent {} not found", id)));
        }
        Err(e) => {
            return Json(ApiResponse::error(format!("Failed to get agent: {}", e)));
        }
    };

    match agent
        .agent
        .execute(&request.input, Some(&state.storage.secrets))
        .await
    {
        Ok(response) => Json(ApiResponse::ok(AgentExecuteResponse { response })),
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to execute agent: {}",
            e
        ))),
    }
}

// POST /api/agents/execute-inline
pub async fn execute_agent_inline(
    State(state): State<AppState>,
    Json(agent_with_input): Json<Value>,
) -> Json<ApiResponse<AgentExecuteResponse>> {
    let agent = match serde_json::from_value::<AgentNode>(agent_with_input["agent"].clone()) {
        Ok(a) => a,
        Err(e) => {
            return Json(ApiResponse::error(format!(
                "Invalid agent configuration: {}",
                e
            )));
        }
    };

    let input = agent_with_input["input"].as_str().unwrap_or("").to_string();

    match agent.execute(&input, Some(&state.storage.secrets)).await {
        Ok(response) => Json(ApiResponse::ok(AgentExecuteResponse { response })),
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to execute agent: {}",
            e
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_workflow::AppCore;
    use restflow_workflow::models::AIModel;
    use std::sync::Arc;
    use tempfile::{TempDir, tempdir};

    async fn create_test_app() -> (Arc<AppCore>, TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let app = Arc::new(AppCore::new(db_path.to_str().unwrap()).await.unwrap());
        (app, temp_dir)
    }

    fn create_test_agent() -> AgentNode {
        AgentNode {
            model: AIModel::ClaudeSonnet4_5,
            prompt: Some("You are a test assistant".to_string()),
            temperature: None,
            api_key_config: None,
            tools: None,
        }
    }

    #[tokio::test]
    async fn test_list_agents_empty() {
        let (app, _tmp_dir) = create_test_app().await;

        let response = list_agents(State(app)).await;
        let body = response.0;

        assert!(body.success);
        assert!(body.data.is_some());
        assert_eq!(body.data.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_create_agent() {
        let (app, _tmp_dir) = create_test_app().await;
        let agent = create_test_agent();

        let request = CreateAgentRequest {
            name: "Test Agent".to_string(),
            agent: agent.clone(),
        };

        let response = create_agent(State(app), Json(request)).await;
        let body = response.0;

        assert!(body.success);
        assert!(body.message.unwrap().contains("created"));

        let data = body.data.unwrap();
        assert_eq!(data.name, "Test Agent");
        assert_eq!(data.agent.model, AIModel::ClaudeSonnet4_5);
    }

    #[tokio::test]
    async fn test_get_agent() {
        let (app, _tmp_dir) = create_test_app().await;
        let agent = create_test_agent();

        let request = CreateAgentRequest {
            name: "Test Agent".to_string(),
            agent,
        };

        let create_response = create_agent(State(app.clone()), Json(request)).await;
        let agent_id = create_response.0.data.unwrap().id;

        let response = get_agent(State(app), Path(agent_id.clone())).await;
        let body = response.0;

        assert!(body.success);
        let data = body.data.unwrap();
        assert_eq!(data.id, agent_id);
        assert_eq!(data.name, "Test Agent");
    }

    #[tokio::test]
    async fn test_get_nonexistent_agent() {
        let (app, _tmp_dir) = create_test_app().await;

        let response = get_agent(State(app), Path("nonexistent".to_string())).await;
        let body = response.0;

        assert!(!body.success);
        assert!(body.message.unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_update_agent() {
        let (app, _tmp_dir) = create_test_app().await;
        let agent = create_test_agent();

        let request = CreateAgentRequest {
            name: "Test Agent".to_string(),
            agent,
        };

        let create_response = create_agent(State(app.clone()), Json(request)).await;
        let agent_id = create_response.0.data.unwrap().id;

        let update_request = UpdateAgentRequest {
            name: Some("Updated Agent".to_string()),
            agent: None,
        };

        let response = update_agent(State(app), Path(agent_id.clone()), Json(update_request)).await;
        let body = response.0;

        assert!(body.success);
        assert!(body.message.unwrap().contains("updated"));

        let data = body.data.unwrap();
        assert_eq!(data.name, "Updated Agent");
    }

    #[tokio::test]
    async fn test_delete_agent() {
        let (app, _tmp_dir) = create_test_app().await;
        let agent = create_test_agent();

        let request = CreateAgentRequest {
            name: "Test Agent".to_string(),
            agent,
        };

        let create_response = create_agent(State(app.clone()), Json(request)).await;
        let agent_id = create_response.0.data.unwrap().id;

        let response = delete_agent(State(app.clone()), Path(agent_id.clone())).await;
        let body = response.0;

        assert!(body.success);
        assert!(body.message.unwrap().contains("deleted"));

        let get_response = get_agent(State(app), Path(agent_id)).await;
        assert!(!get_response.0.success);
    }
}
