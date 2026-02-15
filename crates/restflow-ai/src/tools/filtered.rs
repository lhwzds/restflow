//! Filtered toolset wrappers.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::error::{AiError, Result};
use crate::tools::{ToolOutput, ToolSchema, Toolset};

type ToolPredicate = Arc<dyn Fn(&ToolSchema) -> bool + Send + Sync>;

/// Toolset wrapper that filters visible/callable tools by predicate.
pub struct FilteredToolset<T> {
    inner: T,
    predicate: ToolPredicate,
}

impl<T> FilteredToolset<T> {
    pub fn new(inner: T, predicate: ToolPredicate) -> Self {
        Self { inner, predicate }
    }
}

impl<T: Toolset> FilteredToolset<T> {
    /// Keep only tools listed in `allowed_tools`.
    pub fn from_allowlist(inner: T, allowed_tools: &[String]) -> Self {
        let allowed: HashSet<String> = allowed_tools
            .iter()
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
            .collect();

        let predicate = Arc::new(move |tool: &ToolSchema| {
            if allowed.is_empty() {
                return true;
            }
            allowed.contains(&tool.name)
        });

        Self::new(inner, predicate)
    }
}

#[async_trait]
impl<T: Toolset> Toolset for FilteredToolset<T> {
    fn list_tools(&self) -> Vec<ToolSchema> {
        self.inner
            .list_tools()
            .into_iter()
            .filter(|tool| (self.predicate)(tool))
            .collect()
    }

    async fn call_tool(&self, name: &str, args: Value) -> Result<ToolOutput> {
        if !self
            .list_tools()
            .iter()
            .any(|tool| tool.name.as_str() == name)
        {
            return Err(AiError::ToolNotFound(name.to_string()));
        }
        self.inner.call_tool(name, args).await
    }

    async fn call_tool_safe(&self, name: &str, args: Value) -> Result<ToolOutput> {
        if !self
            .list_tools()
            .iter()
            .any(|tool| tool.name.as_str() == name)
        {
            return Err(AiError::ToolNotFound(name.to_string()));
        }
        self.inner.call_tool_safe(name, args).await
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::tools::{Tool, ToolRegistry};

    struct EchoTool;
    struct ReverseTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }

        fn description(&self) -> &str {
            "Echo input"
        }

        fn parameters_schema(&self) -> Value {
            json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string" }
                }
            })
        }

        async fn execute(&self, input: Value) -> crate::error::Result<ToolOutput> {
            Ok(ToolOutput::success(input))
        }
    }

    #[async_trait]
    impl Tool for ReverseTool {
        fn name(&self) -> &str {
            "reverse"
        }

        fn description(&self) -> &str {
            "Reverse input"
        }

        fn parameters_schema(&self) -> Value {
            json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string" }
                }
            })
        }

        async fn execute(&self, input: Value) -> crate::error::Result<ToolOutput> {
            Ok(ToolOutput::success(input))
        }
    }

    #[test]
    fn allowlist_filters_tool_schemas() {
        let mut registry = ToolRegistry::new();
        registry.register(EchoTool);
        registry.register(ReverseTool);

        let toolset = FilteredToolset::from_allowlist(registry, &["echo".to_string()]);
        let names: Vec<String> = toolset
            .list_tools()
            .into_iter()
            .map(|schema| schema.name)
            .collect();

        assert_eq!(names, vec!["echo".to_string()]);
    }

    #[tokio::test]
    async fn blocked_tool_call_returns_not_found() {
        let mut registry = ToolRegistry::new();
        registry.register(EchoTool);
        registry.register(ReverseTool);

        let toolset = FilteredToolset::from_allowlist(registry, &["echo".to_string()]);
        let err = toolset
            .call_tool("reverse", json!({"text":"hello"}))
            .await
            .unwrap_err();

        assert!(matches!(err, AiError::ToolNotFound(name) if name == "reverse"));
    }
}
