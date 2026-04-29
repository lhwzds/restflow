//! # codocia
//!
//! Event owns shared stream and trace event types.
//!
//! ## Owns
//! - text events
//! - tool call events
//! - tool result events
//! - error and completion events
//!
//! ## Must Not
//! - persist events directly
//! - render UI
//! - call tools
//!
//! ## Inputs
//! - runtime state changes
//! - tool execution updates
//!
//! ## Outputs
//! - Event
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
