//! Chat session models for workspace conversation persistence.
//!
//! This module defines data structures for storing and managing chat sessions
//! within the SkillWorkspace, enabling persistent conversations with agents.
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │                    Chat Session Storage                       │
//! │                                                               │
//! │  ChatSession                                                  │
//! │  ├── id: "session-abc123"                                    │
//! │  ├── agent_id: "research-agent"                              │
//! │  ├── model: "claude-sonnet-4-20250514"                       │
//! │  ├── messages: [ChatMessage, ChatMessage, ...]               │
//! │  └── metadata: { total_tokens: 1500, message_count: 5 }      │
//! │                                                               │
//! │  ChatMessage                                                  │
//! │  ├── role: User | Assistant | System                         │
//! │  ├── content: "Hello, can you help me..."                    │
//! │  ├── timestamp: 1706567890000                                │
//! │  └── execution: Option<MessageExecution>                     │
//! └──────────────────────────────────────────────────────────────┘
//! ```

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Role of a message sender in a chat session.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq, Default)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    /// Message from the user
    #[default]
    User,
    /// Message from the AI assistant
    Assistant,
    /// System message (instructions, context)
    System,
}

/// Status of message execution (distinct from workflow ExecutionStatus).
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq, Default)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ChatExecutionStatus {
    /// Execution is in progress
    #[default]
    Running,
    /// Execution completed successfully
    Completed,
    /// Execution failed with error
    Failed,
}

/// Information about a single execution step.
///
/// Tracks individual steps taken during agent execution, such as
/// tool calls, API requests, or thinking processes.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct ExecutionStepInfo {
    /// Type of step (e.g., "tool_call", "api_request", "thinking")
    pub step_type: String,
    /// Human-readable name of the step
    pub name: String,
    /// Current status of this step
    pub status: String,
    /// Duration of this step in milliseconds (if completed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

impl ExecutionStepInfo {
    /// Create a new execution step info.
    pub fn new(step_type: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            step_type: step_type.into(),
            name: name.into(),
            status: "running".to_string(),
            duration_ms: None,
        }
    }

    /// Set the status of this step.
    pub fn with_status(mut self, status: impl Into<String>) -> Self {
        self.status = status.into();
        self
    }

    /// Set the duration of this step.
    pub fn with_duration(mut self, duration_ms: u64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }
}

/// Execution details for an assistant message.
///
/// Contains information about what the agent did to generate the response,
/// including tool calls, duration, and token usage.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct MessageExecution {
    /// Individual steps taken during execution
    pub steps: Vec<ExecutionStepInfo>,
    /// Total execution duration in milliseconds
    pub duration_ms: u64,
    /// Number of tokens used for this response
    pub tokens_used: u32,
    /// Cost in USD for this response
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    /// Input tokens for this response
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u32>,
    /// Output tokens for this response
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u32>,
    /// Overall execution status
    pub status: ChatExecutionStatus,
}

impl Default for MessageExecution {
    fn default() -> Self {
        Self {
            steps: Vec::new(),
            duration_ms: 0,
            tokens_used: 0,
            cost_usd: None,
            input_tokens: None,
            output_tokens: None,
            status: ChatExecutionStatus::Running,
        }
    }
}

impl MessageExecution {
    /// Create a new message execution tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an execution step.
    pub fn add_step(&mut self, step: ExecutionStepInfo) {
        self.steps.push(step);
    }

    /// Mark execution as completed.
    pub fn complete(mut self, duration_ms: u64, tokens_used: u32) -> Self {
        self.duration_ms = duration_ms;
        self.tokens_used = tokens_used;
        self.status = ChatExecutionStatus::Completed;
        self
    }

    /// Mark execution as failed.
    pub fn fail(mut self, duration_ms: u64) -> Self {
        self.duration_ms = duration_ms;
        self.status = ChatExecutionStatus::Failed;
        self
    }
}

/// A single message in a chat session.
///
/// Represents either a user message, assistant response, or system instruction.
/// Assistant messages may include execution details showing what the agent did.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct ChatMessage {
    /// Unique identifier for this message
    #[serde(default = "new_message_id")]
    pub id: String,
    /// Role of the message sender
    pub role: ChatRole,
    /// Message content (text)
    pub content: String,
    /// Unix timestamp in milliseconds when the message was created
    pub timestamp: i64,
    /// Execution details for assistant messages
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution: Option<MessageExecution>,
}

fn new_message_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

