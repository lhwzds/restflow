use crate::api::{ApiResponse, state::AppState};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Sse,
};
use futures::{Stream, stream};
use restflow_ai::{
    AgentConfig, AgentExecutor, AgentState, AgentStatus, AnthropicClient, LlmClient, OpenAIClient,
    Role, ToolRegistry,
};
use restflow_core::models::{
    AgentExecuteResponse, AgentNode, ExecutionDetails, ExecutionMode, ExecutionStep, Provider,
    TaskEvent, TaskSchedule, ToolCallInfo,
};
use restflow_core::storage::agent::StoredAgent;
use serde::{Deserialize, Serialize};
use std::{convert::Infallible, sync::Arc, time::Instant};
use tracing::warn;

#[derive(Debug, Deserialize)]
pub struct CreateAgentTaskRequest {
    pub name: String,
    pub agent_id: String,
    pub schedule: TaskSchedule,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub notification: Option<restflow_core::models::NotificationConfig>,
    #[serde(default)]
    pub execution_mode: Option<ExecutionMode>,
    #[serde(default)]
    pub memory: Option<restflow_core::models::MemoryConfig>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAgentTaskRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub schedule: Option<TaskSchedule>,
    #[serde(default)]
    pub notification: Option<restflow_core::models::NotificationConfig>,
    #[serde(default)]
    pub execution_mode: Option<ExecutionMode>,
    #[serde(default)]
    pub memory: Option<restflow_core::models::MemoryConfig>,
}

#[derive(Debug, Deserialize)]
pub struct TaskEventQuery {
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct AgentTaskRunResponse {
    pub task: restflow_core::models::AgentTask,
    pub result: AgentExecuteResponse,
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
async fn run_agent_with_executor(
    agent_node: &AgentNode,
    input: &str,
    secret_storage: Option<&restflow_core::storage::SecretStorage>,
    skill_storage: restflow_core::storage::skill::SkillStorage,
    memory_storage: restflow_core::storage::memory::MemoryStorage,
    chat_storage: restflow_core::storage::ChatSessionStorage,
) -> Result<AgentExecuteResponse, String> {
    // Get API key
    let api_key = match &agent_node.api_key_config {
        Some(restflow_core::models::ApiKeyConfig::Direct(key)) => key.clone(),
        Some(restflow_core::models::ApiKeyConfig::Secret(secret_name)) => {
            if let Some(storage) = secret_storage {
                storage
                    .get_secret(secret_name)
                    .map_err(|e| format!("Failed to get secret: {}", e))?
                    .ok_or_else(|| format!("Secret '{}' not found", secret_name))?
            } else {
                return Err("Secret manager not available".to_string());
            }
        }
        None => return Err("No API key configured".to_string()),
    };

    // Get model (required for execution)
    let model = agent_node.require_model().map_err(|e| e.to_string())?;

    // Create LLM client based on model provider
    let llm: Arc<dyn LlmClient> = match model.provider() {
        Provider::OpenAI => {
            Arc::new(OpenAIClient::new(&api_key).with_model(model.as_str()))
        }
        Provider::Anthropic => {
            Arc::new(AnthropicClient::new(&api_key).with_model(model.as_str()))
        }
        Provider::DeepSeek => {
            // DeepSeek uses OpenAI-compatible API
            Arc::new(
                OpenAIClient::new(&api_key)
                    .with_model(model.as_str())
                    .with_base_url("https://api.deepseek.com/v1"),
            )
        }
    };

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

// GET /api/agent-tasks
pub async fn list_agent_tasks(
    State(state): State<AppState>,
) -> Json<ApiResponse<Vec<restflow_core::models::AgentTask>>> {
    match state.storage.agent_tasks.list_tasks() {
        Ok(tasks) => Json(ApiResponse::ok(tasks)),
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to list agent tasks: {}",
            e
        ))),
    }
}

// GET /api/agent-tasks/{id}
pub async fn get_agent_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<restflow_core::models::AgentTask>> {
    match state.storage.agent_tasks.get_task(&id) {
        Ok(Some(task)) => Json(ApiResponse::ok(task)),
        Ok(None) => Json(ApiResponse::error(format!("Agent task '{}' not found", id))),
        Err(e) => Json(ApiResponse::error(format!("Failed to get agent task: {}", e))),
    }
}

// POST /api/agent-tasks
pub async fn create_agent_task(
    State(state): State<AppState>,
    Json(request): Json<CreateAgentTaskRequest>,
) -> Json<ApiResponse<restflow_core::models::AgentTask>> {
    match state.storage.agent_tasks.create_task(
        request.name,
        request.agent_id,
        request.schedule,
    ) {
        Ok(mut task) => {
            let mut needs_update = false;

            if let Some(description) = request.description {
                task.description = Some(description);
                needs_update = true;
            }

            if let Some(input) = request.input {
                task.input = Some(input);
                needs_update = true;
            }

            if let Some(notification) = request.notification {
                task.notification = notification;
                needs_update = true;
            }

            if let Some(execution_mode) = request.execution_mode {
                task.execution_mode = execution_mode;
                needs_update = true;
            }

            if let Some(memory) = request.memory {
                task.memory = memory;
                needs_update = true;
            }

            if needs_update
                && let Err(e) = state.storage.agent_tasks.update_task(&task)
            {
                return Json(ApiResponse::error(format!(
                    "Failed to update agent task: {}",
                    e
                )));
            }

            Json(ApiResponse::ok_with_message(
                task,
                "Agent task created successfully",
            ))
        }
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to create agent task: {}",
            e
        ))),
    }
}

