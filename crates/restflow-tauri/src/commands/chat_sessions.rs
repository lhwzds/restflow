//! Chat session Tauri commands for workspace chat functionality.
//!
//! These commands enable the frontend to create, manage, and interact with
//! chat sessions in the SkillWorkspace.

use crate::agent::{
    SubagentDeps, ToolRegistry, UnifiedAgent, UnifiedAgentConfig, effective_main_agent_tool_names,
    registry_from_allowlist,
};
use crate::chat::ChatStreamState;
use crate::state::AppState;
use restflow_ai::llm::Message;
use restflow_ai::{AnthropicClient, ClaudeCodeClient, LlmClient, OpenAIClient};
use restflow_core::models::{
    AgentNode, ApiKeyConfig, ChatMessage, ChatRole, ChatSession, ChatSessionSummary,
    ChatSessionUpdate, MessageExecution,
};
use restflow_core::{AIModel, Provider};
use serde::Deserialize;
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, State};
use uuid::Uuid;

/// Create a new chat session.
///
/// # Arguments
/// * `agent_id` - The agent to chat with
/// * `model` - The model to use for responses
/// * `name` - Optional custom name for the session
/// * `skill_id` - Optional skill context
#[tauri::command]
pub async fn create_chat_session(
    state: State<'_, AppState>,
    agent_id: String,
    model: String,
    name: Option<String>,
    skill_id: Option<String>,
) -> Result<ChatSession, String> {
    let session = state
        .executor()
        .create_session(Some(agent_id), Some(model), name, skill_id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(session)
}

/// List all chat sessions.
///
/// Returns sessions sorted by updated_at descending (most recent first).
#[tauri::command]
pub async fn list_chat_sessions(state: State<'_, AppState>) -> Result<Vec<ChatSession>, String> {
    state
        .executor()
        .list_full_sessions()
        .await
        .map_err(|e| e.to_string())
}

/// List chat session summaries.
///
/// More efficient than list_chat_sessions when full message history isn't needed.
#[tauri::command]
pub async fn list_chat_session_summaries(
    state: State<'_, AppState>,
) -> Result<Vec<ChatSessionSummary>, String> {
    state
        .executor()
        .list_sessions()
        .await
        .map_err(|e| e.to_string())
}

/// Get a chat session by ID.
#[tauri::command]
pub async fn get_chat_session(
    state: State<'_, AppState>,
    id: String,
) -> Result<ChatSession, String> {
    state
        .executor()
        .get_session(id)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSessionUpdateInput {
    pub agent_id: Option<String>,
    pub model: Option<String>,
    pub name: Option<String>,
}

/// Update a chat session.
#[tauri::command]
pub async fn update_chat_session(
    state: State<'_, AppState>,
    session_id: String,
    updates: ChatSessionUpdateInput,
) -> Result<ChatSession, String> {
    let updates = ChatSessionUpdate {
        agent_id: updates.agent_id,
        model: updates.model,
        name: updates.name,
    };

    state
        .executor()
        .update_session(session_id, updates)
        .await
        .map_err(|e| e.to_string())
}

/// Rename a chat session.
#[tauri::command]
pub async fn rename_chat_session(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> Result<ChatSession, String> {
    state
        .executor()
        .rename_session(id, name)
        .await
        .map_err(|e| e.to_string())
}

/// Delete a chat session.
#[tauri::command]
pub async fn delete_chat_session(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    state
        .executor()
        .delete_session(id)
        .await
        .map_err(|e| e.to_string())
}

/// Add a message to a chat session.
///
/// This adds a user message to the session. The assistant response should be
/// handled separately via streaming or the response generation flow.
#[tauri::command]
pub async fn add_chat_message(
    state: State<'_, AppState>,
    session_id: String,
    message: ChatMessage,
) -> Result<ChatSession, String> {
    state
        .executor()
        .append_message(session_id, message)
        .await
        .map_err(|e| e.to_string())
}

/// Send a chat message and get a response.
///
/// This is a convenience command that:
/// 1. Adds the user message to the session
/// 2. Triggers response generation
/// 3. Adds the assistant response
/// 4. Returns the updated session
///
/// For streaming responses, use add_chat_message + response events instead.
#[tauri::command]
pub async fn send_chat_message(
    state: State<'_, AppState>,
    session_id: String,
    content: String,
) -> Result<ChatSession, String> {
    // Add user message
    state
        .executor()
        .add_message(session_id, ChatRole::User, content)
        .await
        .map_err(|e| e.to_string())
}

/// List chat sessions for a specific agent.
#[tauri::command]
pub async fn list_chat_sessions_by_agent(
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<Vec<ChatSession>, String> {
    state
        .executor()
        .list_sessions_by_agent(agent_id)
        .await
        .map_err(|e| e.to_string())
}

/// List chat sessions for a specific skill.
#[tauri::command]
pub async fn list_chat_sessions_by_skill(
    state: State<'_, AppState>,
    skill_id: String,
) -> Result<Vec<ChatSession>, String> {
    state
        .executor()
        .list_sessions_by_skill(skill_id)
        .await
        .map_err(|e| e.to_string())
}

/// Get the count of chat sessions.
#[tauri::command]
pub async fn get_chat_session_count(state: State<'_, AppState>) -> Result<usize, String> {
    state
        .executor()
        .count_sessions()
        .await
        .map_err(|e| e.to_string())
}

/// Clear old chat sessions.
///
/// Deletes sessions that haven't been updated since the given timestamp.
/// Returns the number of deleted sessions.
#[tauri::command]
pub async fn clear_old_chat_sessions(
    state: State<'_, AppState>,
    older_than_ms: i64,
) -> Result<usize, String> {
    state
        .executor()
        .delete_sessions_older_than(older_than_ms)
        .await
        .map_err(|e| e.to_string())
}

// ============================================================================
// Agent Execution Commands
// ============================================================================

/// Convert a stored chat message into an LLM message.
fn chat_message_to_llm_message(message: &ChatMessage) -> Message {
    match message.role {
        ChatRole::User => Message::user(message.content.clone()),
        ChatRole::Assistant => Message::assistant(message.content.clone()),
        ChatRole::System => Message::system(message.content.clone()),
    }
}

/// Resolve session messages for context, respecting summary pointers.
fn session_messages_for_context(session: &ChatSession) -> Vec<ChatMessage> {
    if session.messages.is_empty() {
        return Vec::new();
    }

    if let Some(summary_id) = session.summary_message_id.as_ref()
        && let Some(idx) = session.messages.iter().position(|m| &m.id == summary_id)
    {
        let mut messages = session.messages[idx..].to_vec();
        if let Some(summary) = messages.first_mut() {
            summary.role = ChatRole::User;
        }
        return messages;
    }

    session.messages.clone()
}

/// Add recent session history to the unified agent.
fn add_session_history(agent: &mut UnifiedAgent, session: &ChatSession, max_messages: usize) {
    let mut messages = session_messages_for_context(session);
    if messages.is_empty() {
        return;
    }

    messages.pop();
    let start = messages.len().saturating_sub(max_messages);
    for message in &messages[start..] {
        agent.add_history_message(chat_message_to_llm_message(message));
    }
}

/// Resolve API key for a model provider.
///
/// Priority:
/// 1. Agent-level api_key_config (direct or secret reference)
/// 2. Well-known secret names (OPENAI_API_KEY, ANTHROPIC_API_KEY, etc.)
async fn resolve_api_key(
    state: &AppState,
    provider: Provider,
    agent_api_key_config: Option<&ApiKeyConfig>,
) -> Result<String, String> {
    // First, check agent-level API key config
    if let Some(config) = agent_api_key_config {
        match config {
            ApiKeyConfig::Direct(key) => {
                if !key.is_empty() {
                    return Ok(key.clone());
                }
            }
            ApiKeyConfig::Secret(secret_name) => {
                if let Some(secret_value) = state
                    .executor()
                    .get_secret(secret_name.to_string())
                    .await
                    .map_err(|e| e.to_string())?
                {
                    return Ok(secret_value);
                }
                return Err(format!("Secret '{}' not found", secret_name));
            }
        }
    }

    // Fall back to well-known secret names for each provider
    let secret_name = provider.api_key_env();

    if let Some(secret_value) = state
        .executor()
        .get_secret(secret_name.to_string())
        .await
        .map_err(|e| e.to_string())?
    {
        return Ok(secret_value);
    }

    Err(format!(
        "No API key configured for provider {:?}. Please add secret '{}' in Settings.",
        provider, secret_name
    ))
}

/// Create an LLM client for the given model.
fn create_llm_client(model: AIModel, api_key: &str) -> Arc<dyn LlmClient> {
    let model_str = model.as_str();
    match model.provider() {
        Provider::Anthropic => {
            if api_key.starts_with("sk-ant-oat") {
                Arc::new(ClaudeCodeClient::new(api_key).with_model(model_str))
            } else {
                Arc::new(AnthropicClient::new(api_key).with_model(model_str))
            }
        }
        provider => {
            let mut client = OpenAIClient::new(api_key).with_model(model_str);
            match provider {
                Provider::OpenAI => {}
                Provider::DeepSeek => {
                    client = client.with_base_url("https://api.deepseek.com/v1");
                }
                Provider::Google => {
                    client = client
                        .with_base_url("https://generativelanguage.googleapis.com/v1beta/openai");
                }
                Provider::Groq => {
                    client = client.with_base_url("https://api.groq.com/openai/v1");
                }
                Provider::OpenRouter => {
                    client = client.with_base_url("https://openrouter.ai/api/v1");
                }
                Provider::XAI => {
                    client = client.with_base_url("https://api.x.ai/v1");
                }
                Provider::Qwen => {
                    client =
                        client.with_base_url("https://dashscope.aliyuncs.com/compatible-mode/v1");
                }
                Provider::Zhipu => {
                    client = client.with_base_url("https://open.bigmodel.cn/api/paas/v4");
                }
                Provider::Moonshot => {
                    client = client.with_base_url("https://api.moonshot.cn/v1");
                }
                Provider::Doubao => {
                    client = client.with_base_url("https://ark.cn-beijing.volces.com/api/v3");
                }
                Provider::Yi => {
                    client = client.with_base_url("https://api.lingyiwanwu.com/v1");
                }
                Provider::SiliconFlow => {
                    client = client.with_base_url("https://api.siliconflow.cn/v1");
                }
                Provider::Anthropic => {}
            }
            Arc::new(client)
        }
    }
}

/// Build the unified agent configuration for a given model.
fn build_agent_config(agent_node: &AgentNode, model: AIModel) -> UnifiedAgentConfig {
    let mut config = UnifiedAgentConfig::default();
    if model.supports_temperature()
        && let Some(temp) = agent_node.temperature
    {
        config.temperature = temp as f32;
    }
    config
}

/// Execute agent for a chat session and return the response.
///
/// This internal function handles:
/// 1. Loading the agent configuration
/// 2. Building conversation context from session history
/// 3. Resolving API keys
/// 4. Creating LLM client and tool registry
/// 5. Running the UnifiedAgent
/// 6. Returning the response text and iteration count
async fn execute_agent_for_session(
    state: &AppState,
    session: &ChatSession,
    user_input: &str,
) -> Result<(String, u32), String> {
    // Load agent
    let stored_agent = state
        .executor()
        .get_agent(session.agent_id.clone())
        .await
        .map_err(|e| e.to_string())?;

    let agent_node = &stored_agent.agent;

    // Get model
    let model = agent_node.require_model().map_err(|e| e.to_string())?;

    // Resolve API key
    let api_key =
        resolve_api_key(state, model.provider(), agent_node.api_key_config.as_ref()).await?;

    // Create LLM client
    let llm = create_llm_client(model, &api_key);

    // Build tool registry
    let subagent_deps = state.subagent_deps(llm.clone());
    let secret_resolver = state.secret_resolver();
    let tool_storage = None;
    let effective_tools = effective_main_agent_tool_names(agent_node.tools.as_deref());
    let tools = Arc::new(registry_from_allowlist(
        Some(&effective_tools),
        Some(&subagent_deps),
        secret_resolver,
        tool_storage,
        Some(&session.agent_id),
    ));

    let system_prompt = state
        .executor()
        .build_agent_system_prompt(agent_node.clone())
        .await
        .map_err(|e| e.to_string())?;

    // Build agent config
    let config = build_agent_config(agent_node, model);

    // Create UnifiedAgent with session history
    let mut agent = UnifiedAgent::new(llm, tools, system_prompt, config);

    // Add conversation history (excluding the last message which is the current input)
    add_session_history(&mut agent, session, 20);

    // Execute agent
    let result = agent
        .execute(user_input)
        .await
        .map_err(|e| format!("Agent execution failed: {}", e))?;

    // Extract response
    let response = if result.success {
        result.output
    } else {
        format!("Error: {}", result.output)
    };

    Ok((response, result.iterations as u32))
}

/// Execute the agent for a chat session and save the response.
///
/// This command:
/// 1. Loads the chat session
/// 2. Gets the last user message as input
/// 3. Executes the agent
/// 4. Saves the assistant response to the session
/// 5. Returns the updated session
#[tauri::command]
pub async fn execute_chat_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<ChatSession, String> {
    // Load session via IPC
    let session = state
        .executor()
        .get_session(session_id.clone())
        .await
        .map_err(|e| e.to_string())?;

    // Get last user message as input
    let user_input = session
        .messages
        .iter()
        .rev()
        .find(|m| m.role == ChatRole::User)
        .map(|m| m.content.clone())
        .ok_or_else(|| "No user message found in session".to_string())?;

    // Execute agent
    let started_at = Instant::now();
    let (response, tokens) = execute_agent_for_session(&state, &session, &user_input).await?;
    let duration_ms = started_at.elapsed().as_millis() as u64;

    // Create execution details
    let execution = MessageExecution::new().complete(duration_ms, tokens);

    // Add assistant response via IPC
    let mut assistant_message = ChatMessage::assistant(&response);
    assistant_message = assistant_message.with_execution(execution);

    let updated_session = state
        .executor()
        .append_message(session_id, assistant_message)
        .await
        .map_err(|e| e.to_string())?;

    Ok(updated_session)
}

/// Send a chat message with streaming response.
///
/// This command:
/// 1. Adds the user message to the session
/// 2. Generates a message ID for the response
/// 3. Spawns a background task to execute the agent and stream events
/// 4. Returns the message ID immediately
///
/// The frontend should listen to 'chat:stream' events to receive updates.
#[tauri::command]
pub async fn send_chat_message_stream(
    state: State<'_, AppState>,
    app: AppHandle,
    session_id: String,
    message: String,
) -> Result<String, String> {
    // Add user message to session via IPC (auto-names if first message)
    let session = state
        .executor()
        .add_message(session_id.clone(), ChatRole::User, message.clone())
        .await
        .map_err(|e| e.to_string())?;

    // Generate message ID for the response
    let message_id = Uuid::new_v4().to_string();

    // Create stream state and get cancel handle
    let (stream_state, cancel_handle) = ChatStreamState::new(
        app.clone(),
        session_id.clone(),
        message_id.clone(),
        session.model.clone(),
    );

    // Register with stream manager
    state.stream_manager.register(&message_id, cancel_handle);

    // Clone what we need for the background task
    let executor = state.executor();
    let session_id_clone = session_id.clone();
    let message_id_clone = message_id.clone();
    let user_input = message.clone();
    let stream_manager = state.stream_manager.clone();
    let subagent_tracker = state.subagent_tracker.clone();
    let subagent_definitions = state.subagent_definitions.clone();
    let subagent_config = state.subagent_config.clone();
    let secret_resolver = state.secret_resolver();
    let tool_storage = None;

    // Spawn background task for assistant response generation
    tokio::spawn(async move {
        let mut stream_state = stream_state;

        // Emit stream started
        stream_state.emit_started();

        let started_at = Instant::now();

        // Reload session to get latest state via IPC
        let session = match executor.get_session(session_id_clone.clone()).await {
            Ok(s) => s,
            Err(e) => {
                stream_state.emit_failed(&format!("Failed to load session: {}", e));
                stream_manager.remove(&message_id_clone);
                return;
            }
        };

        // Load agent
        let stored_agent = match executor.get_agent(session.agent_id.clone()).await {
            Ok(a) => a,
            Err(e) => {
                stream_state.emit_failed(&format!("Failed to load agent: {}", e));
                stream_manager.remove(&message_id_clone);
                return;
            }
        };

        let agent_node = &stored_agent.agent;

        // Get model
        let model = match agent_node.require_model() {
            Ok(m) => m,
            Err(e) => {
                stream_state.emit_failed(e);
                stream_manager.remove(&message_id_clone);
                return;
            }
        };

        // Resolve API key - simplified for background task
        let api_key = match &agent_node.api_key_config {
            Some(ApiKeyConfig::Direct(key)) if !key.is_empty() => key.clone(),
            Some(ApiKeyConfig::Secret(secret_name)) => {
                match executor.get_secret(secret_name.to_string()).await {
                    Ok(Some(key)) => key,
                    Ok(None) => {
                        stream_state.emit_failed(&format!("Secret '{}' not found", secret_name));
                        stream_manager.remove(&message_id_clone);
                        return;
                    }
                    Err(e) => {
                        stream_state.emit_failed(&format!("Failed to get secret: {}", e));
                        stream_manager.remove(&message_id_clone);
                        return;
                    }
                }
            }
            _ => {
                let secret_name = model.provider().api_key_env();
                match executor.get_secret(secret_name.to_string()).await {
                    Ok(Some(key)) => key,
                    Ok(None) => {
                        stream_state.emit_failed(&format!(
                            "No API key configured. Please add '{}' in Settings.",
                            secret_name
                        ));
                        stream_manager.remove(&message_id_clone);
                        return;
                    }
                    Err(e) => {
                        stream_state.emit_failed(&format!("Failed to get secret: {}", e));
                        stream_manager.remove(&message_id_clone);
                        return;
                    }
                }
            }
        };

        // Create LLM client
        let llm = create_llm_client(model, &api_key);

        // Build tool registry
        let subagent_deps = SubagentDeps {
            tracker: subagent_tracker,
            definitions: subagent_definitions,
            llm_client: llm.clone(),
            tool_registry: Arc::new(ToolRegistry::new()),
            config: subagent_config,
        };
        let effective_tools = effective_main_agent_tool_names(agent_node.tools.as_deref());
        let tools = Arc::new(registry_from_allowlist(
            Some(&effective_tools),
            Some(&subagent_deps),
            secret_resolver.clone(),
            tool_storage,
            Some(&session.agent_id),
        ));

        let system_prompt = match executor.build_agent_system_prompt(agent_node.clone()).await {
            Ok(prompt) => prompt,
            Err(e) => {
                stream_state.emit_failed(&format!("Failed to build system prompt: {}", e));
                stream_manager.remove(&message_id_clone);
                return;
            }
        };

        // Build agent config
        let config = build_agent_config(agent_node, model);

        // Create UnifiedAgent with session history
        let mut agent = UnifiedAgent::new(llm, tools, system_prompt, config);

        // Add conversation history (excluding the last message which is the current input)
        add_session_history(&mut agent, &session, 20);

        // Execute agent
        let result = match agent
            .execute_streaming(&user_input, &mut stream_state)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                stream_state.emit_failed(&format!("Agent execution failed: {}", e));
                stream_manager.remove(&message_id_clone);
                return;
            }
        };

        let duration_ms = started_at.elapsed().as_millis() as u64;

        // Extract response
        let response = if result.success {
            result.output
        } else {
            format!("Error: {}", result.output)
        };

        if !result.success {
            stream_state.emit_failed(&response);
        }

        // Save assistant response via IPC
        let execution = MessageExecution::new().complete(duration_ms, result.iterations as u32);
        let mut assistant_message = ChatMessage::assistant(&response);
        assistant_message = assistant_message.with_execution(execution);
        let _ = executor
            .append_message(session_id_clone, assistant_message)
            .await;

        // Remove from stream manager
        stream_manager.remove(&message_id_clone);
    });

    Ok(message_id)
}

/// Cancel an active chat stream.
#[tauri::command]
pub async fn cancel_chat_stream(
    state: State<'_, AppState>,
    _session_id: String,
    message_id: String,
) -> Result<bool, String> {
    Ok(state.stream_manager.cancel(&message_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_message_user_creation() {
        let msg = ChatMessage::user("Hello!");
        assert_eq!(msg.content, "Hello!");
    }

    #[test]
    fn test_chat_session_creation() {
        let session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
        assert!(!session.id.is_empty());
        assert_eq!(session.agent_id, "agent-1");
        assert_eq!(session.model, "claude-sonnet-4");
        assert_eq!(session.name, "New Chat");
    }

    #[test]
    fn test_chat_session_with_name() {
        let session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string())
            .with_name("My Chat");
        assert_eq!(session.name, "My Chat");
    }

    #[test]
    fn test_chat_session_with_skill() {
        let session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string())
            .with_skill("skill-123");
        assert_eq!(session.skill_id, Some("skill-123".to_string()));
    }

    #[test]
    fn test_auto_name_from_message() {
        let mut session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
        session.add_message(ChatMessage::user("Help me debug this code"));
        session.auto_name_from_first_message();
        assert_eq!(session.name, "Help me debug this code");
    }

    #[test]
    fn test_auto_name_truncates_long_message() {
        let mut session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
        session.add_message(ChatMessage::user(
            "This is a very long message that should be truncated to thirty characters",
        ));
        session.auto_name_from_first_message();
        assert!(session.name.ends_with("..."));
        assert!(session.name.len() <= 33);
    }

    #[test]
    fn test_main_agent_default_tools_include_transcribe() {
        let tools = crate::agent::main_agent_default_tool_names();
        assert!(tools.iter().any(|name| name == "transcribe"));
        assert!(tools.iter().any(|name| name == "vision"));
    }

    #[test]
    fn test_effective_main_agent_tool_names_merges_extra_without_duplicates() {
        let extra = vec!["custom_tool".to_string(), "bash".to_string()];
        let merged = effective_main_agent_tool_names(Some(&extra));

        assert!(merged.iter().any(|name| name == "transcribe"));
        assert!(merged.iter().any(|name| name == "custom_tool"));
        assert_eq!(
            merged.iter().filter(|name| name.as_str() == "bash").count(),
            1
        );
    }
}
