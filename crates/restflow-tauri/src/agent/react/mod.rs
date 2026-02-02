//! ReAct loop types for the unified agent.

mod parser;
mod state;

pub use parser::ResponseParser;
pub use state::{AgentState, ConversationHistory};

use restflow_ai::llm::ToolCall;
use serde::{Deserialize, Serialize};

/// Action determined by the agent after an LLM response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentAction {
    /// One or more tool calls requested by the LLM.
    ToolCalls { calls: Vec<ToolCall> },
    /// A final response to return to the user.
    FinalAnswer { content: String },
    /// Continue the loop without a terminal response.
    Continue,
}

/// ReAct loop configuration.
#[derive(Debug, Clone)]
pub struct ReActConfig {
    /// Maximum iterations before terminating.
    pub max_iterations: usize,
    /// Whether to include reasoning in output.
    pub include_reasoning: bool,
}

impl Default for ReActConfig {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            include_reasoning: false,
        }
    }
}
