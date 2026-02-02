//! Chat session Tauri commands for workspace chat functionality.
//!
//! These commands enable the frontend to create, manage, and interact with
//! chat sessions in the SkillWorkspace.

use crate::agent::{
    BashConfig, FileConfig, ToolRegistryBuilder, UnifiedAgent, UnifiedAgentConfig,
    create_llm_client_for_agent,
};
use crate::chat::ChatStreamState;
use crate::state::AppState;
use restflow_core::models::{ChatMessage, ChatRole, ChatSession, ChatSessionSummary, MessageExecution};
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
    let mut session = ChatSession::new(agent_id, model);

    if let Some(n) = name {
        session = session.with_name(n);
    }

    if let Some(sid) = skill_id {
        session = session.with_skill(sid);
    }

    state
        .core
        .storage
        .chat_sessions
        .create(&session)
        .map_err(|e| e.to_string())?;

    Ok(session)
}

/// List all chat sessions.
///
/// Returns sessions sorted by updated_at descending (most recent first).
#[tauri::command]
pub async fn list_chat_sessions(state: State<'_, AppState>) -> Result<Vec<ChatSession>, String> {
    state
        .core
        .storage
        .chat_sessions
        .list()
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
        .core
        .storage
        .chat_sessions
        .list_summaries()
        .map_err(|e| e.to_string())
}

