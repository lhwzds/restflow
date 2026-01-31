//! Chat stream events for real-time message generation.
//!
//! These events are emitted via Tauri's event system to notify the frontend
//! of streaming progress during AI response generation.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Event name for chat stream events
pub const CHAT_STREAM_EVENT: &str = "chat:stream";

/// A chat stream event emitted during message generation
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../web/src/types/generated/")]
pub struct ChatStreamEvent {
    /// Session ID this event belongs to
    pub session_id: String,
    /// Message ID being generated
    pub message_id: String,
    /// Event timestamp (Unix ms)
    pub timestamp: i64,
    /// Event payload
    pub kind: ChatStreamKind,
}

/// Types of chat stream events
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../web/src/types/generated/")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatStreamKind {
    /// Stream started
    Started {
        /// Model being used
        model: String,
    },

    /// Token chunk received
    Token {
        /// The text chunk
        text: String,
        /// Cumulative token count (approximate)
        token_count: u32,
    },

    /// AI is thinking/reasoning (for models that support extended thinking)
    Thinking {
        /// Thinking content
        content: String,
    },

    /// Tool call initiated
    ToolCallStart {
        /// Unique ID for this tool call
        tool_id: String,
        /// Name of the tool being called
        tool_name: String,
        /// JSON-encoded arguments
        arguments: String,
    },

    /// Tool call completed
    ToolCallEnd {
        /// Tool call ID
        tool_id: String,
        /// Result of the tool call
        result: String,
        /// Whether the call succeeded
        success: bool,
    },

    /// Execution step update
    Step {
        /// Type of step (e.g., "tool_call", "api_request")
        step_type: String,
        /// Step name/description
        name: String,
        /// Current status
        status: StepStatus,
    },

    /// Usage statistics update
    Usage {
        /// Input tokens consumed
        input_tokens: u32,
        /// Output tokens generated
        output_tokens: u32,
        /// Total tokens
        total_tokens: u32,
    },

    /// Stream completed successfully
    Completed {
        /// Full response content
        full_content: String,
        /// Total duration in milliseconds
        duration_ms: u64,
        /// Total tokens used
        total_tokens: u32,
    },

    /// Stream failed with error
    Failed {
        /// Error message
        error: String,
        /// Partial content generated before failure
        partial_content: Option<String>,
    },

    /// Stream cancelled by user
    Cancelled {
        /// Partial content generated before cancellation
        partial_content: Option<String>,
    },
}

/// Status of an execution step
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../web/src/types/generated/")]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    /// Step is pending
    Pending,
    /// Step is currently running
    Running,
    /// Step completed successfully
    Completed,
    /// Step failed
    Failed,
}

impl ChatStreamEvent {
    /// Create a new timestamp
    fn now() -> i64 {
        chrono::Utc::now().timestamp_millis()
    }

    /// Create a stream started event
    pub fn started(session_id: &str, message_id: &str, model: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            timestamp: Self::now(),
            kind: ChatStreamKind::Started {
                model: model.to_string(),
            },
        }
    }

    /// Create a token event
    pub fn token(session_id: &str, message_id: &str, text: &str, token_count: u32) -> Self {
        Self {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            timestamp: Self::now(),
            kind: ChatStreamKind::Token {
                text: text.to_string(),
                token_count,
            },
        }
    }

    /// Create a thinking event
    pub fn thinking(session_id: &str, message_id: &str, content: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            timestamp: Self::now(),
            kind: ChatStreamKind::Thinking {
                content: content.to_string(),
            },
        }
    }

    /// Create a tool call start event
    pub fn tool_call_start(
        session_id: &str,
        message_id: &str,
        tool_id: &str,
        tool_name: &str,
        arguments: &str,
    ) -> Self {
        Self {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            timestamp: Self::now(),
            kind: ChatStreamKind::ToolCallStart {
                tool_id: tool_id.to_string(),
                tool_name: tool_name.to_string(),
                arguments: arguments.to_string(),
            },
        }
    }

    /// Create a tool call end event
    pub fn tool_call_end(
        session_id: &str,
        message_id: &str,
        tool_id: &str,
        result: &str,
        success: bool,
    ) -> Self {
        Self {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            timestamp: Self::now(),
            kind: ChatStreamKind::ToolCallEnd {
                tool_id: tool_id.to_string(),
                result: result.to_string(),
                success,
            },
        }
    }

    /// Create a usage event
    pub fn usage(
        session_id: &str,
        message_id: &str,
        input_tokens: u32,
        output_tokens: u32,
    ) -> Self {
        Self {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            timestamp: Self::now(),
            kind: ChatStreamKind::Usage {
                input_tokens,
                output_tokens,
                total_tokens: input_tokens + output_tokens,
            },
        }
    }

    /// Create a completed event
    pub fn completed(
        session_id: &str,
        message_id: &str,
        full_content: &str,
        duration_ms: u64,
        total_tokens: u32,
    ) -> Self {
        Self {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            timestamp: Self::now(),
            kind: ChatStreamKind::Completed {
                full_content: full_content.to_string(),
                duration_ms,
                total_tokens,
            },
        }
    }

    /// Create a failed event
    pub fn failed(session_id: &str, message_id: &str, error: &str, partial_content: Option<String>) -> Self {
        Self {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            timestamp: Self::now(),
            kind: ChatStreamKind::Failed {
                error: error.to_string(),
                partial_content,
            },
        }
    }

    /// Create a cancelled event
    pub fn cancelled(session_id: &str, message_id: &str, partial_content: Option<String>) -> Self {
        Self {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            timestamp: Self::now(),
            kind: ChatStreamKind::Cancelled { partial_content },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_serialization() {
        let event = ChatStreamEvent::started("session-1", "msg-1", "claude-3-opus");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("started"));
        assert!(json.contains("claude-3-opus"));
    }

    #[test]
    fn test_token_event() {
        let event = ChatStreamEvent::token("s1", "m1", "Hello", 1);
        match &event.kind {
            ChatStreamKind::Token { text, token_count } => {
                assert_eq!(text, "Hello");
                assert_eq!(*token_count, 1);
            }
            _ => panic!("Expected Token event"),
        }
    }
}
