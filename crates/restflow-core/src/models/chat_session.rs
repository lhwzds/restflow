//! Chat session models for conversation persistence.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Role of a chat message
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    User,
    Assistant,
    System,
    Tool,
}

/// Execution metadata for a message
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct MessageExecution {
    #[ts(type = "number")]
    pub prompt_tokens: u32,
    #[ts(type = "number")]
    pub completion_tokens: u32,
    #[ts(type = "number")]
    pub total_tokens: u32,
}

impl MessageExecution {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn complete(mut self, prompt_tokens: u32, completion_tokens: u32) -> Self {
        self.prompt_tokens = prompt_tokens;
        self.completion_tokens = completion_tokens;
        self.total_tokens = prompt_tokens.saturating_add(completion_tokens);
        self
    }
}

/// Single chat message in a session
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    #[ts(type = "number")]
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution: Option<MessageExecution>,
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: content.into(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            execution: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: content.into(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            execution: None,
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::System,
            content: content.into(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            execution: None,
        }
    }

    pub fn with_execution(mut self, execution: MessageExecution) -> Self {
        self.execution = Some(execution);
        self
    }
}

/// Aggregate metadata for a chat session
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct ChatSessionMetadata {
    #[ts(type = "number")]
    pub total_tokens: u32,
    #[ts(type = "number")]
    pub message_count: u32,
}

/// Chat session representing a persisted conversation
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChatSession {
    pub id: String,
    pub name: String,
    pub agent_id: String,
    pub model: String,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
    #[serde(default)]
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub metadata: ChatSessionMetadata,
}

impl ChatSession {
    pub fn new(agent_id: String, model: String) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        let id = uuid::Uuid::new_v4().to_string();
        let name = format!("Session {}", &id[..8]);
        Self {
            id,
            name,
            agent_id,
            model,
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
            metadata: ChatSessionMetadata::default(),
        }
    }

    pub fn add_message(&mut self, message: ChatMessage) {
        self.metadata.message_count = self.metadata.message_count.saturating_add(1);
        if let Some(ref exec) = message.execution {
            self.metadata.total_tokens =
                self.metadata.total_tokens.saturating_add(exec.total_tokens);
        }
        self.updated_at = chrono::Utc::now().timestamp_millis();
        self.messages.push(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_new() {
        let session = ChatSession::new("agent-1".to_string(), "model-1".to_string());
        assert!(!session.id.is_empty());
        assert_eq!(session.agent_id, "agent-1");
        assert_eq!(session.model, "model-1");
        assert_eq!(session.messages.len(), 0);
        assert_eq!(session.metadata.message_count, 0);
    }

    #[test]
    fn test_session_add_message_updates_metadata() {
        let mut session = ChatSession::new("agent-1".to_string(), "model-1".to_string());
        let message = ChatMessage::assistant("Hello")
            .with_execution(MessageExecution::new().complete(5, 10));
        session.add_message(message);
        assert_eq!(session.metadata.message_count, 1);
        assert_eq!(session.metadata.total_tokens, 15);
    }
}
