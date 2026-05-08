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

use crate::models::ModelId;
use serde::{Deserialize, Serialize};
use specta::Type;

/// Role of a message sender in a chat session.
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq, Default)]
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
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq, Default)]
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

/// Structured media type for a chat message.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChatMediaType {
    /// Voice audio message.
    Voice,
}

/// Structured media payload for a chat message.
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct ChatMessageMedia {
    /// Media kind.
    pub media_type: ChatMediaType,
    /// Local file path for this media asset.
    pub file_path: String,
    /// Optional media duration in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_sec: Option<u32>,
}

impl ChatMessageMedia {
    /// Create a voice media descriptor.
    pub fn voice(file_path: impl Into<String>, duration_sec: Option<u32>) -> Self {
        Self {
            media_type: ChatMediaType::Voice,
            file_path: file_path.into(),
            duration_sec,
        }
    }
}

/// Structured transcript payload for a chat message.
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct ChatMessageTranscript {
    /// Final transcript text.
    pub text: String,
    /// Optional model identifier used for transcription.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Optional update timestamp in Unix milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

impl ChatMessageTranscript {
    /// Create transcript payload with optional model metadata.
    pub fn new(text: impl Into<String>, model: Option<String>) -> Self {
        Self {
            text: text.into(),
            model,
            updated_at: Some(chrono::Utc::now().timestamp_millis()),
        }
    }
}

/// Information about a single execution step.
///
/// Tracks individual steps taken during agent execution, such as
/// tool calls, API requests, or thinking processes.
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
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
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
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
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
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
    /// Optional structured media metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<ChatMessageMedia>,
    /// Optional structured transcript metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript: Option<ChatMessageTranscript>,
}

/// Lifecycle state for a user-visible conversation turn.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChatTurnStatus {
    /// The turn is currently executing.
    #[default]
    Running,
    /// The turn finished with an assistant response.
    Completed,
    /// The turn was interrupted or canceled by the user.
    Canceled,
    /// The turn failed before producing a final response.
    Failed,
}

/// User-visible event inside a chat turn.
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatTurnEventKind {
    /// User input that started the turn.
    UserMessage { content: String },
    /// Final or partial assistant text captured for this turn.
    AssistantMessage { content: String },
    /// A tool call started during the turn.
    ToolCall {
        call_id: String,
        name: String,
        arguments: String,
    },
    /// A tool call completed during the turn.
    ToolResult {
        call_id: String,
        success: bool,
        result: String,
    },
    /// The runtime reported an error for this turn.
    Error { message: String },
    /// The user canceled or interrupted this turn.
    Canceled,
}

/// A single user-visible event in a chat turn.
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct ChatTurnEvent {
    /// Unique event identifier.
    #[serde(default = "new_message_id")]
    pub id: String,
    /// Unix timestamp in milliseconds when the event was recorded.
    pub timestamp: i64,
    /// Event payload.
    pub kind: ChatTurnEventKind,
}

impl ChatTurnEvent {
    /// Create a new turn event with the current timestamp.
    pub fn new(kind: ChatTurnEventKind) -> Self {
        Self {
            id: new_message_id(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            kind,
        }
    }
}

/// A single user turn containing ordered UI/runtime events.
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct ChatTurn {
    /// Stable turn identifier. Streaming IPC uses the stream id as the turn id.
    pub id: String,
    /// Current lifecycle state.
    pub status: ChatTurnStatus,
    /// Unix timestamp in milliseconds when the turn started.
    pub started_at: i64,
    /// Unix timestamp in milliseconds when the turn was last updated.
    pub updated_at: i64,
    /// Unix timestamp in milliseconds when the turn ended.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
    /// Ordered user-visible runtime events for this turn.
    #[serde(default)]
    pub events: Vec<ChatTurnEvent>,
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
            media: None,
            transcript: None,
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
            media: None,
            transcript: None,
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
            media: None,
            transcript: None,
        }
    }

    /// Add execution details to an assistant message.
    pub fn with_execution(mut self, execution: MessageExecution) -> Self {
        self.execution = Some(execution);
        self
    }

    /// Attach structured media metadata.
    pub fn with_media(mut self, media: ChatMessageMedia) -> Self {
        self.media = Some(media);
        self
    }

    /// Attach structured transcript metadata.
    pub fn with_transcript(mut self, transcript: ChatMessageTranscript) -> Self {
        self.transcript = Some(transcript);
        self
    }
}

