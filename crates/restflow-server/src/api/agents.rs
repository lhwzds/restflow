use crate::api::{ApiResponse, state::AppState};
use axum::{
    Json,
    extract::{Path, State},
};
use restflow_ai::{
    AgentConfig, AgentExecutor, AgentState, AgentStatus, AnthropicClient, LlmClient, OpenAIClient,
    Role, ToolRegistry,
};
use restflow_ai::agent::{AgentContext, MemoryContext, SkillSummary, load_workspace_context};
use restflow_core::auth::{AuthManagerConfig, AuthProfileManager, AuthProvider};
use restflow_core::memory::{ChatSessionMirror, MessageMirror, SearchEngine};
use restflow_core::models::{
    AgentExecuteResponse, AgentNode, ApiKeyConfig, ExecutionDetails, ExecutionStep,
    MemorySearchQuery, Provider, ToolCallInfo,
};
use restflow_core::paths;
use restflow_core::storage::agent::StoredAgent;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tracing::warn;

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
    /// Optional session ID for conversation persistence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// Convert AgentState messages to ExecutionSteps for frontend
fn convert_to_execution_steps(state: &AgentState) -> Vec<ExecutionStep> {
    state
        .messages
        .iter()
        .map(|msg| {
            let step_type = match msg.role {
                Role::System => "system",
                Role::User => "user",
                Role::Assistant => {
                    if msg.tool_calls.is_some() {
                        "tool_call"
                    } else {
                        "assistant"
                    }
                }
                Role::Tool => "tool_result",
            };

            let tool_calls = msg.tool_calls.as_ref().map(|calls| {
                calls
                    .iter()
                    .map(|tc| ToolCallInfo {
                        id: tc.id.clone(),
                        name: tc.name.clone(),
                        arguments: tc.arguments.clone(),
                    })
                    .collect()
            });

            ExecutionStep {
                step_type: step_type.to_string(),
                content: msg.content.clone(),
                tool_calls,
            }
        })
        .collect()
}

/// Convert AgentStatus to string
fn status_to_string(status: &AgentStatus) -> String {
    match status {
        AgentStatus::Running => "running".to_string(),
        AgentStatus::Completed => "completed".to_string(),
        AgentStatus::Failed { error } => format!("failed: {}", error),
        AgentStatus::MaxIterations => "max_iterations".to_string(),
    }
}

