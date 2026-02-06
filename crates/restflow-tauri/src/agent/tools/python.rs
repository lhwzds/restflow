//! Python execution tool for simple scripts.

use crate::agent::tools::ToolResult;
use async_trait::async_trait;
use restflow_ai::error::{AiError, Result};
use restflow_ai::tools::Tool;
use serde_json::{Value, json};
use std::process::Stdio;
use tokio::process::Command;

pub struct PythonTool;

impl Default for PythonTool {
    fn default() -> Self {
        Self
    }
}

impl PythonTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for PythonTool {
    fn name(&self) -> &str {
        "python"
    }

    fn description(&self) -> &str {
        "Execute a Python script and return the output."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "code": {
                    "type": "string",
                    "description": "Python code to execute"
                }
            },
            "required": ["code"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let code = args
            .get("code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AiError::Tool("Missing 'code' argument".to_string()))?;

        let mut cmd = Command::new("python3");
        cmd.arg("-");
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| AiError::Tool(format!("Failed to start python: {}", e)))?;

        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin
                .write_all(code.as_bytes())
                .await
                .map_err(|e| AiError::Tool(format!("Failed to write python input: {}", e)))?;
        }

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| AiError::Tool(format!("Failed to execute python: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(ToolResult::success(json!(stdout.to_string())))
        } else {
            Ok(ToolResult::error(format!("Python error: {}", stderr)))
        }
    }
}
