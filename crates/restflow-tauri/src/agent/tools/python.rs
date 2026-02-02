//! Python execution tool.

use super::{Tool, ToolDefinition, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::process::Stdio;
use tokio::process::Command;

#[derive(Debug, Deserialize)]
struct PythonInput {
    code: String,
    timeout_seconds: Option<u64>,
}

pub struct PythonTool {
    python_path: String,
}

impl PythonTool {
    pub fn new() -> Self {
        Self {
            python_path: "python3".to_string(),
        }
    }

    pub fn with_python_path(python_path: impl Into<String>) -> Self {
        Self {
            python_path: python_path.into(),
        }
    }
}

impl Default for PythonTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for PythonTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "run_python".to_string(),
            description: "Execute Python code and return the output. Use print() to output results."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "code": {
                        "type": "string",
                        "description": "Python code to execute"
                    },
                    "timeout_seconds": {
                        "type": "integer",
                        "description": "Execution timeout in seconds (default: 30)"
                    }
                },
                "required": ["code"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let params: PythonInput = serde_json::from_value(args)?;
        let timeout = params.timeout_seconds.unwrap_or(30);

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout),
            Command::new(&self.python_path)
                .arg("-c")
                .arg(&params.code)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
        )
        .await;

        match output {
            Ok(Ok(result)) => {
                let stdout = String::from_utf8_lossy(&result.stdout).to_string();
                let stderr = String::from_utf8_lossy(&result.stderr).to_string();
                if result.status.success() {
                    let payload = json!({
                        "stdout": stdout.trim(),
                        "stderr": stderr.trim(),
                        "exit_code": 0
                    });
                    Ok(ToolResult::success(payload.to_string()))
                } else {
                    Ok(ToolResult::error(format!(
                        "Python error (exit code {}): {}",
                        result.status.code().unwrap_or(-1),
                        stderr.trim()
                    )))
                }
            }
            Ok(Err(e)) => Ok(ToolResult::error(format!(
                "Failed to execute Python: {}",
                e
            ))),
            Err(_) => Ok(ToolResult::error(format!(
                "Python execution timed out after {}s",
                timeout
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_python_tool_schema() {
        let tool = PythonTool::new();
        assert_eq!(tool.definition().name, "run_python");
        assert!(!tool.definition().description.is_empty());
    }
}
