//! Composable tool wrappers (decorators) for policy enforcement.
//!
//! Core wrappers (ToolWrapper, WrappedTool, TimeoutWrapper, RateLimitWrapper)
//! are defined in restflow-traits and re-exported here.
//! LoggingWrapper stays here because it depends on Scratchpad (agent runtime).

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use serde_json::{Value, json};

// Re-export core wrappers from restflow-traits
pub use restflow_traits::wrapper::{
    RateLimitWrapper, TimeoutWrapper, ToolWrapper, WrappedTool,
};

use restflow_traits::error::Result;
use restflow_traits::tool::{Tool, ToolOutput};

use crate::agent::Scratchpad;

/// Wrapper that logs tool execution and outcome to scratchpad.
pub struct LoggingWrapper {
    scratchpad: Arc<Scratchpad>,
    iteration: usize,
}

impl LoggingWrapper {
    pub fn new(scratchpad: Arc<Scratchpad>, iteration: usize) -> Self {
        Self {
            scratchpad,
            iteration,
        }
    }
}

#[async_trait]
impl ToolWrapper for LoggingWrapper {
    fn wrapper_name(&self) -> &str {
        "logging"
    }

    async fn wrap_execute(
        &self,
        tool_name: &str,
        input: Value,
        next: &dyn Tool,
    ) -> Result<ToolOutput> {
        self.scratchpad.append(
            self.iteration,
            "tool_wrapper_start",
            json!({
                "tool": tool_name,
                "wrapper": self.wrapper_name(),
                "input": input,
            }),
        );

        let start = Instant::now();
        let result = next.execute(input).await;
        let duration_ms = start.elapsed().as_millis();

        match &result {
            Ok(output) => self.scratchpad.append(
                self.iteration,
                "tool_wrapper_result",
                json!({
                    "tool": tool_name,
                    "wrapper": self.wrapper_name(),
                    "success": output.success,
                    "duration_ms": duration_ms,
                }),
            ),
            Err(error) => self.scratchpad.append(
                self.iteration,
                "tool_wrapper_result",
                json!({
                    "tool": tool_name,
                    "wrapper": self.wrapper_name(),
                    "success": false,
                    "duration_ms": duration_ms,
                    "error": error.to_string(),
                }),
            ),
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use super::*;
    use restflow_traits::tool::Tool;
    use restflow_traits::error::Result;

    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }

        fn description(&self) -> &str {
            "Echo input"
        }

        fn parameters_schema(&self) -> Value {
            json!({"type":"object"})
        }

        async fn execute(&self, input: Value) -> Result<ToolOutput> {
            Ok(ToolOutput::success(input))
        }
    }

    #[tokio::test]
    async fn logging_wrapper_appends_to_scratchpad() {
        let dir = tempfile::tempdir().expect("temp dir should be created");
        let path = dir.path().join("tool-wrapper.jsonl");
        let scratchpad = Arc::new(Scratchpad::new(path.clone()).expect("scratchpad should create"));
        let wrapped = WrappedTool::new(
            Arc::new(EchoTool),
            vec![Arc::new(LoggingWrapper::new(scratchpad, 7))],
        );

        let output = wrapped
            .execute(json!({"hello":"world"}))
            .await
            .expect("wrapped execution should succeed");
        assert!(output.success);

        let content = std::fs::read_to_string(path).expect("scratchpad should be readable");
        assert!(content.contains("tool_wrapper_start"));
        assert!(content.contains("tool_wrapper_result"));
        assert!(content.contains("\"tool\":\"echo\""));
    }
}