/// Execute agent using restflow-ai AgentExecutor
#[allow(clippy::too_many_arguments)]
async fn run_agent_with_executor(
    agent_id: &str,
    agent_node: &AgentNode,
    input: &str,
    secret_storage: Option<&restflow_core::storage::SecretStorage>,
    skill_storage: restflow_core::storage::skill::SkillStorage,
    memory_storage: restflow_core::storage::memory::MemoryStorage,
    chat_storage: restflow_core::storage::chat_session::ChatSessionStorage,
    workdir: Option<&std::path::Path>,
) -> Result<AgentExecuteResponse, String> {
    // Get model (required for execution)
    let model = agent_node.require_model().map_err(|e| e.to_string())?;

    // Get API key
    let api_key = resolve_api_key(agent_node, secret_storage, model.provider())
        .await?;

    // Create LLM client based on model provider
    let llm: Arc<dyn LlmClient> = match model.provider() {
        Provider::OpenAI => Arc::new(OpenAIClient::new(&api_key).with_model(model.as_str())),
        Provider::Anthropic => Arc::new(AnthropicClient::new(&api_key).with_model(model.as_str())),
        Provider::DeepSeek => {
            // DeepSeek uses OpenAI-compatible API
            Arc::new(
                OpenAIClient::new(&api_key)
                    .with_model(model.as_str())
                    .with_base_url("https://api.deepseek.com/v1"),
            )
        }
    };

    let mut agent_context = AgentContext::new();

    match skill_storage.list() {
        Ok(skills) => {
            let summaries: Vec<SkillSummary> = skills
                .into_iter()
                .map(|skill| SkillSummary {
                    id: skill.id,
                    name: skill.name,
                    description: skill.description,
                })
                .collect();
            if !summaries.is_empty() {
                agent_context = agent_context.with_skills(summaries);
            }
        }
        Err(e) => {
            warn!(error = %e, "Failed to list skills for agent context");
        }
    }

    let memory_search_engine = SearchEngine::new(memory_storage.clone());
    let memory_query = MemorySearchQuery::new(agent_id.to_string())
        .with_query(input.to_string())
        .paginate(5, 0);
    match memory_search_engine.search_ranked(&memory_query) {
        Ok(results) => {
            let memories: Vec<MemoryContext> = results
                .chunks
                .into_iter()
                .map(|scored| MemoryContext {
                    content: scored.chunk.content,
                    score: scored.score,
                })
                .collect();
            if !memories.is_empty() {
                agent_context = agent_context.with_memories(memories);
            }
        }
        Err(e) => {
            warn!(error = %e, "Failed to search memories for agent context");
        }
    }

    if let Some(dir) = workdir {
        if let Some(content) = load_workspace_context(dir) {
            agent_context = agent_context.with_workspace_context(content);
        }
        agent_context = agent_context.with_workdir(dir.display().to_string());
    }

    // Create tool registry with all tools (including skill tool with storage access)
    let full_registry = restflow_core::services::tool_registry::create_tool_registry(
        skill_storage,
        memory_storage,
        chat_storage,
    );

    // Filter to only selected tools (secure by default)
    let tools = if let Some(ref tool_names) = agent_node.tools {
        if tool_names.is_empty() {
            Arc::new(ToolRegistry::new())
        } else {
            let mut filtered_registry = ToolRegistry::new();
            for name in tool_names {
                if let Some(tool) = full_registry.get(name) {
                    filtered_registry.register_arc(tool);
                } else {
                    warn!(tool_name = %name, "Configured tool not found in registry, skipping");
                }
            }
            Arc::new(filtered_registry)
        }
    } else {
        // No tools configured = no tools available (secure by default)
        Arc::new(ToolRegistry::new())
    };

    // Build agent config
    let mut config = AgentConfig::new(input);

    if let Some(ref prompt) = agent_node.prompt {
        config = config.with_system_prompt(prompt);
    }

    // Only set temperature for models that support it
    if model.supports_temperature()
        && let Some(temp) = agent_node.temperature
    {
        config = config.with_temperature(temp as f32);
    }

    if !agent_context.is_empty() {
        config = config.with_agent_context(agent_context);
    }

    // Execute agent
    let executor = AgentExecutor::new(llm, tools);
    let result = executor
        .run(config)
        .await
        .map_err(|e| format!("Agent execution failed: {}", e))?;

    // Build response
    let response = result.answer.unwrap_or_else(|| {
        if let Some(ref err) = result.error {
            format!("Error: {}", err)
        } else {
            "No response generated".to_string()
        }
    });

    let execution_details = ExecutionDetails {
        iterations: result.iterations,
        total_tokens: result.total_tokens,
        steps: convert_to_execution_steps(&result.state),
        status: status_to_string(&result.state.status),
    };

    Ok(AgentExecuteResponse {
        response,
        execution_details: Some(execution_details),
    })
}

async fn resolve_api_key(
    agent_node: &AgentNode,
    secret_storage: Option<&restflow_core::storage::SecretStorage>,
    provider: Provider,
) -> Result<String, String> {
    if let Some(config) = &agent_node.api_key_config {
        match config {
            ApiKeyConfig::Direct(key) => {
                if !key.is_empty() {
                    return Ok(key.clone());
                }
            }
            ApiKeyConfig::Secret(secret_name) => {
                if let Some(storage) = secret_storage {
                    return storage
                        .get_secret(secret_name)
                        .map_err(|e| format!("Failed to get secret: {}", e))?
                        .ok_or_else(|| format!("Secret '{}' not found", secret_name));
                }
                return Err("Secret manager not available".to_string());
            }
        }
    }

    if let Some(key) = resolve_api_key_from_profiles(provider).await? {
        return Ok(key);
    }

    Err("No API key configured".to_string())
}

