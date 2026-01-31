//! Universal Channel Types
//!
//! Core types for the channel-agnostic communication layer.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Channel type identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ChannelType {
    Telegram,
    Discord,
    Slack,
    Email,
    Webhook,
}

impl ChannelType {
    /// Whether this channel type supports bidirectional interaction
    pub fn supports_interaction(&self) -> bool {
        matches!(self, Self::Telegram | Self::Discord | Self::Slack)
    }

    /// Display name for UI
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Telegram => "Telegram",
            Self::Discord => "Discord",
            Self::Slack => "Slack",
            Self::Email => "Email",
            Self::Webhook => "Webhook",
        }
    }
}

impl std::fmt::Display for ChannelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Message level for formatting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum MessageLevel {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

impl MessageLevel {
    /// Get emoji representation for the message level
    pub fn emoji(&self) -> &'static str {
        match self {
            Self::Info => "ℹ️",
            Self::Success => "✅",
            Self::Warning => "⚠️",
            Self::Error => "❌",
        }
    }
}

/// Inbound message from a channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundMessage {
    /// Unique message ID
    pub id: String,
    /// Channel this message came from
    pub channel_type: ChannelType,
    /// Sender identifier (user ID in the channel)
    pub sender_id: String,
    /// Sender display name (if available)
    pub sender_name: Option<String>,
    /// Conversation identifier (chat_id, channel_id, thread_id, etc.)
    pub conversation_id: String,
    /// Message content
    pub content: String,
    /// Timestamp (milliseconds since epoch)
    pub timestamp: i64,
    /// Reply to message ID (if this is a reply)
    pub reply_to: Option<String>,
    /// Channel-specific metadata
    pub metadata: Option<serde_json::Value>,
}

impl InboundMessage {
    /// Create a new inbound message
    pub fn new(
        id: impl Into<String>,
        channel_type: ChannelType,
        sender_id: impl Into<String>,
        conversation_id: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            channel_type,
            sender_id: sender_id.into(),
            sender_name: None,
            conversation_id: conversation_id.into(),
            content: content.into(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            reply_to: None,
            metadata: None,
        }
    }

    /// Set sender name
    pub fn with_sender_name(mut self, name: impl Into<String>) -> Self {
        self.sender_name = Some(name.into());
        self
    }

    /// Set reply_to
    pub fn with_reply_to(mut self, reply_to: impl Into<String>) -> Self {
        self.reply_to = Some(reply_to.into());
        self
    }

    /// Set metadata
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Outbound message to a channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundMessage {
    /// Conversation identifier
    pub conversation_id: String,
    /// Message content (plain text or markdown)
    pub content: String,
    /// Message level for formatting
    pub level: MessageLevel,
    /// Optional title/header
    pub title: Option<String>,
    /// Reply to specific message
    pub reply_to: Option<String>,
    /// Parse mode (markdown, html, plain)
    pub parse_mode: Option<String>,
}

impl OutboundMessage {
    /// Create a new outbound message
    pub fn new(conversation_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            conversation_id: conversation_id.into(),
            content: content.into(),
            level: MessageLevel::Info,
            title: None,
            reply_to: None,
            parse_mode: Some("Markdown".to_string()),
        }
    }

    /// Set message level
    pub fn with_level(mut self, level: MessageLevel) -> Self {
        self.level = level;
        self
    }

    /// Set title
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set reply_to
    pub fn with_reply_to(mut self, reply_to: impl Into<String>) -> Self {
        self.reply_to = Some(reply_to.into());
        self
    }

    /// Set parse mode
    pub fn with_parse_mode(mut self, mode: impl Into<String>) -> Self {
        self.parse_mode = Some(mode.into());
        self
    }

    /// Create a success message
    pub fn success(conversation_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::new(conversation_id, content).with_level(MessageLevel::Success)
    }

    /// Create an error message
    pub fn error(conversation_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::new(conversation_id, content).with_level(MessageLevel::Error)
    }

