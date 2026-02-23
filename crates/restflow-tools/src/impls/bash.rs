//! Bash command execution tool for AI agents.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Instant;
use tokio::process::Command;
use tokio::time::{Duration, sleep, timeout};

#[cfg(unix)]
use nix::sys::signal::{Signal, killpg};
#[cfg(unix)]
use nix::unistd::Pid;

use crate::Result;
use crate::security::SecurityGate;
use crate::{Tool, ToolErrorCategory, ToolOutput};

/// Default timeout for command execution in seconds.
const DEFAULT_TIMEOUT_SECS: u64 = 300;

/// Maximum output size in bytes (100KB).
const DEFAULT_MAX_OUTPUT_BYTES: usize = 100_000;

/// Bash command execution tool.
#[derive(Clone)]
pub struct BashTool {
    default_workdir: Option<String>,
    timeout_secs: u64,
    max_output_bytes: usize,
    security_gate: Option<Arc<dyn SecurityGate>>,
    agent_id: Option<String>,
    task_id: Option<String>,
    #[cfg(feature = "sandbox")]
    sandbox_policy: Option<restflow_sandbox::SandboxPolicy>,
}

impl Default for BashTool {
    fn default() -> Self {
        Self::new()
    }
}

impl BashTool {
    pub fn new() -> Self {
        Self {
            default_workdir: None,
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
            security_gate: None,
            agent_id: None,
            task_id: None,
            #[cfg(feature = "sandbox")]
            sandbox_policy: None,
        }
    }

    pub fn with_workdir(mut self, workdir: impl Into<String>) -> Self {
        self.default_workdir = Some(workdir.into());
        self
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    pub fn with_max_output(mut self, bytes: usize) -> Self {
        self.max_output_bytes = bytes;
        self
    }

    #[cfg(feature = "sandbox")]
    pub fn with_sandbox_policy(mut self, policy: restflow_sandbox::SandboxPolicy) -> Self {
        self.sandbox_policy = Some(policy);
        self
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

    async fn run_command(
        &self,
        command: &str,
        workdir: &str,
        timeout_secs: u64,
    ) -> std::result::Result<(i32, String, String, bool), std::io::Error> {
        #[cfg(feature = "sandbox")]
        let (program, args) = if let Some(ref policy) = self.sandbox_policy {
            restflow_sandbox::wrap_command(policy, "sh", &["-c", command])
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?
        } else {
            ("sh".to_string(), vec!["-c".to_string(), command.to_string()])
        };
        #[cfg(not(feature = "sandbox"))]
        let (program, args) = ("sh".to_string(), vec!["-c".to_string(), command.to_string()]);

        let mut cmd = Command::new(&program);
        cmd.args(&args)
            .current_dir(workdir)
            .kill_on_drop(true)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(unix)]
        {
            cmd.process_group(0);
        }

        #[cfg(all(unix, feature = "sandbox"))]
        if let Some(ref policy) = self.sandbox_policy {
            let policy = policy.clone();
            unsafe {
                cmd.pre_exec(move || {
                    restflow_sandbox::pre_exec_hook(&policy)
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
                });
            }
        }

        let child = cmd.spawn()?;
        #[cfg(unix)]
        let process_group_id = child.id().map(|pid| pid as i32);

        let output =
            match timeout(Duration::from_secs(timeout_secs), child.wait_with_output()).await {
                Ok(result) => result?,
                Err(_) => {
                    #[cfg(unix)]
                    if let Some(process_group_id) = process_group_id {
                        let pgid = Pid::from_raw(process_group_id);
                        let _ = killpg(pgid, Signal::SIGTERM);
                        sleep(Duration::from_millis(500)).await;
                        let _ = killpg(pgid, Signal::SIGKILL);
                    }

                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        format!("Timeout after {timeout_secs} seconds"),
                    ));
                }
            };

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

    fn truncate_output(&self, bytes: &[u8]) -> (String, bool) {
        let total_len = bytes.len();
        let truncated = total_len > self.max_output_bytes;
        let slice = if truncated {
            let mut end = self.max_output_bytes;
            while end > 0 && (bytes[end] & 0xC0) == 0x80 {
                end -= 1;
            }
            &bytes[..end]
        } else {
            bytes
        };

        let text = String::from_utf8_lossy(slice).to_string();
        if truncated {
            (
                format!("{}...\n[Output truncated, {} bytes total]", text, total_len),
                true,
            )
        } else {
            (text, false)
        }
    }

