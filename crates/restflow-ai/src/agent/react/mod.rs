//! ReAct (Reasoning + Acting) loop implementation.

mod parser;
mod state;

pub use parser::ResponseParser;
pub use state::{AgentState, ConversationHistory};

use serde::{Deserialize, Serialize};

/// Action determined by the agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentAction {
    /// Call a tool with arguments
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    /// Provide final answer to user
    FinalAnswer { content: String },
    /// Agent needs more thinking (internal)
    Continue,
}

/// ReAct loop configuration
#[derive(Debug, Clone)]
pub struct ReActConfig {
    /// Maximum iterations before forcing termination
    pub max_iterations: usize,
    /// Whether to include reasoning in output
    pub include_reasoning: bool,
}

impl Default for ReActConfig {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            include_reasoning: false,
        }
    }
}
