//! LoggingWrapper — logs tool execution to a JSONL file.
//!
//! Core wrappers (ToolWrapper, WrappedTool, TimeoutWrapper, RateLimitWrapper)
//! live in restflow-traits and are re-exported via tools/mod.rs.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{Value, json};

use restflow_traits::error::Result;
use restflow_traits::tool::{Tool, ToolOutput};
use restflow_traits::wrapper::ToolWrapper;

/// Wrapper that logs tool execution and outcome to a JSONL file.
pub struct LoggingWrapper {
    log_path: PathBuf,
    iteration: usize,
}

impl LoggingWrapper {
    pub fn new(log_path: PathBuf, iteration: usize) -> Self {
        Self {
            log_path,
            iteration,
        }
    }

    fn append(&self, event_type: &'static str, data: Value) {
        if let Some(parent) = self.log_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let entry = json!({
            "timestamp": Utc::now().to_rfc3339(),
            "iteration": self.iteration,
            "event_type": event_type,
            "data": data,
        });
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
            && let Ok(line) = serde_json::to_string(&entry)
        {
            let _ = writeln!(file, "{line}");
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
        self.append(
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
            Ok(output) => self.append(
                "tool_wrapper_result",
                json!({
                    "tool": tool_name,
                    "wrapper": self.wrapper_name(),
                    "success": output.success,
                    "duration_ms": duration_ms,
                }),
            ),
            Err(error) => self.append(
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
    use restflow_traits::error::Result;
    use restflow_traits::tool::Tool;
    use restflow_traits::wrapper::WrappedTool;

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
    async fn logging_wrapper_appends_to_trace_file() {
        let dir = tempfile::tempdir().expect("temp dir should be created");
        let path = dir.path().join("tool-wrapper.jsonl");
        let wrapped = WrappedTool::new(
            Arc::new(EchoTool),
            vec![Arc::new(LoggingWrapper::new(path.clone(), 7))],
        );

        let output = wrapped
            .execute(json!({"hello":"world"}))
            .await
            .expect("wrapped execution should succeed");
        assert!(output.success);

        let content = std::fs::read_to_string(path).expect("trace file should be readable");
        assert!(content.contains("tool_wrapper_start"));
        assert!(content.contains("tool_wrapper_result"));
        assert!(content.contains("\"tool\":\"echo\""));
    }
}