    fn classify_command_failure(stderr: &str) -> (ToolErrorCategory, bool) {
        let normalized = stderr.to_ascii_lowercase();
        let shell_not_found = (normalized.contains("sh:") || normalized.contains("bash:"))
            && normalized.contains("not found");

        if normalized.contains("command not found")
            || normalized.contains("no such file or directory")
            || shell_not_found
        {
            return (ToolErrorCategory::Config, false);
        }

        if normalized.contains("permission denied")
            || normalized.contains("operation not permitted")
            || normalized.contains("unauthorized")
        {
            return (ToolErrorCategory::Auth, false);
        }

        if normalized.contains("connection refused")
            || normalized.contains("connection reset")
            || normalized.contains("timed out")
            || normalized.contains("timeout")
            || normalized.contains("temporary failure in name resolution")
            || normalized.contains("name or service not known")
            || normalized.contains("network is unreachable")
            || normalized.contains("no route to host")
        {
            return (ToolErrorCategory::Network, true);
        }

        (ToolErrorCategory::Execution, false)
    }
}

/// Input parameters for bash command execution.
#[derive(Debug, Deserialize)]
pub struct BashInput {
    pub command: String,
    #[serde(default)]
    pub workdir: Option<String>,
    #[serde(default)]
    pub timeout: Option<u64>,
    #[serde(default)]
    pub yolo_mode: bool,
}

/// Output from bash command execution.
#[derive(Debug, Serialize, Deserialize)]
pub struct BashOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub truncated: bool,
    pub duration_ms: u64,
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Run shell commands in the local environment and return stdout, stderr, and exit status. Use this for command execution; for file content operations, prefer the file tool."
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
                    "description": "Timeout in seconds (default: 300)"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let input: BashInput = serde_json::from_value(input)?;

        let workdir = input
            .workdir
            .or_else(|| self.default_workdir.clone())
            .unwrap_or_else(|| ".".to_string());

        let timeout_secs = input.timeout.unwrap_or(self.timeout_secs);

        if !input.yolo_mode
            && let Some(security_gate) = &self.security_gate
        {
            let agent_id = self
                .agent_id
                .as_deref()
                .ok_or_else(|| crate::ToolError::Tool("Missing agent_id".into()))?;
            let task_id = self
                .task_id
                .as_deref()
                .ok_or_else(|| crate::ToolError::Tool("Missing task_id".into()))?;

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
                        error_category: Some(ToolErrorCategory::Auth),
                        retryable: Some(false),
                        retry_after_ms: None,
                    });
                }

                return Ok(ToolOutput {
                    success: false,
                    result: serde_json::json!({
                        "blocked": true,
                    }),
                    error: decision.reason,
                    error_category: Some(ToolErrorCategory::Config),
                    retryable: Some(false),
                    retry_after_ms: None,
                });
            }
        }

        let start = Instant::now();

        let result = self
            .run_command(&input.command, &workdir, timeout_secs)
            .await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok((exit_code, stdout, stderr, truncated)) => {
                let failure_meta =
                    (exit_code != 0).then(|| Self::classify_command_failure(&stderr));
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
                    error: (exit_code != 0)
                        .then(|| format!("Command exited with code {}", exit_code)),
                    error_category: failure_meta.as_ref().map(|(category, _)| category.clone()),
                    retryable: failure_meta.map(|(_, retryable)| retryable),
                    retry_after_ms: None,
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => Ok(ToolOutput {
                success: false,
                result: serde_json::json!({
                    "error": "Command timed out",
                    "timeout_secs": timeout_secs,
                }),
                error: Some(format!("Timeout after {} seconds", timeout_secs)),
                error_category: Some(ToolErrorCategory::Network),
                retryable: Some(true),
                retry_after_ms: None,
            }),
            Err(e) => Ok(ToolOutput {
                success: false,
                result: serde_json::json!({"error": e.to_string()}),
                error: Some(e.to_string()),
                error_category: Some(ToolErrorCategory::Execution),
                retryable: Some(false),
                retry_after_ms: None,
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
    fn test_bash_tool_name() {
        let tool = BashTool::new();
        assert_eq!(tool.name(), "bash");
    }

    #[test]
    fn test_classify_command_failure_shell_not_found() {
        let (category, retryable) =
            BashTool::classify_command_failure("sh: 1: nonexistent_command_12345: not found");
        assert_eq!(category, ToolErrorCategory::Config);
        assert!(!retryable);
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

    #[tokio::test]
    #[cfg(unix)]
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
    }

    #[tokio::test]
    #[cfg(unix)]
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
}