/// Get a chat session by ID.
#[tauri::command]
pub async fn get_chat_session(
    state: State<'_, AppState>,
    id: String,
) -> Result<ChatSession, String> {
    state
        .core
        .storage
        .chat_sessions
        .get(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Chat session '{}' not found", id))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSessionUpdate {
    pub agent_id: Option<String>,
    pub model: Option<String>,
    pub name: Option<String>,
}

/// Update a chat session.
#[tauri::command]
pub async fn update_chat_session(
    state: State<'_, AppState>,
    session_id: String,
    updates: ChatSessionUpdate,
) -> Result<ChatSession, String> {
    let mut session = state
        .core
        .storage
        .chat_sessions
        .get(&session_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Chat session '{}' not found", session_id))?;

    let mut updated = false;

    if let Some(agent_id) = updates.agent_id {
        session.agent_id = agent_id;
        updated = true;
    }

    if let Some(model) = updates.model {
        session.model = model;
        updated = true;
    }

    let has_name_update = updates.name.is_some();
    if let Some(name) = updates.name {
        session.rename(name);
        updated = true;
    }

    if updated {
        if !has_name_update {
            session.updated_at = chrono::Utc::now().timestamp_millis();
        }

        state
            .core
            .storage
            .chat_sessions
            .update(&session)
            .map_err(|e| e.to_string())?;
    }

    Ok(session)
}

/// Rename a chat session.
#[tauri::command]
pub async fn rename_chat_session(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> Result<ChatSession, String> {
    let mut session = state
        .core
        .storage
        .chat_sessions
        .get(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Chat session '{}' not found", id))?;

    session.rename(name);

    state
        .core
        .storage
        .chat_sessions
        .update(&session)
        .map_err(|e| e.to_string())?;

    Ok(session)
}

/// Delete a chat session.
#[tauri::command]
pub async fn delete_chat_session(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    state
        .core
        .storage
        .chat_sessions
        .delete(&id)
        .map_err(|e| e.to_string())
}

/// Add a message to a chat session.
///
/// This adds a user message to the session. The assistant response should be
/// handled separately via streaming or the agent execution flow.
#[tauri::command]
pub async fn add_chat_message(
    state: State<'_, AppState>,
    session_id: String,
    message: ChatMessage,
) -> Result<ChatSession, String> {
    let mut session = state
        .core
        .storage
        .chat_sessions
        .get(&session_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Chat session '{}' not found", session_id))?;

    session.add_message(message);

    // Auto-name from first user message if still default
    if session.name == "New Chat" && session.messages.len() == 1 {
        session.auto_name_from_first_message();
    }

    state
        .core
        .storage
        .chat_sessions
        .update(&session)
        .map_err(|e| e.to_string())?;

    Ok(session)
}

/// Send a chat message and get a response.
///
/// This is a convenience command that:
/// 1. Adds the user message to the session
/// 2. Triggers agent execution
/// 3. Adds the assistant response
/// 4. Returns the updated session
///
/// For streaming responses, use add_chat_message + agent execution events instead.
#[tauri::command]
pub async fn send_chat_message(
    state: State<'_, AppState>,
    session_id: String,
    content: String,
) -> Result<ChatSession, String> {
    // Get the session
    let mut session = state
        .core
        .storage
        .chat_sessions
        .get(&session_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Chat session '{}' not found", session_id))?;

    // Add user message
    let user_message = ChatMessage::user(&content);
    session.add_message(user_message);

    // Auto-name from first user message if still default
    if session.name == "New Chat" && session.messages.len() == 1 {
        session.auto_name_from_first_message();
    }

    // TODO: Trigger agent execution and get response
    // For now, just save with the user message
    // The actual agent execution will be handled by the streaming flow

    state
        .core
        .storage
        .chat_sessions
        .update(&session)
        .map_err(|e| e.to_string())?;

    Ok(session)
}

/// List chat sessions for a specific agent.
#[tauri::command]
pub async fn list_chat_sessions_by_agent(
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<Vec<ChatSession>, String> {
    state
        .core
        .storage
        .chat_sessions
        .list_by_agent(&agent_id)
        .map_err(|e| e.to_string())
}

/// List chat sessions for a specific skill.
#[tauri::command]
pub async fn list_chat_sessions_by_skill(
    state: State<'_, AppState>,
    skill_id: String,
) -> Result<Vec<ChatSession>, String> {
    state
        .core
        .storage
        .chat_sessions
        .list_by_skill(&skill_id)
        .map_err(|e| e.to_string())
}

/// Get the count of chat sessions.
#[tauri::command]
pub async fn get_chat_session_count(state: State<'_, AppState>) -> Result<usize, String> {
    state
        .core
        .storage
        .chat_sessions
        .count()
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
        .core
        .storage
        .chat_sessions
        .delete_older_than(older_than_ms)
        .map_err(|e| e.to_string())
}

// ============================================================================
// Agent Execution Commands
// ============================================================================

/// Build conversation history from session messages for context injection.
///
/// Returns a formatted string of recent messages (excluding the last user message
/// which will be passed as the agent input).
fn build_conversation_context(session: &ChatSession, max_messages: usize) -> String {
    let messages = &session.messages;
    if messages.len() <= 1 {
        return String::new();
    }

    // Take up to max_messages, excluding the last one (current user input)
    let context_messages = if messages.len() > max_messages + 1 {
        &messages[messages.len() - max_messages - 1..messages.len() - 1]
    } else {
        &messages[..messages.len() - 1]
    };

    if context_messages.is_empty() {
        return String::new();
    }

    let mut context = String::from("## Conversation History\n\n");
    for msg in context_messages {
        let role = match msg.role {
            ChatRole::User => "User",
            ChatRole::Assistant => "Assistant",
            ChatRole::System => "System",
        };
        context.push_str(&format!("**{}**: {}\n\n", role, msg.content));
    }
    context
}

/// Execute agent for a chat session and return the response.
///
/// This internal function handles:
/// 1. Loading the agent configuration
/// 2. Building conversation context from session history
/// 3. Resolving API keys
/// 4. Creating LLM client and tool registry
/// 5. Running the agent
/// 6. Returning the response text and token count
async fn execute_agent_for_session(
    state: &AppState,
    session: &ChatSession,
    user_input: &str,
) -> Result<(String, u32), String> {
    // Load agent
    let stored_agent = state
        .core
        .storage
        .agents
        .get_agent(session.agent_id.clone())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Agent '{}' not found", session.agent_id))?;

    // Build conversation context to inject into prompt
    let conversation_context = build_conversation_context(session, 20);

    // Modify agent's prompt to include conversation context
    let mut agent_node = stored_agent.agent.clone();
    let base_prompt = agent_node
        .prompt
        .clone()
        .unwrap_or_else(|| "You are a helpful AI assistant.".to_string());

    agent_node.prompt = Some(if conversation_context.is_empty() {
        base_prompt
    } else {
        format!("{}\n\n{}", base_prompt, conversation_context)
    });

    // Create LLM client
    let llm = create_llm_client_for_agent(&agent_node, &state.core.storage)
        .await
        .map_err(|e| e.to_string())?;

    // Build tool registry
    let tool_registry = ToolRegistryBuilder::new()
        .with_bash(BashConfig::default())
        .with_file(FileConfig::default())
        .with_http()
        .build();

    // Execute with UnifiedAgent
    let mut unified = UnifiedAgent::new(
        llm,
        Arc::new(tool_registry),
        state.core.storage.clone(),
        agent_node,
        UnifiedAgentConfig::default(),
    );

    let result = unified
        .execute(user_input)
        .await
        .map_err(|e| format!("Agent execution failed: {}", e))?;

    Ok((result.output, result.total_tokens))
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
    // Load session
    let session = state
        .core
        .storage
        .chat_sessions
        .get(&session_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Chat session '{}' not found", session_id))?;

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

    // Mirror the response with execution details
    let mut updated_session = state
        .core
        .storage
        .chat_sessions
        .get(&session_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Chat session '{}' not found", session_id))?;

    let mut assistant_message = ChatMessage::assistant(&response);
    assistant_message = assistant_message.with_execution(execution);
    updated_session.add_message(assistant_message);

    state
        .core
        .storage
        .chat_sessions
        .update(&updated_session)
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
    // Add user message to session first
    let mut session = state
        .core
        .storage
        .chat_sessions
        .get(&session_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Chat session '{}' not found", session_id))?;

    let user_message = ChatMessage::user(&message);
    session.add_message(user_message);

    // Auto-name from first user message if still default
    if session.name == "New Chat" && session.messages.len() == 1 {
        session.auto_name_from_first_message();
    }

    state
        .core
        .storage
        .chat_sessions
        .update(&session)
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
    let storage = state.core.storage.clone();
    let session_id_clone = session_id.clone();
    let message_id_clone = message_id.clone();
    let user_input = message.clone();
    let stream_manager = state.stream_manager.clone();

    // Spawn background task for agent execution
    tokio::spawn(async move {
        // Emit stream started
        stream_state.emit_started();

        let started_at = Instant::now();

        // Reload session to get latest state
        let session = match storage.chat_sessions.get(&session_id_clone) {
            Ok(Some(s)) => s,
            Ok(None) => {
                stream_state.emit_failed("Session not found");
                stream_manager.remove(&message_id_clone);
                return;
            }
            Err(e) => {
                stream_state.emit_failed(&format!("Failed to load session: {}", e));
                stream_manager.remove(&message_id_clone);
                return;
            }
        };

        // Load agent
        let stored_agent = match storage.agents.get_agent(session.agent_id.clone()) {
            Ok(Some(a)) => a,
            Ok(None) => {
                stream_state.emit_failed(&format!("Agent '{}' not found", session.agent_id));
                stream_manager.remove(&message_id_clone);
                return;
            }
            Err(e) => {
                stream_state.emit_failed(&format!("Failed to load agent: {}", e));
                stream_manager.remove(&message_id_clone);
                return;
            }
        };

        // Build conversation context and modify agent's prompt
        let conversation_context = build_conversation_context(&session, 20);
        let mut agent_node = stored_agent.agent.clone();
        let base_prompt = agent_node
            .prompt
            .clone()
            .unwrap_or_else(|| "You are a helpful AI assistant.".to_string());

        agent_node.prompt = Some(if conversation_context.is_empty() {
            base_prompt
        } else {
            format!("{}\n\n{}", base_prompt, conversation_context)
        });

        // Create LLM client
        let llm = match create_llm_client_for_agent(&agent_node, &storage).await {
            Ok(client) => client,
            Err(e) => {
                stream_state.emit_failed(&format!("Failed to create LLM client: {}", e));
                stream_manager.remove(&message_id_clone);
                return;
            }
        };

        // Build tool registry
        let tool_registry = ToolRegistryBuilder::new()
            .with_bash(BashConfig::default())
            .with_file(FileConfig::default())
            .with_http()
            .build();

        // Execute with UnifiedAgent
        let mut unified = UnifiedAgent::new(
            llm,
            Arc::new(tool_registry),
            storage.clone(),
            agent_node,
            UnifiedAgentConfig::default(),
        );

        let result = match unified.execute(&user_input).await {
            Ok(r) => r,
            Err(e) => {
                stream_state.emit_failed(&format!("Agent execution failed: {}", e));
                stream_manager.remove(&message_id_clone);
                return;
            }
        };

        let duration_ms = started_at.elapsed().as_millis() as u64;

        // Emit completed event
        stream_state.emit_completed();

        // Save assistant response
        let execution = MessageExecution::new().complete(duration_ms, result.total_tokens);

        if let Ok(Some(mut updated_session)) = storage.chat_sessions.get(&session_id_clone) {
            let mut assistant_message = ChatMessage::assistant(&result.output);
            assistant_message = assistant_message.with_execution(execution);
            updated_session.add_message(assistant_message);
            let _ = storage.chat_sessions.update(&updated_session);
        }

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
}
