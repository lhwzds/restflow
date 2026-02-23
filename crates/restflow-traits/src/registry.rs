//! Tool registry for managing available tools

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use crate::error::{Result, ToolError};
use crate::tool::{Tool, ToolOutput, ToolSchema};
use crate::wrapper::{ToolWrapper, WrappedTool};

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

    /// Register a tool with wrapper decorators.
    pub fn register_wrapped_arc(
        &mut self,
        tool: Arc<dyn Tool>,
        wrappers: Vec<Arc<dyn ToolWrapper>>,
    ) {
        let wrapped = Arc::new(WrappedTool::new(tool, wrappers));
        let name = wrapped.name().to_string();
        self.tools.insert(name, wrapped);
    }

    /// Register a concrete tool with wrapper decorators.
    pub fn register_wrapped<T: Tool + 'static>(
        &mut self,
        tool: T,
        wrappers: Vec<Arc<dyn ToolWrapper>>,
    ) {
        self.register_wrapped_arc(Arc::new(tool), wrappers);
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
            .ok_or_else(|| ToolError::NotFound(name.to_string()))?;
        tool.execute(input).await
    }

    /// Execute a tool by name.
    /// All tools execute in parallel; concurrency is controlled by the executor's semaphore.
    pub async fn execute_safe(&self, name: &str, input: Value) -> Result<ToolOutput> {
        self.execute(name, input).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_registry_empty() {
        let registry = ToolRegistry::new();
        assert!(!registry.has("unknown"));
        assert_eq!(registry.list().len(), 0);
        assert_eq!(registry.schemas().len(), 0);
    }

    #[tokio::test]
    async fn test_execute_not_found() {
        let registry = ToolRegistry::new();
        let result = registry.execute("missing", serde_json::json!({})).await;
        assert!(result.is_err());
    }
}
