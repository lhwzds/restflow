//! Tools module - Agent tool registry
//!
//! Tools are capabilities that an agent can invoke.
//! Sources:
//! - Rust built-in tools
//! - Python functions via @tool decorator
//! - restflow-workflow nodes (HTTP, Email, etc.)
//! - External MCP servers

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// JSON Schema for tool parameters
pub type JsonSchema = serde_json::Value;

/// Tool output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    /// Output data
    pub data: serde_json::Value,
    /// Error message if tool failed
    pub error: Option<String>,
}

impl ToolOutput {
    /// Create successful output
    pub fn success(data: serde_json::Value) -> Self {
        Self { data, error: None }
    }

    /// Create error output
    pub fn error(message: &str) -> Self {
        Self {
            data: serde_json::Value::Null,
            error: Some(message.to_string()),
        }
    }
}

/// Tool trait for agent capabilities
#[async_trait]
pub trait Tool: Send + Sync {
    /// Unique tool name
    fn name(&self) -> &str;

    /// Human-readable description for LLM
    fn description(&self) -> &str;

    /// JSON Schema for parameters
    fn parameters_schema(&self) -> JsonSchema;

    /// Execute the tool
    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<ToolOutput>;
}

/// Tool can come from multiple sources
#[derive(Clone)]
pub enum ToolSource {
    /// Rust built-in tool
    Builtin(Arc<dyn Tool>),

    /// Python function via @tool decorator
    #[cfg(feature = "python")]
    Python {
        func: pyo3::PyObject,
        schema: JsonSchema,
    },

    /// External MCP server tool
    Mcp {
        server_url: String,
        tool_name: String,
    },
}

/// Tool registry for dynamic registration
pub struct ToolRegistry {
    tools: HashMap<String, ToolSource>,
}

impl ToolRegistry {
    /// Create a new tool registry
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a built-in tool
    pub fn register(&mut self, tool: impl Tool + 'static) {
        let name = tool.name().to_string();
        self.tools.insert(name, ToolSource::Builtin(Arc::new(tool)));
    }

    /// Register an MCP tool
    pub fn register_mcp(&mut self, name: &str, server_url: &str, tool_name: &str) {
        self.tools.insert(
            name.to_string(),
            ToolSource::Mcp {
                server_url: server_url.to_string(),
                tool_name: tool_name.to_string(),
            },
        );
    }

    /// Get a tool by name
    pub fn get(&self, name: &str) -> Option<&ToolSource> {
        self.tools.get(name)
    }

    /// List all tool names
    pub fn list(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }

    /// Get tool schema for LLM
    pub fn get_schema(&self, name: &str) -> Option<JsonSchema> {
        self.tools.get(name).map(|source| match source {
            ToolSource::Builtin(tool) => tool.parameters_schema(),
            #[cfg(feature = "python")]
            ToolSource::Python { schema, .. } => schema.clone(),
            ToolSource::Mcp { .. } => {
                // TODO: Fetch schema from MCP server
                serde_json::json!({})
            }
        })
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Built-in Tools
// ============================================================================

/// Simple calculator tool
pub struct CalculatorTool;

#[async_trait]
impl Tool for CalculatorTool {
    fn name(&self) -> &str {
        "calculator"
    }

    fn description(&self) -> &str {
        "Evaluates mathematical expressions. Input should be a valid math expression like '2 + 2' or '(10 * 5) / 2'."
    }

    fn parameters_schema(&self) -> JsonSchema {
        serde_json::json!({
            "type": "object",
            "properties": {
                "expression": {
                    "type": "string",
                    "description": "Mathematical expression to evaluate"
                }
            },
            "required": ["expression"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<ToolOutput> {
        let expression = input
            .get("expression")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'expression' parameter"))?;

        // TODO: Implement safe expression evaluation
        // For now, return a placeholder
        Ok(ToolOutput::success(serde_json::json!({
            "result": format!("Calculated: {}", expression),
            "note": "Expression evaluation not yet implemented"
        })))
    }
}

/// Get current time tool
pub struct CurrentTimeTool;

#[async_trait]
impl Tool for CurrentTimeTool {
    fn name(&self) -> &str {
        "get_current_time"
    }

    fn description(&self) -> &str {
        "Returns the current date and time."
    }

    fn parameters_schema(&self) -> JsonSchema {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn execute(&self, _input: serde_json::Value) -> anyhow::Result<ToolOutput> {
        let now = chrono::Utc::now();
        Ok(ToolOutput::success(serde_json::json!({
            "timestamp": now.to_rfc3339(),
            "unix": now.timestamp()
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tool_registry() {
        let mut registry = ToolRegistry::new();
        registry.register(CurrentTimeTool);

        assert!(registry.get("get_current_time").is_some());
        assert!(registry.get("unknown").is_none());

        let tools = registry.list();
        assert!(tools.contains(&"get_current_time"));
    }
}
