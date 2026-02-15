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
use tokio::time::{Duration, sleep, timeout};

#[cfg(unix)]
use nix::sys::signal::{Signal, killpg};
#[cfg(unix)]
use nix::unistd::Pid;

use super::traits::{Tool, ToolErrorCategory, ToolOutput};
use crate::error::Result;
use crate::security::SecurityGate;

/// Default timeout for command execution in seconds
const DEFAULT_TIMEOUT_SECS: u64 = 300;

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
        timeout_secs: u64,
    ) -> std::result::Result<(i32, String, String, bool), std::io::Error> {
        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(command)
            .current_dir(workdir)
            .kill_on_drop(true)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(unix)]
        {
            // Put the shell into its own process group so timeout/cancel can terminate the full tree.
            cmd.process_group(0);
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

    /// Truncate output if it exceeds max size.
    /// Finds a valid UTF-8 boundary to avoid splitting multi-byte characters.
    fn truncate_output(&self, bytes: &[u8]) -> (String, bool) {
        let total_len = bytes.len();
        let truncated = total_len > self.max_output_bytes;
        let slice = if truncated {
            // Walk backwards from the cut point to find a valid UTF-8 boundary.
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
                format!("{}...\n[Output truncated, {} bytes total]", text, total_len,),
                true,
            )
        } else {
            (text, false)
        }
    }

    fn classify_command_failure(stderr: &str) -> (ToolErrorCategory, bool) {
        let normalized = stderr.to_ascii_lowercase();

        if normalized.contains("command not found")
            || normalized.contains("no such file or directory")
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
    /// Internal flag for executor-driven approval bypass.
    #[serde(default)]
    pub yolo_mode: bool,
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

        if !input.yolo_mode
            && let Some(security_gate) = &self.security_gate
        {
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
    use crate::security::{SecurityDecision, SecurityGate};
    use async_trait::async_trait;
    use std::sync::Arc;
    use tempfile::tempdir;

    struct AlwaysApprovalGate;

    #[async_trait]
    impl SecurityGate for AlwaysApprovalGate {
        async fn check_command(
            &self,
            _command: &str,
            _task_id: &str,
            _agent_id: &str,
            _workdir: Option<&str>,
        ) -> Result<SecurityDecision> {
            Ok(SecurityDecision::requires_approval(
                "approval-1".to_string(),
                Some("needs approval".to_string()),
            ))
        }
    }

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
        assert!(tool.description().contains("file tool"));
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
    fn test_truncate_output_multibyte_boundary() {
        // "你好" is 6 bytes (3 per char). Setting max to 4 should cut before
        // the second character, not in the middle of its 3-byte sequence.
        let tool = BashTool::new().with_max_output(4);
        let data = "你好世界".as_bytes(); // 12 bytes
        let (text, truncated) = tool.truncate_output(data);
        assert!(truncated);
        // Should contain only the first full character "你" (3 bytes)
        assert!(text.starts_with("你"));
        assert!(!text.contains('�'));
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
        assert!(!input.yolo_mode);
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
        assert!(!input.yolo_mode);
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
        assert_eq!(output.error_category, Some(ToolErrorCategory::Config));
        assert_eq!(output.retryable, Some(false));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_bash_tool_execute_network_classified_as_retryable() {
        let tool = BashTool::new();
        let output = tool
            .execute(serde_json::json!({
                "command": "echo 'Connection refused' >&2; exit 7"
            }))
            .await
            .unwrap();

        assert!(!output.success);
        assert_eq!(output.error_category, Some(ToolErrorCategory::Network));
        assert_eq!(output.retryable, Some(true));
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

    #[tokio::test]
    #[cfg(unix)]
    async fn test_bash_tool_yolo_mode_bypasses_security_gate() {
        let tool = BashTool::new().with_security(Arc::new(AlwaysApprovalGate), "agent-1", "task-1");

        let blocked = tool
            .execute(serde_json::json!({
                "command": "echo blocked"
            }))
            .await
            .unwrap();
        assert!(!blocked.success);
        assert_eq!(blocked.result["pending_approval"], true);

        let yolo = tool
            .execute(serde_json::json!({
                "command": "echo allowed",
                "yolo_mode": true
            }))
            .await
            .unwrap();
        assert!(yolo.success);
        let result: BashOutput = serde_json::from_value(yolo.result).unwrap();
        assert!(result.stdout.contains("allowed"));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_bash_tool_timeout_kills_spawned_child_processes() {
        let tool = BashTool::new().with_timeout(1);
        let temp_dir = tempdir().unwrap();
        let pid_file = temp_dir.path().join("child.pid");
        let command = format!(
            "sleep 30 & child=$!; echo $child > {}; wait",
            pid_file.display()
        );

        let output = tool
            .execute(serde_json::json!({
                "command": command
            }))
            .await
            .unwrap();

        assert!(!output.success);
        assert!(output.error.as_ref().unwrap().contains("Timeout"));

        let child_pid = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .parse::<i32>()
            .unwrap();

        sleep(Duration::from_millis(300)).await;

        let status = Command::new("sh")
            .arg("-c")
            .arg(format!("kill -0 {child_pid}"))
            .status()
            .await
            .unwrap();

        assert!(
            !status.success(),
            "Child process should be terminated on timeout, pid={child_pid}",
        );
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