// PUT /api/agent-tasks/{id}
pub async fn update_agent_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateAgentTaskRequest>,
) -> Json<ApiResponse<restflow_core::models::AgentTask>> {
    let mut task = match state.storage.agent_tasks.get_task(&id) {
        Ok(Some(task)) => task,
        Ok(None) => {
            return Json(ApiResponse::error(format!(
                "Agent task '{}' not found",
                id
            )))
        }
        Err(e) => {
            return Json(ApiResponse::error(format!(
                "Failed to get agent task: {}",
                e
            )))
        }
    };

    if let Some(name) = request.name {
        task.name = name;
    }

    if let Some(description) = request.description {
        task.description = Some(description);
    }

    if let Some(agent_id) = request.agent_id {
        task.agent_id = agent_id;
    }

    if let Some(input) = request.input {
        task.input = Some(input);
    }

    if let Some(schedule) = request.schedule {
        task.schedule = schedule;
        task.update_next_run();
    }

    if let Some(notification) = request.notification {
        task.notification = notification;
    }

    if let Some(execution_mode) = request.execution_mode {
        task.execution_mode = execution_mode;
    }

    if let Some(memory) = request.memory {
        task.memory = memory;
    }

    task.updated_at = chrono::Utc::now().timestamp_millis();

    match state.storage.agent_tasks.update_task(&task) {
        Ok(_) => Json(ApiResponse::ok_with_message(
            task,
            "Agent task updated successfully",
        )),
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to update agent task: {}",
            e
        ))),
    }
}

// DELETE /api/agent-tasks/{id}
pub async fn delete_agent_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<()>> {
    match state.storage.agent_tasks.delete_task(&id) {
        Ok(deleted) => {
            if deleted {
                Json(ApiResponse::message("Agent task deleted successfully"))
            } else {
                Json(ApiResponse::error(format!(
                    "Agent task '{}' not found",
                    id
                )))
            }
        }
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to delete agent task: {}",
            e
        ))),
    }
}

// POST /api/agent-tasks/{id}/pause
pub async fn pause_agent_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<restflow_core::models::AgentTask>> {
    match state.storage.agent_tasks.pause_task(&id) {
        Ok(task) => Json(ApiResponse::ok(task)),
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to pause agent task: {}",
            e
        ))),
    }
}

// POST /api/agent-tasks/{id}/resume
pub async fn resume_agent_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<restflow_core::models::AgentTask>> {
    match state.storage.agent_tasks.resume_task(&id) {
        Ok(task) => Json(ApiResponse::ok(task)),
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to resume agent task: {}",
            e
        ))),
    }
}

// POST /api/agent-tasks/{id}/run
pub async fn run_agent_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<AgentTaskRunResponse>>, (StatusCode, String)> {
    let task = state
        .storage
        .agent_tasks
        .get_task(&id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Agent task '{}' not found", id)))?;

    if task.execution_mode != ExecutionMode::Api {
        return Err((
            StatusCode::BAD_REQUEST,
            "Only API execution mode is supported in server mode".to_string(),
        ));
    }

    let stored_agent: StoredAgent = state
        .storage
        .agents
        .get_agent(task.agent_id.clone())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Agent '{}' not found", task.agent_id),
            )
        })?;

    state
        .storage
        .agent_tasks
        .start_task_execution(&id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let input = task.input.clone().unwrap_or_default();
    let started_at = Instant::now();

    let result = run_agent_with_executor(
        &stored_agent.agent,
        &input,
        Some(&state.storage.secrets),
        state.storage.skills.clone(),
        state.storage.memory.clone(),
        state.storage.chat_sessions.clone(),
    )
    .await;

    let duration_ms = started_at.elapsed().as_millis() as i64;

    match result {
        Ok(execution) => {
            let task = state
                .storage
                .agent_tasks
                .complete_task_execution(&id, Some(execution.response.clone()), duration_ms)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

            Ok(Json(ApiResponse::ok(AgentTaskRunResponse {
                task,
                result: execution,
            })))
        }
        Err(err) => {
            let _ = state
                .storage
                .agent_tasks
                .fail_task_execution(&id, err.clone(), duration_ms);
            Ok(Json(ApiResponse::error(format!(
                "Agent task execution failed: {}",
                err
            ))))
        }
    }
}

// GET /api/agent-tasks/{id}/events
pub async fn get_agent_task_events(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<TaskEventQuery>,
) -> Json<ApiResponse<Vec<TaskEvent>>> {
    let events = if let Some(limit) = query.limit {
        state
            .storage
            .agent_tasks
            .list_recent_events_for_task(&id, limit)
    } else {
        state.storage.agent_tasks.list_events_for_task(&id)
    };

    match events {
        Ok(events) => Json(ApiResponse::ok(events)),
        Err(e) => Json(ApiResponse::error(format!(
            "Failed to list agent task events: {}",
            e
        ))),
    }
}

// GET /api/agent-tasks/{id}/stream
pub async fn stream_agent_task_events(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<axum::response::sse::Event, Infallible>>>, (StatusCode, String)> {
    let task_exists = state
        .storage
        .agent_tasks
        .get_task(&id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .is_some();

    if !task_exists {
        return Err((StatusCode::NOT_FOUND, format!("Agent task '{}' not found", id)));
    }

    let events = state
        .storage
        .agent_tasks
        .list_events_for_task(&id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let stream = stream::iter(events.into_iter().map(|event| {
        let data = axum::response::sse::Event::default()
            .json_data(event)
            .unwrap();
        Ok::<_, Infallible>(data)
    }));

    Ok(Sse::new(stream))
}
