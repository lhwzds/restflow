//! Chat session Tauri commands for workspace chat functionality.
//!
//! These commands enable the frontend to create, manage, and interact with
//! chat sessions in the SkillWorkspace.

use crate::state::AppState;
use restflow_core::models::{ChatMessage, ChatSession, ChatSessionSummary};
use tauri::State;

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
