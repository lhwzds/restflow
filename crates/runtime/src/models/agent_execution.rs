//! Agent execution types for visualization
//!
//! These types are used to capture and display agent execution details,
//! including iteration steps, tool calls, and token usage.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

/// Agent execution response with details
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AgentExecuteResponse {
    /// The final response text from the agent
    pub response: String,
    /// Optional execution details for visualization
    pub execution_details: Option<ExecutionDetails>,
}

/// Execution details for visualization
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ExecutionDetails {
    /// Number of ReAct loop iterations
    pub iterations: usize,
    /// Total tokens used (input + output)
    pub total_tokens: u32,
    /// List of execution steps
    pub steps: Vec<ExecutionStep>,
    /// Final status: "completed", "failed", "max_iterations", etc.
    pub status: String,
}

/// Individual execution step
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ExecutionStep {
    /// Step type: "system" | "user" | "assistant" | "tool_call" | "tool_result"
    pub step_type: String,
    /// Content of the step (message text or tool result)
    pub content: String,
    /// Tool calls made in this step (for assistant messages)
    pub tool_calls: Option<Vec<ToolCallInfo>>,
}

/// Tool call information
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ToolCallInfo {
    /// Unique identifier for this tool call
    pub id: String,
    /// Name of the tool being called
    pub name: String,
    /// Arguments passed to the tool (JSON object)
    pub arguments: Value,
}
