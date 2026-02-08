//! CLI Agent Executor for executing external CLI tools (e.g., Claude Code, Aider).
//!
//! This module provides an executor that runs external CLI tools as agent backends,
//! enabling integration with tools like Claude Code CLI while maintaining the same
//! BackgroundAgent infrastructure.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, bail};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::models::CliExecutionConfig;

use super::events::TaskStreamEvent;
use super::runner::ExecutionResult;

/// Output callback type for streaming CLI output
pub type OutputCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// CLI Agent Executor that runs external CLI tools
pub struct CliAgentExecutor {
    /// Optional callback for streaming output lines
    output_callback: Option<OutputCallback>,
    /// Optional channel for streaming output events
    output_sender: Option<mpsc::Sender<String>>,
}

impl Default for CliAgentExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl CliAgentExecutor {
    /// Create a new CLI executor with no output streaming
    pub fn new() -> Self {
        Self {
            output_callback: None,
            output_sender: None,
        }
    }

    /// Create a CLI executor with an output callback for streaming
    pub fn with_output_callback<F>(callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        Self {
            output_callback: Some(Arc::new(callback)),
            output_sender: None,
        }
    }

    /// Create a CLI executor with a channel for streaming output
    pub fn with_output_channel(sender: mpsc::Sender<String>) -> Self {
        Self {
            output_callback: None,
            output_sender: Some(sender),
        }
    }

    /// Execute a CLI command with the given configuration and input
    pub async fn execute_cli(
        &self,
        config: &CliExecutionConfig,
        input: Option<&str>,
    ) -> Result<ExecutionResult> {
        info!(
            binary = %config.binary,
            args = ?config.args,
            working_dir = ?config.working_dir,
            timeout_secs = config.timeout_secs,
            "Starting CLI execution"
        );

        // Build the command
        let mut cmd = Command::new(&config.binary);
        cmd.args(&config.args);

        // Set working directory if specified
        if let Some(ref cwd) = config.working_dir {
            cmd.current_dir(cwd);
        }

        // Add input as prompt argument if provided
        if let Some(input_text) = input {
            cmd.arg("-p").arg(input_text);
        }

        // Configure stdio for output capture
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        // Spawn the process
        let mut child = cmd.spawn().map_err(|e| {
            error!(binary = %config.binary, error = %e, "Failed to spawn CLI process");
            anyhow::anyhow!("Failed to spawn CLI process '{}': {}", config.binary, e)
        })?;

        // Take stdout for streaming
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture stdout"))?;

        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture stderr"))?;

        // Stream stdout
        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let mut output = String::new();
        let mut stderr_output = String::new();

        // Create timeout future
        let timeout_duration = Duration::from_secs(config.timeout_secs);

        let stream_task = async {
            loop {
                tokio::select! {
                    line = stdout_reader.next_line() => {
                        match line {
                            Ok(Some(line)) => {
                                debug!(line = %line, "CLI stdout");
                                output.push_str(&line);
                                output.push('\n');

                                // Stream to callback
                                if let Some(ref cb) = self.output_callback {
                                    cb(&line);
                                }

                                // Stream to channel
                                if let Some(ref sender) = self.output_sender {
                                    let _ = sender.send(line).await;
                                }
                            }
                            Ok(None) => break, // EOF
                            Err(e) => {
                                warn!(error = %e, "Error reading stdout");
                                break;
                            }
                        }
                    }
                    line = stderr_reader.next_line() => {
                        match line {
                            Ok(Some(line)) => {
                                debug!(line = %line, "CLI stderr");
                                stderr_output.push_str(&line);
                                stderr_output.push('\n');
                            }
                            Ok(None) => {} // EOF on stderr
                            Err(e) => {
                                warn!(error = %e, "Error reading stderr");
                            }
                        }
                    }
                }
            }
        };

        // Run with timeout
        let timeout_result = tokio::time::timeout(timeout_duration, stream_task).await;

        if timeout_result.is_err() {
            // Timeout occurred - kill the process
            warn!(
                binary = %config.binary,
                timeout_secs = config.timeout_secs,
                "CLI execution timed out, killing process"
            );
            let _ = child.kill().await;
            bail!(
                "CLI execution timed out after {} seconds",
                config.timeout_secs
            );
        }

        // Wait for process to finish
        let status = child.wait().await?;

        let output_trimmed = output.trim().to_string();

        if !status.success() {
            let exit_code = status.code().unwrap_or(-1);
            error!(
                binary = %config.binary,
                exit_code = exit_code,
                stderr = %stderr_output.trim(),
                "CLI execution failed"
            );
            bail!(
                "CLI '{}' failed with exit code {}: {}",
                config.binary,
                exit_code,
                stderr_output.trim()
            );
        }

        info!(
            binary = %config.binary,
            output_len = output_trimmed.len(),
            "CLI execution completed successfully"
        );

        Ok(ExecutionResult {
            output: output_trimmed,
            messages: vec![], // CLI doesn't produce structured messages
            success: true,
        })
    }
}