    /// Create a warning message
    pub fn warning(conversation_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::new(conversation_id, content).with_level(MessageLevel::Warning)
    }

    /// Format the message with emoji prefix based on level
    pub fn formatted_content(&self) -> String {
        let emoji = self.level.emoji();
        if let Some(title) = &self.title {
            format!("{} *{}*\n\n{}", emoji, title, self.content)
        } else {
            format!("{} {}", emoji, self.content)
        }
    }
}

/// Conversation context (tracks which channel a user is using)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationContext {
    /// Conversation ID
    pub conversation_id: String,
    /// Channel type for this conversation
    pub channel_type: ChannelType,
    /// Associated task ID (if any)
    pub task_id: Option<String>,
    /// User identifier in the channel
    pub user_id: String,
    /// Last activity timestamp (milliseconds since epoch)
    pub last_activity: i64,
}

impl ConversationContext {
    /// Create a new conversation context
    pub fn new(
        conversation_id: impl Into<String>,
        channel_type: ChannelType,
        user_id: impl Into<String>,
    ) -> Self {
        Self {
            conversation_id: conversation_id.into(),
            channel_type,
            task_id: None,
            user_id: user_id.into(),
            last_activity: chrono::Utc::now().timestamp_millis(),
        }
    }

    /// Set associated task ID
    pub fn with_task_id(mut self, task_id: impl Into<String>) -> Self {
        self.task_id = Some(task_id.into());
        self
    }

    /// Update last activity timestamp
    pub fn touch(&mut self) {
        self.last_activity = chrono::Utc::now().timestamp_millis();
    }

    /// Check if context is stale (older than given duration in milliseconds)
    pub fn is_stale(&self, max_age_ms: i64) -> bool {
        let now = chrono::Utc::now().timestamp_millis();
        now - self.last_activity > max_age_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_type_supports_interaction() {
        assert!(ChannelType::Telegram.supports_interaction());
        assert!(ChannelType::Discord.supports_interaction());
        assert!(ChannelType::Slack.supports_interaction());
        assert!(!ChannelType::Email.supports_interaction());
        assert!(!ChannelType::Webhook.supports_interaction());
    }

    #[test]
    fn test_channel_type_display_name() {
        assert_eq!(ChannelType::Telegram.display_name(), "Telegram");
        assert_eq!(ChannelType::Discord.display_name(), "Discord");
    }

    #[test]
    fn test_message_level_emoji() {
        assert_eq!(MessageLevel::Info.emoji(), "ℹ️");
        assert_eq!(MessageLevel::Success.emoji(), "✅");
        assert_eq!(MessageLevel::Warning.emoji(), "⚠️");
        assert_eq!(MessageLevel::Error.emoji(), "❌");
    }

    #[test]
    fn test_outbound_message_formatting() {
        let msg = OutboundMessage::success("123", "Task completed").with_title("Build Job");
        let formatted = msg.formatted_content();
        assert!(formatted.contains("✅"));
        assert!(formatted.contains("*Build Job*"));
        assert!(formatted.contains("Task completed"));
    }

    #[test]
    fn test_inbound_message_builder() {
        let msg = InboundMessage::new(
            "msg-1",
            ChannelType::Telegram,
            "user-123",
            "chat-456",
            "Hello world",
        )
        .with_sender_name("John")
        .with_reply_to("msg-0");

        assert_eq!(msg.id, "msg-1");
        assert_eq!(msg.sender_name, Some("John".to_string()));
        assert_eq!(msg.reply_to, Some("msg-0".to_string()));
    }

    #[test]
    fn test_conversation_context_staleness() {
        let mut ctx = ConversationContext::new("conv-1", ChannelType::Telegram, "user-1");

        // Fresh context is not stale
        assert!(!ctx.is_stale(1000));

        // Manually set old timestamp
        ctx.last_activity = chrono::Utc::now().timestamp_millis() - 5000;
        assert!(ctx.is_stale(1000));
        assert!(!ctx.is_stale(10000));
    }
}
