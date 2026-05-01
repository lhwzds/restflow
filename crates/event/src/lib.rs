//! # codocia
//!
//! Event owns shared stream and trace event types.
//!
//! ## Owns
//! - text events
//! - tool call events
//! - tool result events
//! - error and completion events
//! - event constructors
//!
//! ## Must Not
//! - persist events directly
//! - render UI
//! - call tools
//!
//! ## Inputs
//! - runtime state changes
//! - tool execution updates
//! - finalization updates
//!
//! ## Outputs
//! - Event
//! - terminal event detection
//!
//! ## Used By
//! - agent
//! - chat
//! - run
//!
//! ## Verify
//! - cargo check -p event

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Text {
        value: String,
    },
    ToolCall {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        id: String,
        value: serde_json::Value,
    },
    Error {
        message: String,
    },
    Done,
}

impl Event {
    pub fn text(value: impl Into<String>) -> Self {
        Self::Text {
            value: value.into(),
        }
    }

    pub fn tool_call(
        id: impl Into<String>,
        name: impl Into<String>,
        input: serde_json::Value,
    ) -> Self {
        Self::ToolCall {
            id: id.into(),
            name: name.into(),
            input,
        }
    }

    pub fn tool_result(id: impl Into<String>, value: serde_json::Value) -> Self {
        Self::ToolResult {
            id: id.into(),
            value,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::Error {
            message: message.into(),
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Done | Self::Error { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_create_expected_events() {
        assert_eq!(
            Event::text("hello"),
            Event::Text {
                value: "hello".to_string()
            }
        );
        assert_eq!(
            Event::tool_call("call-1", "echo", serde_json::json!({ "x": 1 })),
            Event::ToolCall {
                id: "call-1".to_string(),
                name: "echo".to_string(),
                input: serde_json::json!({ "x": 1 }),
            }
        );
        assert_eq!(
            Event::tool_result("call-1", serde_json::json!({ "ok": true })),
            Event::ToolResult {
                id: "call-1".to_string(),
                value: serde_json::json!({ "ok": true }),
            }
        );
    }

    #[test]
    fn terminal_detection_only_matches_done_and_error() {
        assert!(!Event::text("hello").is_terminal());
        assert!(Event::Done.is_terminal());
        assert!(Event::error("failed").is_terminal());
    }
}
