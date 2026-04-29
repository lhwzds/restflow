//! # codocia
//!
//! Chat owns sessions, turns, and message history.
//!
//! ## Owns
//! - Session
//! - Message
//! - Role
//! - chat history composition
//!
//! ## Must Not
//! - own durable background runs
//! - render TUI layout
//! - decide model catalog policy
//!
//! ## Inputs
//! - user messages
//! - assistant events
//! - tool events
//!
//! ## Outputs
//! - session history
//! - message lists
//!
//! ## Depends On
//! - agent
//! - event
//! - skill
//! - store
//!
//! ## Verify
//! - cargo check -p chat

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
    Tool,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub messages: Vec<Message>,
}

impl Session {
    pub fn push(&mut self, message: Message) {
        self.messages.push(message);
    }
}
