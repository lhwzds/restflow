//! Tool registry for managing available tools

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use crate::error::{AiError, Result};
use crate::tools::traits::{Tool, ToolOutput, ToolSchema};

/// Registry for managing available tools
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    /// Create a new empty tool registry
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool
    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        let name = tool.name().to_string();
        self.tools.insert(name, Arc::new(tool));
    }

    /// Register a tool from Arc
    pub fn register_arc(&mut self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        self.tools.insert(name, tool);
    }

    /// Get a tool by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// Check if tool exists
    pub fn has(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// List all tool names
    pub fn list(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }

    /// Get schemas for all registered tools
    pub fn schemas(&self) -> Vec<ToolSchema> {
        self.tools.values().map(|t| t.schema()).collect()
    }

    /// Execute a tool by name
    pub async fn execute(&self, name: &str, input: Value) -> Result<ToolOutput> {
        let tool = self
            .get(name)
            .ok_or_else(|| AiError::ToolNotFound(name.to_string()))?;
        tool.execute(input).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::HttpTool;

    #[test]
    fn test_tool_registry() {
        let mut registry = ToolRegistry::new();
        registry.register(HttpTool::new());

        assert!(registry.has("http_request"));
        assert!(!registry.has("unknown"));
        assert_eq!(registry.list().len(), 1);
    }

    #[test]
    fn test_tool_schemas() {
        let mut registry = ToolRegistry::new();
        registry.register(HttpTool::new());

        let schemas = registry.schemas();
        assert_eq!(schemas.len(), 1);
        assert_eq!(schemas[0].name, "http_request");
    }
}
