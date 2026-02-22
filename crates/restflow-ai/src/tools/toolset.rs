//! Composable toolset abstraction.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::tools::error::Result;
use crate::tools::registry::ToolRegistry;
use crate::tools::traits::{ToolOutput, ToolSchema};

/// Runtime context for optional per-step toolset preparation.
#[derive(Debug, Clone, Default)]
pub struct ToolsetContext {
    pub step: Option<usize>,
    pub agent_id: Option<String>,
}

/// Common abstraction over different toolset implementations.
#[async_trait]
pub trait Toolset: Send + Sync {
    /// List schemas for all currently available tools.
    fn list_tools(&self) -> Vec<ToolSchema>;

    /// Call a tool by name.
    async fn call_tool(&self, name: &str, args: Value) -> Result<ToolOutput>;

    /// Call a tool with parallel-safety semantics.
    async fn call_tool_safe(&self, name: &str, args: Value) -> Result<ToolOutput>;

    /// Optional hook before each step.
    async fn prepare(&self, _context: &ToolsetContext) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl Toolset for ToolRegistry {
    fn list_tools(&self) -> Vec<ToolSchema> {
        self.schemas()
    }

    async fn call_tool(&self, name: &str, args: Value) -> Result<ToolOutput> {
        self.execute(name, args).await
    }

    async fn call_tool_safe(&self, name: &str, args: Value) -> Result<ToolOutput> {
        self.execute_safe(name, args).await
    }
}

#[async_trait]
impl<T: Toolset + ?Sized> Toolset for Arc<T> {
    fn list_tools(&self) -> Vec<ToolSchema> {
        self.as_ref().list_tools()
    }

    async fn call_tool(&self, name: &str, args: Value) -> Result<ToolOutput> {
        self.as_ref().call_tool(name, args).await
    }

    async fn call_tool_safe(&self, name: &str, args: Value) -> Result<ToolOutput> {
        self.as_ref().call_tool_safe(name, args).await
    }

    async fn prepare(&self, context: &ToolsetContext) -> Result<()> {
        self.as_ref().prepare(context).await
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn registry_as_toolset_lists_tools() {
        let registry = ToolRegistry::new();
        let tools = Toolset::list_tools(&registry);
        assert!(tools.is_empty());
    }

    #[tokio::test]
    async fn registry_as_toolset_call_unknown_fails() {
        let registry = ToolRegistry::new();
        let result = Toolset::call_tool(&registry, "missing_tool", json!({})).await;
        assert!(result.is_err());
    }
}