impl ChatMessage {
    /// Create a new user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            id: new_message_id(),
            role: ChatRole::User,
            content: content.into(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            execution: None,
        }
    }

    /// Create a new assistant message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            id: new_message_id(),
            role: ChatRole::Assistant,
            content: content.into(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            execution: None,
        }
    }

    /// Create a new system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            id: new_message_id(),
            role: ChatRole::System,
            content: content.into(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            execution: None,
        }
    }

    /// Add execution details to an assistant message.
    pub fn with_execution(mut self, execution: MessageExecution) -> Self {
        self.execution = Some(execution);
        self
    }
}

/// Metadata for a chat session.
///
/// Tracks aggregate statistics about the session.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct ChatSessionMetadata {
    /// Total tokens used across all messages
    pub total_tokens: u32,
    /// Number of messages in the session
    pub message_count: u32,
    /// Last model used (may differ from session default)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_model: Option<String>,
}

impl ChatSessionMetadata {
    /// Create new empty metadata.
    pub fn new() -> Self {
        Self::default()
    }

    /// Update metadata after adding a message.
    pub fn update(&mut self, tokens: u32, model: Option<String>) {
        self.total_tokens += tokens;
        self.message_count += 1;
        if let Some(m) = model {
            self.last_model = Some(m);
        }
    }
}

/// A chat session containing conversation history with an agent.
///
/// Sessions persist conversations across application restarts and can be
/// associated with specific skills for context-aware interactions.
///
/// # Example
///
/// ```rust
/// use restflow_core::models::chat_session::{ChatSession, ChatMessage};
///
/// let mut session = ChatSession::new(
///     "research-agent".to_string(),
///     "claude-sonnet-4-20250514".to_string(),
/// );
///
/// session.add_message(ChatMessage::user("Hello!"));
/// session.add_message(ChatMessage::assistant("Hi there! How can I help?"));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct ChatSession {
    /// Unique identifier for this session
    pub id: String,
    /// Human-readable session name
    pub name: String,
    /// ID of the agent this session is with
    pub agent_id: String,
    /// Default model for this session
    pub model: String,
    /// Ordered list of messages in the conversation
    pub messages: Vec<ChatMessage>,
    /// Unix timestamp in milliseconds when the session was created
    pub created_at: i64,
    /// Unix timestamp in milliseconds when the session was last updated
    pub updated_at: i64,
    /// Optional skill ID for context-aware sessions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_id: Option<String>,
    /// Optional per-session retention policy (e.g., "1h", "1d", "7d", "30d")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention: Option<String>,
    /// Summary message pointer for compacted sessions
    #[serde(default)]
    pub summary_message_id: Option<String>,
    /// Cumulative prompt tokens used in this session
    #[serde(default)]
    pub prompt_tokens: i64,
    /// Cumulative completion tokens used in this session
    #[serde(default)]
    pub completion_tokens: i64,
    /// Total cost accumulated for this session (including compaction)
    #[serde(default)]
    pub cost: f64,
    /// Session metadata (tokens, message count, etc.)
    pub metadata: ChatSessionMetadata,
}

/// Partial update payload for a chat session.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default, PartialEq)]
#[ts(export)]
pub struct ChatSessionUpdate {
    pub agent_id: Option<String>,
    pub model: Option<String>,
    pub name: Option<String>,
}

impl ChatSession {
    /// Create a new chat session with the given agent and model.
    pub fn new(agent_id: String, model: String) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: "New Chat".to_string(),
            agent_id,
            model,
            messages: Vec::new(),
            created_at: now,
            updated_at: now,
            skill_id: None,
            retention: None,
            summary_message_id: None,
            prompt_tokens: 0,
            completion_tokens: 0,
            cost: 0.0,
            metadata: ChatSessionMetadata::new(),
        }
    }

    /// Create a new chat session with a custom name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Associate the session with a skill.
    pub fn with_skill(mut self, skill_id: impl Into<String>) -> Self {
        self.skill_id = Some(skill_id.into());
        self
    }

    /// Set an optional retention policy for this session.
    pub fn with_retention(mut self, retention: impl Into<String>) -> Self {
        self.retention = Some(retention.into());
        self
    }

    /// Add a message to the session.
    pub fn add_message(&mut self, message: ChatMessage) {
        // Update metadata
        if let Some(ref exec) = message.execution {
            self.metadata.update(exec.tokens_used, None);
        } else {
            self.metadata.message_count += 1;
        }

        self.messages.push(message);
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }

    /// Rename the session.
    pub fn rename(&mut self, name: impl Into<String>) {
        self.name = name.into();
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }

    /// Generate a session name from the first user message.
    ///
    /// Truncates to 30 characters with ellipsis if needed.
    pub fn auto_name_from_first_message(&mut self) {
        if let Some(msg) = self.messages.iter().find(|m| m.role == ChatRole::User) {
            let name: String = msg.content.chars().take(30).collect();
            self.name = if msg.content.chars().count() > 30 {
                format!("{}...", name)
            } else {
                name
            };
            self.updated_at = chrono::Utc::now().timestamp_millis();
        }
    }

    /// Get the last N messages from the session.
    pub fn last_messages(&self, n: usize) -> &[ChatMessage] {
        let start = self.messages.len().saturating_sub(n);
        &self.messages[start..]
    }
}