/// Metadata for a chat session.
///
/// Tracks aggregate statistics about the session.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type, PartialEq)]
pub struct ChatSessionMetadata {
    /// Total tokens used across all messages
    pub total_tokens: u32,
    /// Number of messages in the session
    pub message_count: u32,
}

impl ChatSessionMetadata {
    /// Create new empty metadata.
    pub fn new() -> Self {
        Self::default()
    }

    /// Update metadata after adding a message.
    pub fn update(&mut self, tokens: u32) {
        self.total_tokens += tokens;
        self.message_count += 1;
    }
}

/// Origin of a chat session.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChatSessionSource {
    /// Created from workspace UI / local API entrypoints.
    Workspace,
    /// Created for durable task execution.
    Background,
}

/// A chat session containing conversation history with an agent.
///
/// Sessions persist conversations across application restarts and can be
/// associated with specific skills for context-aware interactions.
///
/// # Example
///
/// ```rust
/// use runtime::models::chat_session::{ChatSession, ChatMessage};
///
/// let mut session = ChatSession::new(
///     "research-agent".to_string(),
///     "claude-sonnet-4-20250514".to_string(),
/// );
///
/// session.add_message(ChatMessage::user("Hello!"));
/// session.add_message(ChatMessage::assistant("Hi there! How can I help?"));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct ChatSession {
    /// Unique identifier for this session
    pub id: String,
    /// Human-readable session name
    pub name: String,
    /// ID of the agent this session is with
    pub agent_id: String,
    /// Current provider for this session
    #[serde(default)]
    pub provider: String,
    /// Current model for this session
    pub model: String,
    /// Ordered list of messages in the conversation
    pub messages: Vec<ChatMessage>,
    /// Ordered turn/event history used by terminal UI projection and replay.
    #[serde(default)]
    pub turns: Vec<ChatTurn>,
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
    /// Optional origin channel of this session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_channel: Option<ChatSessionSource>,
    /// Optional channel-specific conversation identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_conversation_id: Option<String>,
    /// Unix timestamp in milliseconds when the session was archived.
    /// None means the session is active.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<i64>,
}

/// Partial update payload for a chat session.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Default, PartialEq)]
pub struct ChatSessionUpdate {
    pub agent_id: Option<String>,
    pub model: Option<String>,
    pub name: Option<String>,
}

impl ChatSession {
    pub fn resolve_model_identity(model: &str) -> (String, String) {
        if let Some(model_id) = ModelId::from_api_name(model)
            .or_else(|| ModelId::from_canonical_id(model))
            .or_else(|| ModelId::from_serialized_str(model))
        {
            return (
                model_id.provider().as_canonical_str().to_string(),
                model_id.as_serialized_str().to_string(),
            );
        }

        let normalized =
            ModelId::normalize_model_id(model).unwrap_or_else(|| model.trim().to_string());
        let provider = ModelId::from_serialized_str(&normalized)
            .map(|model_id| model_id.provider().as_canonical_str().to_string())
            .unwrap_or_default();
        (provider, normalized)
    }

