//! Agent session management with conversation history.
//!
//! This module provides the session state management for the main agent,
//! including message history, metadata, and active skills tracking.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Agent session containing conversation history and state
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentSession {
    /// Session ID
    pub id: String,

    /// Conversation messages
    pub messages: Vec<SessionMessage>,

    /// Currently active skills
    pub active_skills: Vec<String>,

    /// Session metadata
    pub metadata: SessionMetadata,

    /// Model being used
    pub model: String,

    /// Creation timestamp (Unix ms)
    pub created_at: i64,

    /// Last activity timestamp (Unix ms)
    pub last_activity: i64,
}

impl AgentSession {
    /// Create a new session
    pub fn new(id: String, model: String) -> Self {
        let now = Utc::now().timestamp_millis();
        Self {
            id,
            messages: Vec::new(),
            active_skills: Vec::new(),
            metadata: SessionMetadata::default(),
            model,
            created_at: now,
            last_activity: now,
        }
    }

    /// Add a user message to the session
    pub fn add_user_message(&mut self, content: String) {
        let message = SessionMessage {
            role: ChatRole::User,
            content,
            timestamp: Utc::now().timestamp_millis(),
            source: MessageSource::User,
            subagent_id: None,
            execution: None,
        };
        self.messages.push(message);
        self.last_activity = Utc::now().timestamp_millis();
        self.metadata.message_count += 1;
    }

    /// Add an assistant message to the session
    pub fn add_assistant_message(&mut self, content: String, execution: Option<MessageExecution>) {
        let message = SessionMessage {
            role: ChatRole::Assistant,
            content,
            timestamp: Utc::now().timestamp_millis(),
            source: MessageSource::MainAgent,
            subagent_id: None,
            execution,
        };
        self.messages.push(message);
        self.last_activity = Utc::now().timestamp_millis();
        self.metadata.message_count += 1;
    }

    /// Add a system message to the session
    pub fn add_system_message(&mut self, content: String) {
        let message = SessionMessage {
            role: ChatRole::System,
            content,
            timestamp: Utc::now().timestamp_millis(),
            source: MessageSource::System,
            subagent_id: None,
            execution: None,
        };
        self.messages.push(message);
        self.last_activity = Utc::now().timestamp_millis();
    }

    /// Add a sub-agent result message
    pub fn add_subagent_result(&mut self, content: String, agent_id: String, agent_name: String) {
        let message = SessionMessage {
            role: ChatRole::Tool,
            content,
            timestamp: Utc::now().timestamp_millis(),
            source: MessageSource::SubagentResult {
                agent_id: agent_id.clone(),
                agent_name,
            },
            subagent_id: Some(agent_id),
            execution: None,
        };
        self.messages.push(message);
        self.last_activity = Utc::now().timestamp_millis();
        self.metadata.total_subagents_spawned += 1;
    }

    /// Add a skill injection message
    pub fn add_skill_injection(&mut self, skill_id: String, content: String) {
        let message = SessionMessage {
            role: ChatRole::System,
            content,
            timestamp: Utc::now().timestamp_millis(),
            source: MessageSource::SkillInjection {
                skill_id: skill_id.clone(),
            },
            subagent_id: None,
            execution: None,
        };
        self.messages.push(message);
        self.active_skills.push(skill_id);
        self.last_activity = Utc::now().timestamp_millis();
    }

    /// Update token usage
    pub fn add_tokens(&mut self, tokens: u64) {
        self.metadata.total_tokens += tokens;
    }

    /// Update tool call count
    pub fn increment_tool_calls(&mut self) {
        self.metadata.total_tools_called += 1;
    }

    /// Get message count
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Get the last N messages for context
    pub fn last_messages(&self, n: usize) -> &[SessionMessage] {
        let start = self.messages.len().saturating_sub(n);
        &self.messages[start..]
    }

    /// Clear message history
    pub fn clear_history(&mut self) {
        self.messages.clear();
        self.metadata = SessionMetadata::default();
    }
}

/// A message in the session
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SessionMessage {
    /// Message role
    pub role: ChatRole,

    /// Message content
    pub content: String,

    /// Timestamp (Unix ms)
    pub timestamp: i64,

    /// Message source
    pub source: MessageSource,

    /// Associated sub-agent ID (if from sub-agent)
    pub subagent_id: Option<String>,

    /// Execution details (for assistant messages)
    pub execution: Option<MessageExecution>,
}

/// Message role in the conversation
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    System,
    User,
    Assistant,
    Tool,
}

/// Source of a message
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessageSource {
    User,
    MainAgent,
    SubagentResult {
        agent_id: String,
        agent_name: String,
    },
    System,
    SkillInjection {
        skill_id: String,
    },
}

/// Execution details for an assistant message
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MessageExecution {
    /// Execution steps performed
    pub steps: Vec<ExecutionStep>,

    /// Total duration in milliseconds
    pub duration_ms: u64,

    /// Tokens used
    pub tokens_used: u32,

    /// Execution status
    pub status: ExecutionStatus,
}

/// A step in the execution process
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExecutionStep {
    /// Step type (e.g., "thinking", "tool_call", "subagent_spawn")
    pub step_type: String,

    /// Step name or description
    pub name: String,

    /// Step status
    pub status: String,

    /// Duration in milliseconds
    pub duration_ms: Option<u64>,

    /// Additional data
    #[ts(type = "any | null")]
    pub data: Option<serde_json::Value>,
}

/// Execution status
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub enum ExecutionStatus {
    Running,
    Completed,
    Failed,
}

/// Session metadata for tracking usage
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SessionMetadata {
    /// Total tokens used
    pub total_tokens: u64,

    /// Total sub-agents spawned
    pub total_subagents_spawned: u32,

    /// Total tool calls made
    pub total_tools_called: u32,

    /// Total message count
    pub message_count: u32,

    /// Skills used in this session
    pub skills_used: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_creation() {
        let session = AgentSession::new(
            "test-session".to_string(),
            "claude-sonnet-4-20250514".to_string(),
        );
        assert_eq!(session.id, "test-session");
        assert!(session.messages.is_empty());
        assert_eq!(session.metadata.message_count, 0);
    }

    #[test]
    fn test_add_messages() {
        let mut session = AgentSession::new(
            "test-session".to_string(),
            "claude-sonnet-4-20250514".to_string(),
        );

        session.add_user_message("Hello".to_string());
        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.messages[0].role, ChatRole::User);

        session.add_assistant_message("Hi there!".to_string(), None);
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[1].role, ChatRole::Assistant);

        assert_eq!(session.metadata.message_count, 2);
    }

    #[test]
    fn test_last_messages() {
        let mut session = AgentSession::new(
            "test-session".to_string(),
            "claude-sonnet-4-20250514".to_string(),
        );

        for i in 0..10 {
            session.add_user_message(format!("Message {}", i));
        }

        let last_3 = session.last_messages(3);
        assert_eq!(last_3.len(), 3);
        assert_eq!(last_3[0].content, "Message 7");
        assert_eq!(last_3[2].content, "Message 9");
    }
}
