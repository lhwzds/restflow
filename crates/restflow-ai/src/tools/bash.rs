//! Bash command execution tool for AI agents
//!
//! Provides shell command execution with:
//! - Configurable timeout (default 120s)
//! - Output truncation for large outputs
//! - Working directory support
//! - Security check integration (optional)
//!
//! # Example
//!
//! ```ignore
//! let tool = BashTool::new();
//! let output = tool.execute(serde_json::json!({
//!     "command": "ls -la",
//!     "workdir": "/tmp"
//! })).await?;
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Instant;
use tokio::process::Command;
use tokio::time::{Duration, timeout};

use super::traits::{Tool, ToolOutput};
use crate::error::Result;
use crate::security::SecurityGate;

/// Default timeout for command execution in seconds
const DEFAULT_TIMEOUT_SECS: u64 = 120;

/// Maximum output size in bytes (100KB)
const DEFAULT_MAX_OUTPUT_BYTES: usize = 100_000;

/// Bash command execution tool
#[derive(Clone)]
pub struct BashTool {
    /// Default working directory for commands
    default_workdir: Option<String>,
    /// Command timeout in seconds
    timeout_secs: u64,
    /// Maximum output size in bytes
    max_output_bytes: usize,
    /// Optional security gate
    security_gate: Option<Arc<dyn SecurityGate>>,
    /// Agent identifier for security checks
    agent_id: Option<String>,
    /// Task identifier for security checks
    task_id: Option<String>,
}

impl Default for BashTool {
    fn default() -> Self {
        Self::new()
    }
}

impl BashTool {
    /// Create a new BashTool with default settings
    pub fn new() -> Self {
        Self {
            default_workdir: None,
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
            security_gate: None,
            agent_id: None,
            task_id: None,
        }
    }

    /// Set default working directory for commands
    pub fn with_workdir(mut self, workdir: impl Into<String>) -> Self {
        self.default_workdir = Some(workdir.into());
        self
    }

    /// Set command timeout in seconds
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Set maximum output size in bytes
    pub fn with_max_output(mut self, bytes: usize) -> Self {
        self.max_output_bytes = bytes;
        self
    }

    /// Attach a security gate for command approval
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

    /// Run a shell command and capture output
    async fn run_command(
        &self,
        command: &str,
        workdir: &str,
    ) -> std::result::Result<(i32, String, String, bool), std::io::Error> {
        let output = Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(workdir)
            .kill_on_drop(true)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        let exit_code = output.status.code().unwrap_or(-1);

        let (stdout, stdout_truncated) = self.truncate_output(&output.stdout);
        let (stderr, stderr_truncated) = self.truncate_output(&output.stderr);

        Ok((
            exit_code,
            stdout,
            stderr,
            stdout_truncated || stderr_truncated,
        ))
    }

    /// Truncate output if it exceeds max size
    fn truncate_output(&self, bytes: &[u8]) -> (String, bool) {
        let truncated = bytes.len() > self.max_output_bytes;
        let bytes = if truncated {
            &bytes[..self.max_output_bytes]
        } else {
            bytes
        };

        let text = String::from_utf8_lossy(bytes).to_string();
        if truncated {
            (
                format!(
                    "{}...\n[Output truncated, {} bytes total]",
                    text,
                    bytes.len()
                ),
                true,
            )
        } else {
            (text, false)
        }
    }
}

/// Input parameters for bash command execution
#[derive(Debug, Deserialize)]
pub struct BashInput {
    /// Command to execute
    pub command: String,
    /// Working directory (optional)
    #[serde(default)]
    pub workdir: Option<String>,
    /// Timeout in seconds (optional, default: 120)
    #[serde(default)]
    pub timeout: Option<u64>,
}

