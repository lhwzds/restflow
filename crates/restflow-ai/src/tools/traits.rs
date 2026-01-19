//! Tool trait and types for AI agent tools

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::Result;

/// JSON Schema for tool parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: Value, // JSON Schema object
}

/// Result of tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    pub success: bool,
    pub result: Value,
    pub error: Option<String>,
}

impl ToolOutput {
    /// Create a successful tool output
    pub fn success(result: Value) -> Self {
        Self {
            success: true,
            result,
            error: None,
        }
    }

    /// Create an error tool output
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            result: Value::Null,
            error: Some(message.into()),
        }
    }
}

/// Core trait for agent tools
#[async_trait]
pub trait Tool: Send + Sync {
    /// Unique tool name (used in LLM function calls)
    fn name(&self) -> &str;

    /// Human-readable description for LLM context
    fn description(&self) -> &str;

    /// JSON Schema for input parameters
    fn parameters_schema(&self) -> Value;

    /// Execute the tool with given input
    async fn execute(&self, input: Value) -> Result<ToolOutput>;

    /// Build complete schema for LLM
    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: self.parameters_schema(),
        }
    }
}
