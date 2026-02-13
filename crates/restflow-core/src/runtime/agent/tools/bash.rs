//! Bash execution tool with security constraints.

use crate::runtime::agent::tools::ToolResult;
use async_trait::async_trait;
use restflow_ai::error::{AiError, Result};
use restflow_ai::tools::Tool;
use serde_json::{Value, json};
use std::process::Stdio;
use tokio::process::Command;

/// Configuration for bash tool security.
#[derive(Debug, Clone)]
pub struct BashConfig {
    /// Working directory for commands.
    pub working_dir: Option<String>,

    /// Command timeout in seconds.
    pub timeout_secs: u64,

    /// Blocked commands (security).
    pub blocked_commands: Vec<String>,

    /// Whether to allow sudo.
    pub allow_sudo: bool,
    /// Maximum total bytes for stdout/stderr output payload.
    pub max_output_bytes: usize,
}

impl Default for BashConfig {
    fn default() -> Self {
        Self {
            working_dir: None,
            timeout_secs: 30,
            blocked_commands: vec![
                "rm -rf /".to_string(),
                "mkfs".to_string(),
                "dd if=/dev".to_string(),
            ],
            allow_sudo: false,
            max_output_bytes: 1_000_000,
        }
    }
}

pub struct BashTool {
    config: BashConfig,
}

impl BashTool {
    pub fn new(config: BashConfig) -> Self {
        Self { config }
    }

    fn is_command_blocked(&self, command: &str) -> bool {
        for blocked in &self.config.blocked_commands {
            if command.contains(blocked) {
                return true;
            }
        }

        if !self.config.allow_sudo && command.trim_start().starts_with("sudo") {
            return true;
        }

        false
    }

    fn truncate_to_limit(&self, value: &str) -> String {
        if value.len() <= self.config.max_output_bytes {
            return value.to_string();
        }
        let mut end = self.config.max_output_bytes;
        while end > 0 && !value.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}\n[Output truncated]", &value[..end])
    }
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Execute a bash command and return the output."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The bash command to execute"
                }
            },
            "required": ["command"]
        })
    }

    fn supports_parallel(&self) -> bool {
        false
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let command = args
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AiError::Tool("Missing 'command' argument".to_string()))?;

        if self.is_command_blocked(command) {
            return Ok(ToolResult::error("Command blocked for security reasons"));
        }

        let mut cmd = Command::new("bash");
        cmd.arg("-c").arg(command);

        if let Some(ref cwd) = self.config.working_dir {
            cmd.current_dir(cwd);
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = tokio::time::timeout(
            tokio::time::Duration::from_secs(self.config.timeout_secs),
            cmd.output(),
        )
        .await
        .map_err(|_| {
            AiError::Tool(format!(
                "Command timed out after {}s",
                self.config.timeout_secs
            ))
        })?
        .map_err(|e| AiError::Tool(format!("Failed to execute command: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = self.truncate_to_limit(&stdout);
        let stderr = self.truncate_to_limit(&stderr);

        if output.status.success() {
            Ok(ToolResult::success(json!(stdout)))
        } else {
            Ok(ToolResult {
                success: false,
                result: json!(stdout),
                error: Some(stderr),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_to_limit_when_output_exceeds_limit() {
        let tool = BashTool::new(BashConfig {
            max_output_bytes: 5,
            ..BashConfig::default()
        });
        let truncated = tool.truncate_to_limit("123456789");
        assert!(truncated.starts_with("12345"));
        assert!(truncated.contains("[Output truncated]"));
    }

    #[test]
    fn test_truncate_to_limit_keeps_short_output() {
        let tool = BashTool::new(BashConfig {
            max_output_bytes: 50,
            ..BashConfig::default()
        });
        assert_eq!(tool.truncate_to_limit("short"), "short");
    }
}