/// Create a CliAgentExecutor that emits TaskStreamEvents
pub fn create_cli_executor_with_events<E>(
    task_id: String,
    event_emitter: Arc<E>,
) -> CliAgentExecutor
where
    E: crate::runtime::background_agent::TaskEventEmitter + 'static,
{
    CliAgentExecutor::with_output_callback(move |line| {
        let event = TaskStreamEvent::output(&task_id, line, false);
        // Use blocking emit since we're in a sync callback
        let emitter = event_emitter.clone();
        let event_clone = event.clone();
        tokio::spawn(async move {
            emitter.emit(event_clone).await;
        });
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn test_cli_executor_echo() {
        let executor = CliAgentExecutor::new();
        let config = CliExecutionConfig {
            binary: "echo".to_string(),
            args: vec!["Hello, World!".to_string()],
            working_dir: None,
            timeout_secs: 10,
            use_pty: false,
        };

        let result = executor.execute_cli(&config, None).await;
        assert!(result.is_ok());

        let result = result.unwrap();
        assert!(result.success);
        assert!(result.output.contains("Hello, World!"));
    }

    #[tokio::test]
    async fn test_cli_executor_with_callback() {
        let line_count = Arc::new(AtomicUsize::new(0));
        let line_count_clone = line_count.clone();

        let executor = CliAgentExecutor::with_output_callback(move |_line| {
            line_count_clone.fetch_add(1, Ordering::SeqCst);
        });

        let config = CliExecutionConfig {
            binary: "echo".to_string(),
            args: vec!["line1\nline2\nline3".to_string()],
            working_dir: None,
            timeout_secs: 10,
            use_pty: false,
        };

        let result = executor.execute_cli(&config, None).await;
        assert!(result.is_ok());
        assert!(line_count.load(Ordering::SeqCst) >= 1);
    }

    #[tokio::test]
    async fn test_cli_executor_timeout() {
        let executor = CliAgentExecutor::new();
        let config = CliExecutionConfig {
            binary: "sleep".to_string(),
            args: vec!["10".to_string()],
            working_dir: None,
            timeout_secs: 1, // 1 second timeout
            use_pty: false,
        };

        let result = executor.execute_cli(&config, None).await;
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert!(error.to_string().contains("timed out"));
    }

    #[tokio::test]
    async fn test_cli_executor_nonexistent_binary() {
        let executor = CliAgentExecutor::new();
        let config = CliExecutionConfig {
            binary: "nonexistent_binary_12345".to_string(),
            args: vec![],
            working_dir: None,
            timeout_secs: 10,
            use_pty: false,
        };

        let result = executor.execute_cli(&config, None).await;
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert!(error.to_string().contains("Failed to spawn"));
    }

    #[tokio::test]
    async fn test_cli_executor_with_input() {
        let executor = CliAgentExecutor::new();
        let config = CliExecutionConfig {
            binary: "echo".to_string(),
            args: vec![],
            working_dir: None,
            timeout_secs: 10,
            use_pty: false,
        };

        // Input is added as -p argument
        let result = executor.execute_cli(&config, Some("test input")).await;
        assert!(result.is_ok());

        let result = result.unwrap();
        // echo will output "-p test input"
        assert!(result.output.contains("-p"));
        assert!(result.output.contains("test input"));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_cli_executor_working_dir() {
        let executor = CliAgentExecutor::new();
        let config = CliExecutionConfig {
            binary: "pwd".to_string(),
            args: vec![],
            working_dir: Some("/tmp".to_string()),
            timeout_secs: 10,
            use_pty: false,
        };

        let result = executor.execute_cli(&config, None).await;
        assert!(result.is_ok());

        let result = result.unwrap();
        // On macOS, /tmp is a symlink to /private/tmp
        assert!(result.output.contains("tmp"));
    }

    #[tokio::test]
    #[cfg(windows)]
    async fn test_cli_executor_working_dir() {
        let executor = CliAgentExecutor::new();
        // Use cd command on Windows to print current directory
        let config = CliExecutionConfig {
            binary: "cmd".to_string(),
            args: vec!["/C".to_string(), "cd".to_string()],
            working_dir: Some(std::env::temp_dir().to_string_lossy().to_string()),
            timeout_secs: 10,
            use_pty: false,
        };

        let result = executor.execute_cli(&config, None).await;
        assert!(result.is_ok());

        let result = result.unwrap();
        // Output should contain the temp directory path
        assert!(result.output.to_lowercase().contains("temp"));
    }
}