/// Summary view of a chat session (for listing).
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct ChatSessionSummary {
    /// Session ID
    pub id: String,
    /// Session name
    pub name: String,
    /// Agent ID
    pub agent_id: String,
    /// Model used
    pub model: String,
    /// Optional skill ID for context-aware sessions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_id: Option<String>,
    /// Number of messages
    pub message_count: u32,
    /// Last update timestamp
    pub updated_at: i64,
    /// Preview of last message (truncated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message_preview: Option<String>,
}

impl From<&ChatSession> for ChatSessionSummary {
    fn from(session: &ChatSession) -> Self {
        let last_message_preview = session.messages.last().map(|m| {
            let preview: String = m.content.chars().take(50).collect();
            if m.content.chars().count() > 50 {
                format!("{}...", preview)
            } else {
                preview
            }
        });

        Self {
            id: session.id.clone(),
            name: session.name.clone(),
            agent_id: session.agent_id.clone(),
            model: session.model.clone(),
            skill_id: session.skill_id.clone(),
            message_count: session.metadata.message_count,
            updated_at: session.updated_at,
            last_message_preview,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_role_default() {
        assert_eq!(ChatRole::default(), ChatRole::User);
    }

    #[test]
    fn test_execution_status_default() {
        assert_eq!(ChatExecutionStatus::default(), ChatExecutionStatus::Running);
    }

    #[test]
    fn test_execution_step_info_new() {
        let step = ExecutionStepInfo::new("tool_call", "Search files");
        assert_eq!(step.step_type, "tool_call");
        assert_eq!(step.name, "Search files");
        assert_eq!(step.status, "running");
        assert!(step.duration_ms.is_none());
    }

    #[test]
    fn test_execution_step_info_with_status_and_duration() {
        let step = ExecutionStepInfo::new("api_call", "Call LLM")
            .with_status("completed")
            .with_duration(150);
        assert_eq!(step.status, "completed");
        assert_eq!(step.duration_ms, Some(150));
    }

    #[test]
    fn test_message_execution_complete() {
        let mut exec = MessageExecution::new();
        exec.add_step(ExecutionStepInfo::new("thinking", "Planning"));
        let exec = exec.complete(1500, 250);

        assert_eq!(exec.status, ChatExecutionStatus::Completed);
        assert_eq!(exec.duration_ms, 1500);
        assert_eq!(exec.tokens_used, 250);
        assert_eq!(exec.steps.len(), 1);
    }

    #[test]
    fn test_message_execution_fail() {
        let exec = MessageExecution::new().fail(500);
        assert_eq!(exec.status, ChatExecutionStatus::Failed);
        assert_eq!(exec.duration_ms, 500);
    }

    #[test]
    fn test_chat_message_user() {
        let msg = ChatMessage::user("Hello!");
        assert_eq!(msg.role, ChatRole::User);
        assert_eq!(msg.content, "Hello!");
        assert!(msg.execution.is_none());
    }

    #[test]
    fn test_chat_message_assistant() {
        let msg = ChatMessage::assistant("Hi there!");
        assert_eq!(msg.role, ChatRole::Assistant);
        assert_eq!(msg.content, "Hi there!");
    }

    #[test]
    fn test_chat_message_system() {
        let msg = ChatMessage::system("You are a helpful assistant.");
        assert_eq!(msg.role, ChatRole::System);
    }

    #[test]
    fn test_chat_message_with_execution() {
        let exec = MessageExecution::new().complete(1000, 100);
        let msg = ChatMessage::assistant("Done!").with_execution(exec);
        assert!(msg.execution.is_some());
        assert_eq!(msg.execution.unwrap().tokens_used, 100);
    }

    #[test]
    fn test_chat_session_new() {
        let session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
        assert!(!session.id.is_empty());
        assert_eq!(session.name, "New Chat");
        assert_eq!(session.agent_id, "agent-1");
        assert_eq!(session.model, "claude-sonnet-4");
        assert!(session.messages.is_empty());
        assert!(session.skill_id.is_none());
    }

    #[test]
    fn test_chat_session_with_name() {
        let session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string())
            .with_name("My Coding Session");
        assert_eq!(session.name, "My Coding Session");
    }

    #[test]
    fn test_chat_session_with_skill() {
        let session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string())
            .with_skill("skill-123");
        assert_eq!(session.skill_id, Some("skill-123".to_string()));
    }