/// Output from bash command execution
#[derive(Debug, Serialize, Deserialize)]
pub struct BashOutput {
    /// Exit code (0 = success)
    pub exit_code: i32,
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
    /// Whether output was truncated
    pub truncated: bool,
    /// Execution time in milliseconds
    pub duration_ms: u64,
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Run shell commands in the local environment and return stdout, stderr, and exit status."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "workdir": {
                    "type": "string",
                    "description": "Working directory for command execution"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 120)"
                }
            },
            "required": ["command"]
        })
    }

    fn supports_parallel(&self) -> bool {
        false
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let input: BashInput = serde_json::from_value(input)?;

        // Determine working directory
        let workdir = input
            .workdir
            .or_else(|| self.default_workdir.clone())
            .unwrap_or_else(|| ".".to_string());

        // Determine timeout
        let timeout_secs = input.timeout.unwrap_or(self.timeout_secs);

        if let Some(security_gate) = &self.security_gate {
            let agent_id = self
                .agent_id
                .as_deref()
                .ok_or_else(|| crate::error::AiError::Tool("Missing agent_id".into()))?;
            let task_id = self
                .task_id
                .as_deref()
                .ok_or_else(|| crate::error::AiError::Tool("Missing task_id".into()))?;

            let decision = security_gate
                .check_command(&input.command, task_id, agent_id, Some(&workdir))
                .await?;

            if !decision.allowed {
                if decision.requires_approval {
                    return Ok(ToolOutput {
                        success: false,
                        result: serde_json::json!({
                            "pending_approval": true,
                            "approval_id": decision.approval_id,
                        }),
                        error: decision.reason,
                    });
                }

                return Ok(ToolOutput {
                    success: false,
                    result: serde_json::json!({
                        "blocked": true,
                    }),
                    error: decision.reason,
                });
            }
        }

        let start = Instant::now();

        // Execute with timeout
        let result = timeout(
            Duration::from_secs(timeout_secs),
            self.run_command(&input.command, &workdir),
        )
        .await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(Ok((exit_code, stdout, stderr, truncated))) => {
                let output = BashOutput {
                    exit_code,
                    stdout,
                    stderr,
                    truncated,
                    duration_ms,
                };

                Ok(ToolOutput {
                    success: exit_code == 0,
                    result: serde_json::to_value(&output)?,
                    error: if exit_code != 0 {
                        Some(format!("Command exited with code {}", exit_code))
                    } else {
                        None
                    },
                })
            }
            Ok(Err(e)) => Ok(ToolOutput {
                success: false,
                result: serde_json::json!({"error": e.to_string()}),
                error: Some(e.to_string()),
            }),
            Err(_) => Ok(ToolOutput {
                success: false,
                result: serde_json::json!({
                    "error": "Command timed out",
                    "timeout_secs": timeout_secs,
                }),
                error: Some(format!("Timeout after {} seconds", timeout_secs)),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bash_tool_new() {
        let tool = BashTool::new();
        assert_eq!(tool.timeout_secs, DEFAULT_TIMEOUT_SECS);
        assert_eq!(tool.max_output_bytes, DEFAULT_MAX_OUTPUT_BYTES);
        assert!(tool.default_workdir.is_none());
    }

    #[test]
    fn test_bash_tool_with_workdir() {
        let tool = BashTool::new().with_workdir("/tmp");
        assert_eq!(tool.default_workdir, Some("/tmp".to_string()));
    }

    #[test]
    fn test_bash_tool_with_timeout() {
        let tool = BashTool::new().with_timeout(60);
        assert_eq!(tool.timeout_secs, 60);
    }

    #[test]
    fn test_bash_tool_with_max_output() {
        let tool = BashTool::new().with_max_output(50_000);
        assert_eq!(tool.max_output_bytes, 50_000);
    }

    #[test]
    fn test_bash_tool_name() {
        let tool = BashTool::new();
        assert_eq!(tool.name(), "bash");
    }

    #[test]
    fn test_bash_tool_description() {
        let tool = BashTool::new();
        assert!(tool.description().contains("shell commands"));
    }

    #[test]
    fn test_bash_tool_schema() {
        let tool = BashTool::new();
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["command"].is_object());
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&Value::String("command".to_string()))
        );
    }

    #[test]
    fn test_truncate_output_no_truncation() {
        let tool = BashTool::new();
        let data = b"hello world";
        let (text, truncated) = tool.truncate_output(data);
        assert_eq!(text, "hello world");
        assert!(!truncated);
    }

    #[test]
    fn test_truncate_output_with_truncation() {
        let tool = BashTool::new().with_max_output(10);
        let data = b"hello world this is a long string";
        let (text, truncated) = tool.truncate_output(data);
        assert!(truncated);
        assert!(text.contains("[Output truncated"));
    }

    #[test]
    fn test_bash_input_deserialization() {
        let input: BashInput = serde_json::from_value(serde_json::json!({
            "command": "ls -la"
        }))
        .unwrap();
        assert_eq!(input.command, "ls -la");
        assert!(input.workdir.is_none());
        assert!(input.timeout.is_none());
    }

    #[test]
    fn test_bash_input_full_deserialization() {
        let input: BashInput = serde_json::from_value(serde_json::json!({
            "command": "ls -la",
            "workdir": "/tmp",
            "timeout": 60
        }))
        .unwrap();
        assert_eq!(input.command, "ls -la");
        assert_eq!(input.workdir, Some("/tmp".to_string()));
        assert_eq!(input.timeout, Some(60));
    }

    #[tokio::test]
    #[cfg(unix)] // BashTool uses 'sh -c' which is Unix-specific
    async fn test_bash_tool_execute_simple() {
        let tool = BashTool::new();
        let output = tool
            .execute(serde_json::json!({
                "command": "echo hello"
            }))
            .await
            .unwrap();

        assert!(output.success);
        let result: BashOutput = serde_json::from_value(output.result).unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("hello"));
        assert!(!result.truncated);
    }

    #[tokio::test]
    #[cfg(unix)] // BashTool uses 'sh -c' which is Unix-specific
    async fn test_bash_tool_execute_with_workdir() {
        let tool = BashTool::new();
        let output = tool
            .execute(serde_json::json!({
                "command": "pwd",
                "workdir": "/tmp"
            }))
            .await
            .unwrap();

        assert!(output.success);
        let result: BashOutput = serde_json::from_value(output.result).unwrap();
        assert!(result.stdout.contains("/tmp") || result.stdout.contains("/private/tmp"));
    }

    #[tokio::test]
    #[cfg(unix)] // BashTool uses 'sh -c' which is Unix-specific
    async fn test_bash_tool_execute_nonzero_exit() {
        let tool = BashTool::new();
        let output = tool
            .execute(serde_json::json!({
                "command": "exit 1"
            }))
            .await
            .unwrap();

        assert!(!output.success);
        let result: BashOutput = serde_json::from_value(output.result).unwrap();
        assert_eq!(result.exit_code, 1);
        assert!(output.error.is_some());
    }

    #[tokio::test]
    #[cfg(unix)] // BashTool uses 'sh -c' which is Unix-specific
    async fn test_bash_tool_execute_with_stderr() {
        let tool = BashTool::new();
        let output = tool
            .execute(serde_json::json!({
                "command": "echo error >&2"
            }))
            .await
            .unwrap();

        assert!(output.success); // Exit code is still 0
        let result: BashOutput = serde_json::from_value(output.result).unwrap();
        assert!(result.stderr.contains("error"));
    }

    #[tokio::test]
    #[cfg(unix)] // BashTool uses 'sh -c' which is Unix-specific
    async fn test_bash_tool_execute_timeout() {
        let tool = BashTool::new().with_timeout(1);
        let output = tool
            .execute(serde_json::json!({
                "command": "sleep 10"
            }))
            .await
            .unwrap();

        assert!(!output.success);
        assert!(output.error.as_ref().unwrap().contains("Timeout"));
    }

    #[tokio::test]
    #[cfg(unix)] // BashTool uses 'sh -c' which is Unix-specific
    async fn test_bash_tool_execute_with_duration() {
        let tool = BashTool::new();
        let output = tool
            .execute(serde_json::json!({
                "command": "sleep 0.1 && echo done"
            }))
            .await
            .unwrap();

        assert!(output.success);
        let result: BashOutput = serde_json::from_value(output.result).unwrap();
        assert!(result.duration_ms >= 100);
    }

    #[tokio::test]
    #[cfg(unix)] // BashTool uses 'sh -c' which is Unix-specific
    async fn test_bash_tool_execute_invalid_command() {
        let tool = BashTool::new();
        let output = tool
            .execute(serde_json::json!({
                "command": "nonexistent_command_12345"
            }))
            .await
            .unwrap();

        assert!(!output.success);
        let result: BashOutput = serde_json::from_value(output.result).unwrap();
        assert_ne!(result.exit_code, 0);
    }

    #[tokio::test]
    #[cfg(unix)] // BashTool uses 'sh -c' which is Unix-specific
    async fn test_bash_tool_default_workdir() {
        let tool = BashTool::new().with_workdir("/tmp");
        let output = tool
            .execute(serde_json::json!({
                "command": "pwd"
            }))
            .await
            .unwrap();

        assert!(output.success);
        let result: BashOutput = serde_json::from_value(output.result).unwrap();
        assert!(result.stdout.contains("/tmp") || result.stdout.contains("/private/tmp"));
    }

    #[tokio::test]
    #[cfg(unix)] // BashTool uses 'sh -c' which is Unix-specific
    async fn test_bash_tool_input_timeout_override() {
        let tool = BashTool::new().with_timeout(60);
        let output = tool
            .execute(serde_json::json!({
                "command": "sleep 10",
                "timeout": 1
            }))
            .await
            .unwrap();

        assert!(!output.success);
        assert!(output.error.as_ref().unwrap().contains("Timeout"));
    }

    #[test]
    fn test_bash_output_serialization() {
        let output = BashOutput {
            exit_code: 0,
            stdout: "hello".to_string(),
            stderr: String::new(),
            truncated: false,
            duration_ms: 100,
        };

        let json = serde_json::to_value(&output).unwrap();
        assert_eq!(json["exit_code"], 0);
        assert_eq!(json["stdout"], "hello");
        assert_eq!(json["truncated"], false);
        assert_eq!(json["duration_ms"], 100);
    }
}
