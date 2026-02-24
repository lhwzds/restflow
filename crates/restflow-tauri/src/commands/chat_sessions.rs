//! Chat session Tauri commands for workspace chat functionality.
//!
//! These commands enable the frontend to create, manage, and interact with
//! chat sessions in the SkillWorkspace.

use crate::chat::ChatStreamState;
use crate::state::AppState;
use restflow_core::daemon::StreamFrame;
use restflow_core::models::{
    ChatMessage, ChatRole, ChatSession, ChatSessionSummary, ChatSessionUpdate,
};
use serde::Deserialize;
use tauri::{AppHandle, State};
use uuid::Uuid;

#[cfg(test)]
use crate::agent::effective_main_agent_tool_names;

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
    state
        .executor()
        .execute_chat_session(session_id, None)
        .await
        .map_err(|e| e.to_string())
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

    // Spawn background task for assistant response generation
    tokio::spawn(async move {
        let mut stream_state = stream_state;

        // Emit stream started
        stream_state.emit_started();

        let result = executor
            .execute_chat_session_stream(
                session_id_clone,
                Some(user_input),
                message_id_clone.clone(),
                |frame| {
                    match frame {
                        StreamFrame::Start { .. } => {}
                        StreamFrame::Data { content } => stream_state.emit_token(&content),
                        StreamFrame::ToolCall {
                            id,
                            name,
                            arguments,
                        } => stream_state.emit_tool_call_start(&id, &name, &arguments.to_string()),
                        StreamFrame::ToolResult {
                            id,
                            result,
                            success,
                        } => stream_state.emit_tool_call_end(&id, &result, success),
                        StreamFrame::Done { total_tokens } => {
                            if let Some(total) = total_tokens {
                                stream_state.update_usage(0, total);
                            }
                            stream_state.emit_completed();
                        }
                        StreamFrame::BackgroundAgentEvent { .. } => {}
                        StreamFrame::Error { code, message } => {
                            if code == 499 {
                                stream_state.emit_cancelled();
                            } else {
                                stream_state.emit_failed(&message);
                            }
                        }
                    }
                    Ok(())
                },
            )
            .await;

        if let Err(e) = result {
            stream_state.emit_failed(&format!("Agent execution failed: {}", e));
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
    let local_cancelled = state.stream_manager.cancel(&message_id);
    match state
        .executor()
        .cancel_chat_session_stream(message_id)
        .await
        .map_err(|e| e.to_string())
    {
        Ok(remote_cancelled) => Ok(local_cancelled || remote_cancelled),
        Err(err) => {
            if local_cancelled {
                Ok(true)
            } else {
                Err(err)
            }
        }
    }
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