    #[test]
    fn test_chat_session_with_retention() {
        let session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string())
            .with_retention("7d");
        assert_eq!(session.retention, Some("7d".to_string()));
    }

    #[test]
    fn test_chat_session_add_message() {
        let mut session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
        let initial_updated = session.updated_at;

        // Small delay to ensure timestamp changes
        std::thread::sleep(std::time::Duration::from_millis(1));

        session.add_message(ChatMessage::user("Hello!"));
        assert_eq!(session.messages.len(), 1);
        assert_eq!(session.metadata.message_count, 1);
        assert!(session.updated_at >= initial_updated);
    }

    #[test]
    fn test_chat_session_rename() {
        let mut session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
        session.rename("Renamed Session");
        assert_eq!(session.name, "Renamed Session");
    }

    #[test]
    fn test_chat_session_auto_name_short() {
        let mut session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
        session.add_message(ChatMessage::user("Help me debug"));
        session.auto_name_from_first_message();
        assert_eq!(session.name, "Help me debug");
    }

    #[test]
    fn test_chat_session_auto_name_long() {
        let mut session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
        session.add_message(ChatMessage::user(
            "This is a very long message that should be truncated to thirty characters",
        ));
        session.auto_name_from_first_message();
        assert!(session.name.ends_with("..."));
        assert!(session.name.len() <= 33); // 30 chars + "..."
    }

    #[test]
    fn test_chat_session_last_messages() {
        let mut session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
        session.add_message(ChatMessage::user("Message 1"));
        session.add_message(ChatMessage::assistant("Response 1"));
        session.add_message(ChatMessage::user("Message 2"));
        session.add_message(ChatMessage::assistant("Response 2"));

        let last_two = session.last_messages(2);
        assert_eq!(last_two.len(), 2);
        assert_eq!(last_two[0].content, "Message 2");
        assert_eq!(last_two[1].content, "Response 2");
    }

    #[test]
    fn test_chat_session_summary_from() {
        let mut session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string())
            .with_name("Test Session");
        session.add_message(ChatMessage::user("Hello!"));

        let summary = ChatSessionSummary::from(&session);
        assert_eq!(summary.id, session.id);
        assert_eq!(summary.name, "Test Session");
        assert_eq!(summary.agent_id, "agent-1");
        assert_eq!(summary.message_count, 1);
        assert_eq!(summary.last_message_preview, Some("Hello!".to_string()));
    }

    #[test]
    fn test_chat_session_summary_truncates_preview() {
        let mut session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
        session.add_message(ChatMessage::user(
            "This is a very long message that exceeds fifty characters and should be truncated",
        ));

        let summary = ChatSessionSummary::from(&session);
        assert!(summary.last_message_preview.unwrap().ends_with("..."));
    }

    #[test]
    fn test_chat_session_metadata_update() {
        let mut metadata = ChatSessionMetadata::new();
        metadata.update(100, Some("claude-opus-4".to_string()));

        assert_eq!(metadata.total_tokens, 100);
        assert_eq!(metadata.message_count, 1);
        assert_eq!(metadata.last_model, Some("claude-opus-4".to_string()));
    }

    // TypeScript binding export tests
    #[test]
    fn export_bindings_chat_role() {
        ChatRole::export_to_string(&ts_rs::Config::default()).unwrap();
    }

    #[test]
    fn export_bindings_chat_execution_status() {
        ChatExecutionStatus::export_to_string(&ts_rs::Config::default()).unwrap();
    }

    #[test]
    fn export_bindings_execution_step_info() {
        ExecutionStepInfo::export_to_string(&ts_rs::Config::default()).unwrap();
    }

    #[test]
    fn export_bindings_message_execution() {
        MessageExecution::export_to_string(&ts_rs::Config::default()).unwrap();
    }

    #[test]
    fn export_bindings_chat_message() {
        ChatMessage::export_to_string(&ts_rs::Config::default()).unwrap();
    }

    #[test]
    fn export_bindings_chat_session_metadata() {
        ChatSessionMetadata::export_to_string(&ts_rs::Config::default()).unwrap();
    }

    #[test]
    fn export_bindings_chat_session() {
        ChatSession::export_to_string(&ts_rs::Config::default()).unwrap();
    }

    #[test]
    fn export_bindings_chat_session_summary() {
        ChatSessionSummary::export_to_string(&ts_rs::Config::default()).unwrap();
    }
}
