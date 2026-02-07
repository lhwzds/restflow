//! Python code execution tool

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;

use crate::ToolAction;
use crate::error::Result;
use crate::security::SecurityGate;
use crate::tools::traits::check_security;
use crate::tools::traits::{Tool, ToolOutput};

#[derive(Debug, Deserialize)]
struct PythonInput {
    code: String,
    timeout_seconds: Option<u64>,
}

/// Python code execution tool
pub struct PythonTool {
    python_path: String,
    security_gate: Option<Arc<dyn SecurityGate>>,
    agent_id: Option<String>,
    task_id: Option<String>,
}

impl Default for PythonTool {
    fn default() -> Self {
        Self::new()
    }
}

impl PythonTool {
    /// Create a new Python tool with default python3 path
    pub fn new() -> Self {
        Self {
            python_path: "python3".to_string(),
            security_gate: None,
            agent_id: None,
            task_id: None,
        }
    }

    /// Create with a custom Python interpreter path
    pub fn with_python_path(python_path: impl Into<String>) -> Self {
        Self {
            python_path: python_path.into(),
            security_gate: None,
            agent_id: None,
            task_id: None,
        }
    }

    pub fn with_security(
        mut self,
        security_gate: Arc<dyn SecurityGate>,
        agent_id: impl Into<String>,
        task_id: impl Into<String>,
    ) -> Self {
        self.security_gate = Some(security_gate);
        self.agent_id = Some(agent_id.into());
        self.task_id = Some(task_id.into());
        self
    }
}

#[async_trait]
impl Tool for PythonTool {
    fn name(&self) -> &str {
        "run_python"
    }

    fn description(&self) -> &str {
        "Execute inline Python code in a subprocess and return stdout, stderr, and exit code."
    }

    fn parameters_schema(&self) -> Value {
        json!({
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
        })
    }

    fn supports_parallel(&self) -> bool {
        false
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: PythonInput = serde_json::from_value(input)?;
        let timeout = params.timeout_seconds.unwrap_or(30);

        let action = ToolAction {
            tool_name: "python".to_string(),
            operation: "execute".to_string(),
            target: "<inline>".to_string(),
            summary: "Execute python code".to_string(),
        };

        if let Some(message) = check_security(
            self.security_gate.as_deref(),
            action,
            self.agent_id.as_deref(),
            self.task_id.as_deref(),
        )
        .await?
        {
            return Ok(ToolOutput::error(message));
        }

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(timeout),
            Command::new(&self.python_path)
                .arg("-c")
                .arg(&params.code)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
        )
        .await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                if output.status.success() {
                    Ok(ToolOutput::success(json!({
                        "stdout": stdout.trim(),
                        "stderr": stderr.trim(),
                        "exit_code": 0
                    })))
                } else {
                    Ok(ToolOutput::error(format!(
                        "Python error (exit code {}): {}",
                        output.status.code().unwrap_or(-1),
                        stderr.trim()
                    )))
                }
            }
            Ok(Err(e)) => Ok(ToolOutput::error(format!(
                "Failed to execute Python: {}",
                e
            ))),
            Err(_) => Ok(ToolOutput::error(format!(
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
        assert_eq!(tool.name(), "run_python");
        assert!(!tool.description().is_empty());

        let schema = tool.parameters_schema();
        assert!(schema.get("properties").is_some());
    }

    #[tokio::test]
    async fn test_python_tool_simple() {
        let tool = PythonTool::new();
        let input = json!({
            "code": "print('hello world')"
        });

        let result = tool.execute(input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.result["stdout"], "hello world");
    }
}