    /// Create a new chat session with the given agent and model.
    pub fn new(agent_id: String, model: String) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        let (provider, model) = Self::resolve_model_identity(&model);
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: "New Chat".to_string(),
            agent_id,
            provider,
            model,
            messages: Vec::new(),
            turns: Vec::new(),
            created_at: now,
            updated_at: now,
            skill_id: None,
            retention: None,
            summary_message_id: None,
            prompt_tokens: 0,
            completion_tokens: 0,
            cost: 0.0,
            metadata: ChatSessionMetadata::new(),
            source_channel: None,
            source_conversation_id: None,
            archived_at: None,
        }
    }

    pub fn set_model_identity(&mut self, model: ModelId) {
        self.provider = model.provider().as_canonical_str().to_string();
        self.model = model.as_serialized_str().to_string();
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }

    pub fn set_model_identity_from_raw(&mut self, model: &str) {
        let (provider, normalized_model) = Self::resolve_model_identity(model);
        self.provider = provider;
        self.model = normalized_model;
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }

    pub fn hydrate_provider_from_model(&mut self) -> bool {
        let (provider, normalized_model) = Self::resolve_model_identity(&self.model);
        let changed = self.provider != provider || self.model != normalized_model;
        self.provider = provider;
        self.model = normalized_model;
        changed
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

    /// Associate this session with a managed source.
    pub fn with_source(
        mut self,
        source_channel: ChatSessionSource,
        source_conversation_id: impl Into<String>,
    ) -> Self {
        self.source_channel = Some(source_channel);
        self.source_conversation_id = Some(source_conversation_id.into());
        self
    }

    /// Maximum messages stored per session to prevent unbounded DB growth.
    const MAX_STORED_MESSAGES: usize = 200;

    /// Add a message to the session.
    pub fn add_message(&mut self, message: ChatMessage) {
        // Update metadata
        if let Some(ref exec) = message.execution {
            self.metadata.update(exec.tokens_used);
        } else {
            self.metadata.message_count += 1;
        }

        self.messages.push(message);

        // Prevent unbounded growth in long-running sessions.
        if self.messages.len() > Self::MAX_STORED_MESSAGES {
            let excess = self.messages.len() - Self::MAX_STORED_MESSAGES;
            self.messages.drain(..excess);
            self.metadata.message_count = self.messages.len() as u32;
        }

        self.updated_at = chrono::Utc::now().timestamp_millis();
    }

    fn ensure_turn_index(&mut self, turn_id: &str) -> usize {
        if let Some(index) = self.turns.iter().position(|turn| turn.id == turn_id) {
            return index;
        }

        let now = chrono::Utc::now().timestamp_millis();
        self.turns.push(ChatTurn {
            id: turn_id.to_string(),
            status: ChatTurnStatus::Running,
            started_at: now,
            updated_at: now,
            completed_at: None,
            events: Vec::new(),
        });
        self.turns.len() - 1
    }

    /// Start or update a turn with the user input that drives it.
    pub fn record_turn_user_message(&mut self, turn_id: &str, content: impl Into<String>) {
        let content = content.into();
        let index = self.ensure_turn_index(turn_id);
        let turn = &mut self.turns[index];
        turn.status = ChatTurnStatus::Running;
        turn.completed_at = None;
        let already_recorded = turn.events.iter().any(|event| {
            matches!(
                &event.kind,
                ChatTurnEventKind::UserMessage { content: existing } if existing == &content
            )
        });
        if !already_recorded {
            turn.events
                .push(ChatTurnEvent::new(ChatTurnEventKind::UserMessage {
                    content,
                }));
        }
        let now = chrono::Utc::now().timestamp_millis();
        turn.updated_at = now;
        self.updated_at = now;
    }

    /// Append an ordered event to a turn.
    pub fn record_turn_event(&mut self, turn_id: &str, kind: ChatTurnEventKind) {
        let index = self.ensure_turn_index(turn_id);
        let turn = &mut self.turns[index];
        turn.events.push(ChatTurnEvent::new(kind));
        let now = chrono::Utc::now().timestamp_millis();
        turn.updated_at = now;
        self.updated_at = now;
    }

    /// Mark a turn as completed and persist its assistant output.
    pub fn complete_turn_with_assistant_message(
        &mut self,
        turn_id: &str,
        content: impl Into<String>,
    ) {
        let content = content.into();
        let index = self.ensure_turn_index(turn_id);
        let turn = &mut self.turns[index];
        let already_recorded = turn.events.iter().any(|event| {
            matches!(
                &event.kind,
                ChatTurnEventKind::AssistantMessage { content: existing } if existing == &content
            )
        });
        if !content.trim().is_empty() && !already_recorded {
            turn.events
                .push(ChatTurnEvent::new(ChatTurnEventKind::AssistantMessage {
                    content,
                }));
        }
        let now = chrono::Utc::now().timestamp_millis();
        turn.status = ChatTurnStatus::Completed;
        turn.updated_at = now;
        turn.completed_at = Some(now);
        self.updated_at = now;
    }

    /// Mark a turn as failed and persist the error message.
    pub fn fail_turn(&mut self, turn_id: &str, message: impl Into<String>) {
        let message = message.into();
        let index = self.ensure_turn_index(turn_id);
        let turn = &mut self.turns[index];
        if !message.trim().is_empty() {
            turn.events
                .push(ChatTurnEvent::new(ChatTurnEventKind::Error { message }));
        }
        let now = chrono::Utc::now().timestamp_millis();
        turn.status = ChatTurnStatus::Failed;
        turn.updated_at = now;
        turn.completed_at = Some(now);
        self.updated_at = now;
    }

    /// Mark a turn as canceled.
    pub fn cancel_turn(&mut self, turn_id: &str) {
        let index = self.ensure_turn_index(turn_id);
        let turn = &mut self.turns[index];
        let already_recorded = turn
            .events
            .iter()
            .any(|event| matches!(event.kind, ChatTurnEventKind::Canceled));
        if !already_recorded {
            turn.events
                .push(ChatTurnEvent::new(ChatTurnEventKind::Canceled));
        }
        let now = chrono::Utc::now().timestamp_millis();
        turn.status = ChatTurnStatus::Canceled;
        turn.updated_at = now;
        turn.completed_at = Some(now);
        self.updated_at = now;
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

    /// Mark this session as archived.
    pub fn archive(&mut self) {
        let now = chrono::Utc::now().timestamp_millis();
        self.archived_at = Some(now);
        self.updated_at = now;
    }

    /// Mark this session as active.
    pub fn unarchive(&mut self) {
        self.archived_at = None;
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }

    /// Whether this session is archived.
    pub fn is_archived(&self) -> bool {
        self.archived_at.is_some()
    }
}

/// Summary view of a chat session (for listing).
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct ChatSessionSummary {
    /// Session ID
    pub id: String,
    /// Session name
    pub name: String,
    /// Agent ID
    pub agent_id: String,
    /// Provider used
    pub provider: String,
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
    /// Optional origin channel of this session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_channel: Option<ChatSessionSource>,
    /// Optional channel-specific conversation identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_conversation_id: Option<String>,
    /// Unix timestamp in milliseconds when the session was archived.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<i64>,
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
            provider: session.provider.clone(),
            model: session.model.clone(),
            skill_id: session.skill_id.clone(),
            message_count: session.metadata.message_count,
            updated_at: session.updated_at,
            last_message_preview,
            source_channel: session.source_channel,
            source_conversation_id: session.source_conversation_id.clone(),
            archived_at: session.archived_at,
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
    fn records_turn_events_without_adding_model_context_messages() {
        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());

        session.record_turn_user_message("turn-1", "hello");
        session.record_turn_event(
            "turn-1",
            ChatTurnEventKind::ToolCall {
                call_id: "call-1".to_string(),
                name: "bash".to_string(),
                arguments: "{\"cmd\":\"pwd\"}".to_string(),
            },
        );
        session.record_turn_event(
            "turn-1",
            ChatTurnEventKind::ToolResult {
                call_id: "call-1".to_string(),
                success: true,
                result: "/tmp".to_string(),
            },
        );
        session.complete_turn_with_assistant_message("turn-1", "done");

        assert!(session.messages.is_empty());
        assert_eq!(session.turns.len(), 1);
        assert_eq!(session.turns[0].status, ChatTurnStatus::Completed);
        assert_eq!(session.turns[0].events.len(), 4);
        assert!(session.turns[0].completed_at.is_some());
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
        assert!(msg.media.is_none());
        assert!(msg.transcript.is_none());
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
    fn test_chat_message_with_media_and_transcript() {
        let msg = ChatMessage::user("[Voice message]")
            .with_media(ChatMessageMedia::voice("/tmp/voice.webm", Some(8)))
            .with_transcript(ChatMessageTranscript::new(
                "hello",
                Some("whisper-1".to_string()),
            ));
        assert!(msg.media.is_some());
        assert!(msg.transcript.is_some());
        assert_eq!(
            msg.media.as_ref().map(|m| m.media_type),
            Some(ChatMediaType::Voice)
        );
        assert_eq!(
            msg.transcript.as_ref().map(|t| t.text.as_str()),
            Some("hello")
        );
    }

    #[test]
    fn test_chat_session_new() {
        let session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
        assert!(!session.id.is_empty());
        assert_eq!(session.name, "New Chat");
        assert_eq!(session.agent_id, "agent-1");
        assert_eq!(session.provider, "anthropic");
        assert_eq!(session.model, "claude-sonnet-4-5");
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
    fn test_chat_session_archive_and_unarchive() {
        let mut session = ChatSession::new("agent-1".to_string(), "claude-sonnet-4".to_string());
        assert!(!session.is_archived());
        assert!(session.archived_at.is_none());

        session.archive();
        assert!(session.is_archived());
        assert!(session.archived_at.is_some());

        session.unarchive();
        assert!(!session.is_archived());
        assert!(session.archived_at.is_none());
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
        session.archive();

        let summary = ChatSessionSummary::from(&session);
        assert_eq!(summary.id, session.id);
        assert_eq!(summary.name, "Test Session");
        assert_eq!(summary.agent_id, "agent-1");
        assert_eq!(summary.provider, session.provider);
        assert_eq!(summary.model, session.model);
        assert_eq!(summary.message_count, 1);
        assert_eq!(summary.last_message_preview, Some("Hello!".to_string()));
        assert!(summary.archived_at.is_some());
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
        metadata.update(100);

        assert_eq!(metadata.total_tokens, 100);
        assert_eq!(metadata.message_count, 1);
    }

    #[test]
    fn test_chat_session_resolves_provider_from_raw_model() {
        let (provider, model) = ChatSession::resolve_model_identity("MiniMax-M2.5");

        assert_eq!(provider, "minimax");
        assert_eq!(model, "minimax-m2-5");
    }

    #[test]
    fn test_chat_session_new_sets_model_identity() {
        let session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());

        assert_eq!(session.provider, "openai");
        assert_eq!(session.model, "gpt-5");
    }

    // TypeScript binding export tests
    #[test]
    fn test_add_message_enforces_max_stored_limit() {
        let mut session = ChatSession::new("agent-1".to_string(), "model".to_string());
        let total = ChatSession::MAX_STORED_MESSAGES + 10;

        for i in 0..total {
            if i % 2 == 0 {
                session.add_message(ChatMessage::user(format!("msg {}", i)));
            } else {
                session.add_message(ChatMessage::assistant(format!("reply {}", i)));
            }
        }

        assert_eq!(session.messages.len(), ChatSession::MAX_STORED_MESSAGES);

        // Most recent message should be retained
        let last = session.messages.last().unwrap();
        assert!(last.content.contains(&(total - 1).to_string()));
    }

    #[test]
    fn test_add_message_below_cap_is_unaffected() {
        let mut session = ChatSession::new("agent-1".to_string(), "model".to_string());
        session.add_message(ChatMessage::user("hello"));
        session.add_message(ChatMessage::assistant("hi"));
        assert_eq!(session.messages.len(), 2);
    }

    #[test]
    fn test_message_count_matches_after_drain() {
        let mut session = ChatSession::new("agent-1".to_string(), "model".to_string());
        let total = ChatSession::MAX_STORED_MESSAGES + 50;

        for i in 0..total {
            session.add_message(ChatMessage::user(format!("msg {}", i)));
        }

        assert_eq!(session.messages.len(), ChatSession::MAX_STORED_MESSAGES);
        assert_eq!(
            session.metadata.message_count,
            session.messages.len() as u32
        );
    }
}
