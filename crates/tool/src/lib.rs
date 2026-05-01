//! # codocia
//!
//! Tool owns the callable tool contract and registry.
//!
//! ## Owns
//! - Tool trait
//! - Registry
//! - tool specs
//! - tool lookup
//! - tool call boundary
//!
//! ## Must Not
//! - decide per-turn permissions alone
//! - own agent loop state
//! - write durable run state
//!
//! ## Inputs
//! - JSON tool input
//! - tool call request
//! - registered tool implementations
//!
//! ## Outputs
//! - JSON tool output
//! - tool call result
//! - tool registry names
//!
//! ## Used By
//! - agent
//!
//! ## Verify
//! - cargo check -p tool

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn spec(&self) -> ToolSpec {
        ToolSpec::new(self.name())
    }
    async fn call(&self, input: Value) -> Result<Value>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
}

impl ToolSpec {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            input_schema: serde_json::json!({ "type": "object" }),
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_input_schema(mut self, input_schema: Value) -> Self {
        self.input_schema = input_schema;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
}

impl ToolCall {
    pub fn new(id: impl Into<String>, name: impl Into<String>, input: Value) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            input,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolOutput {
    pub call_id: String,
    pub value: Value,
}

#[derive(Default, Clone)]
pub struct Registry {
    tools: BTreeMap<String, Arc<dyn Tool>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert<T>(&mut self, tool: T)
    where
        T: Tool + 'static,
    {
        self.tools.insert(tool.name().to_string(), Arc::new(tool));
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    pub fn specs(&self) -> Vec<ToolSpec> {
        self.tools.values().map(|tool| tool.spec()).collect()
    }

    pub async fn call(&self, call: ToolCall) -> Result<ToolOutput> {
        let tool = self
            .get(&call.name)
            .ok_or_else(|| anyhow::anyhow!("tool not found: {}", call.name))?;
        let value = tool.call(call.input).await?;
        Ok(ToolOutput {
            call_id: call.id,
            value,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }

        fn spec(&self) -> ToolSpec {
            ToolSpec::new("echo").with_description("Return the input unchanged.")
        }

        async fn call(&self, input: Value) -> Result<Value> {
            Ok(input)
        }
    }

    #[test]
    fn registry_exposes_specs_and_calls_tools() {
        block_on_once(async {
            let mut registry = Registry::new();
            registry.insert(EchoTool);

            assert_eq!(registry.names(), vec!["echo"]);
            assert_eq!(
                registry.specs()[0].description.as_deref(),
                Some("Return the input unchanged.")
            );

            let output = registry
                .call(ToolCall::new(
                    "call-1",
                    "echo",
                    serde_json::json!({ "message": "hello" }),
                ))
                .await
                .unwrap();

            assert_eq!(output.call_id, "call-1");
            assert_eq!(output.value, serde_json::json!({ "message": "hello" }));
        });
    }

    #[test]
    fn registry_reports_missing_tool() {
        block_on_once(async {
            let registry = Registry::new();

            let error = registry
                .call(ToolCall::new("call-1", "missing", serde_json::json!({})))
                .await
                .unwrap_err();

            assert!(error.to_string().contains("tool not found: missing"));
        });
    }

    fn block_on_once<T>(future: impl Future<Output = T>) -> T {
        let waker = Waker::from(Arc::new(NoopWake));
        let mut context = Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);

        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("tool future unexpectedly yielded"),
        }
    }

    struct NoopWake;

    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }
}