async fn resolve_api_key_from_profiles(provider: Provider) -> Result<Option<String>, String> {
    let mut config = AuthManagerConfig::default();
    let profiles_path = paths::ensure_data_dir()
        .map_err(|e| format!("Failed to resolve data dir: {}", e))?
        .join("auth_profiles.json");
    config.profiles_path = Some(profiles_path);

    let manager = AuthProfileManager::with_config(config);
    manager
        .initialize()
        .await
        .map_err(|e| format!("Failed to initialize auth profiles: {}", e))?;

    let selection = match provider {
        Provider::Anthropic => {
            if let Some(selection) = manager.select_profile(AuthProvider::Anthropic).await {
                Some(selection)
            } else {
                manager.select_profile(AuthProvider::ClaudeCode).await
            }
        }
        Provider::OpenAI => manager.select_profile(AuthProvider::OpenAI).await,
        Provider::DeepSeek => None,
    };

    Ok(selection.map(|profile| profile.profile.get_api_key().to_string()))
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

    let workdir = std::env::current_dir().ok();

    match run_agent_with_executor(
        &agent.id,
        &agent.agent,
        &request.input,
        Some(&state.storage.secrets),
        state.storage.skills.clone(),
        state.storage.memory.clone(),
        state.storage.chat_sessions.clone(),
        workdir.as_deref(),
    )
    .await
    {
        Ok(response) => {
            if let Some(ref session_id) = request.session_id {
                let mirror = ChatSessionMirror::new(Arc::new(state.storage.chat_sessions.clone()));

                if let Err(e) = mirror.mirror_user(session_id, &request.input).await {
                    warn!(error = %e, "Failed to mirror user message");
                }

                let tokens = response
                    .execution_details
                    .as_ref()
                    .map(|details| details.total_tokens);

                if let Err(e) = mirror
                    .mirror_assistant(session_id, &response.response, tokens)
                    .await
                {
                    warn!(error = %e, "Failed to mirror assistant message");
                }
            }

            Json(ApiResponse::ok(response))
        }
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

    // Validate input field exists and is a non-empty string
    let input = match agent_with_input.get("input") {
        Some(Value::String(s)) if !s.trim().is_empty() => s.clone(),
        Some(Value::String(_)) => {
            return Json(ApiResponse::error("Input cannot be empty".to_string()));
        }
        Some(_) => {
            return Json(ApiResponse::error("Input must be a string".to_string()));
        }
        None => {
            return Json(ApiResponse::error(
                "Missing required field: input".to_string(),
            ));
        }
    };

    let workdir = std::env::current_dir().ok();

    match run_agent_with_executor(
        "inline",
        &agent,
        &input,
        Some(&state.storage.secrets),
        state.storage.skills.clone(),
        state.storage.memory.clone(),
        state.storage.chat_sessions.clone(),
        workdir.as_deref(),
    )
    .await
    {
        Ok(response) => Json(ApiResponse::ok(response)),
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to execute agent: {}",
            e
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_core::AppCore;
    use restflow_core::models::AIModel;
    use std::sync::Arc;
    use tempfile::{TempDir, tempdir};

    async fn create_test_app() -> (AppState, TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let core = Arc::new(AppCore::new(db_path.to_str().unwrap()).await.unwrap());
        let app = AppState::new(core);
        (app, temp_dir)
    }

    fn create_test_agent() -> AgentNode {
        AgentNode {
            model: Some(AIModel::ClaudeSonnet4_5),
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
        let agents = body.data.unwrap();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name, "Default Assistant");
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
        assert_eq!(data.agent.model, Some(AIModel::ClaudeSonnet4_5));
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
