//! CLI-based agent executor for external coding agents.
//!
//! This module provides `CliExecutor`, which implements the `AgentExecutor`
//! trait by invoking external CLI tools like `claude`, `aider`, `codex`, etc.
//! It supports configurable timeouts, working directories, and PTY execution.

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use tracing::{debug, error, info, warn};

use restflow_core::models::CliExecutionConfig;

use super::runner::AgentExecutor;

/// CLI-based agent executor that runs external coding agent CLIs.
///
/// This executor:
/// - Spawns the configured CLI binary with the task input as argument
/// - Captures stdout/stderr for output
/// - Respects timeout configuration
/// - Supports custom working directory
/// - Optionally uses PTY for interactive CLIs (future enhancement)
///
/// # Supported CLIs
///
/// - `claude` - Anthropic's Claude Code CLI (recommended)
/// - `aider` - AI pair programming in your terminal
/// - `codex` - OpenAI's Codex CLI
/// - Any CLI that accepts a prompt and outputs to stdout
///
/// # Example
///
/// ```ignore
/// use restflow_core::models::CliExecutionConfig;
/// use restflow_tauri::agent_task::CliExecutor;
///
/// let config = CliExecutionConfig {
///     binary: "claude".to_string(),
///     args: vec!["-p".to_string()],
///     working_dir: Some("/path/to/project".to_string()),
///     timeout_secs: 300,
///     use_pty: false,
/// };
///
/// let executor = CliExecutor::new(config);
/// let result = executor.execute("agent-1", Some("Write hello world")).await?;
/// ```
pub struct CliExecutor {
    config: CliExecutionConfig,
}

impl CliExecutor {
    /// Create a new CliExecutor with the given configuration.
    pub fn new(config: CliExecutionConfig) -> Self {
        Self { config }
    }

    /// Create a new CliExecutor with default configuration for claude CLI.
    pub fn default_claude() -> Self {
        Self {
            config: CliExecutionConfig {
                binary: "claude".to_string(),
                args: vec!["-p".to_string()],
                working_dir: None,
                timeout_secs: 300,
                use_pty: false,
            },
        }
    }

    /// Build the command arguments for the CLI.
    ///
    /// Different CLIs have different argument patterns:
    /// - claude: `claude -p "prompt"` (print mode, no interactive)
    /// - aider: `aider --message "prompt" --yes`
    /// - Custom: user-configured args + input as final arg
    fn build_args(&self, input: &str) -> Vec<String> {
        let mut args = self.config.args.clone();

        match self.config.binary.as_str() {
            "claude" => {
                // Claude CLI: use -p for print mode if not already specified
                if !args.contains(&"-p".to_string()) && !args.contains(&"--print".to_string()) {
                    args.push("-p".to_string());
                }
                // Add the prompt as the final argument
                args.push(input.to_string());
            }
            "aider" => {
                // Aider: use --message for non-interactive mode
                if !args.contains(&"--message".to_string()) && !args.contains(&"-m".to_string()) {
                    args.push("--message".to_string());
                    args.push(input.to_string());
                }
                // Add --yes to auto-confirm if not present
                if !args.contains(&"--yes".to_string()) && !args.contains(&"-y".to_string()) {
                    args.push("--yes".to_string());
                }
            }
            _ => {
                // Generic CLI: append input as final argument
                args.push(input.to_string());
            }
        }

        args
    }

    /// Execute the CLI command and capture output.
    async fn run_cli(&self, args: Vec<String>) -> Result<String> {
        let binary = &self.config.binary;
        let timeout_duration = Duration::from_secs(self.config.timeout_secs);

        info!(
            "Executing CLI: {} {} (timeout: {}s)",
            binary,
            args.join(" "),
            self.config.timeout_secs
        );

        let mut cmd = Command::new(binary);
        cmd.args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Set working directory if specified
        if let Some(ref dir) = self.config.working_dir {
            debug!("Setting working directory: {}", dir);
            cmd.current_dir(dir);
        }

        // Spawn the process
        let mut child = cmd.spawn().with_context(|| {
            format!(
                "Failed to spawn CLI '{}'. Is it installed and in PATH?",
                binary
            )
        })?;

        // Capture stdout and stderr
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Failed to capture stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("Failed to capture stderr"))?;

        // Create buffered readers
        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let mut output_lines = Vec::new();
        let mut error_lines = Vec::new();

        // Read output with timeout
        let read_output = async {
            loop {
                tokio::select! {
                    line = stdout_reader.next_line() => {
                        match line {
                            Ok(Some(line)) => {
                                debug!("stdout: {}", line);
                                output_lines.push(line);
                            }
                            Ok(None) => break,
                            Err(e) => {
                                warn!("Error reading stdout: {}", e);
                                break;
                            }
                        }
                    }
                    line = stderr_reader.next_line() => {
                        match line {
                            Ok(Some(line)) => {
                                debug!("stderr: {}", line);
                                error_lines.push(line);
                            }
                            Ok(None) => {}
                            Err(e) => {
                                warn!("Error reading stderr: {}", e);
                            }
                        }
                    }
                }
            }

            // Wait for process to complete
            child.wait().await
        };

        let status = match timeout(timeout_duration, read_output).await {
            Ok(Ok(status)) => status,
            Ok(Err(e)) => {
                error!("CLI process error: {}", e);
                // Try to kill the process
                let _ = child.kill().await;
                return Err(anyhow!("CLI process error: {}", e));
            }
            Err(_) => {
                error!(
                    "CLI execution timed out after {}s",
                    self.config.timeout_secs
                );
                // Kill the timed-out process
                let _ = child.kill().await;
                return Err(anyhow!(
                    "CLI execution timed out after {} seconds",
                    self.config.timeout_secs
                ));
            }
        };

        // Check exit status
        if !status.success() {
            let exit_code = status.code().unwrap_or(-1);
            let stderr_output = error_lines.join("\n");
            error!(
                "CLI exited with code {}: {}",
                exit_code,
                stderr_output.chars().take(500).collect::<String>()
            );
            return Err(anyhow!(
                "CLI exited with code {}:\n{}",
                exit_code,
                stderr_output
            ));
        }

        // Combine output
        let output = output_lines.join("\n");

        if output.is_empty() && !error_lines.is_empty() {
            // Some CLIs output to stderr for progress, check if it looks like an error
            let stderr_output = error_lines.join("\n");
            if stderr_output.to_lowercase().contains("error") {
                warn!("CLI produced no stdout but stderr contains errors");
                return Err(anyhow!("CLI error: {}", stderr_output));
            }
        }

        info!(
            "CLI execution completed successfully ({} lines of output)",
            output_lines.len()
        );

        Ok(output)
    }
}

#[async_trait]
impl AgentExecutor for CliExecutor {
    /// Execute an agent via CLI.
    ///
    /// The agent_id is informational (for logging). The actual CLI binary
    /// and configuration come from the CliExecutionConfig.
    ///
    /// # Arguments
    ///
    /// * `agent_id` - Identifier for logging purposes
    /// * `input` - The prompt/task to send to the CLI
    ///
    /// # Returns
    ///
    /// The CLI's stdout output on success, or an error with details.
    async fn execute(&self, agent_id: &str, input: Option<&str>) -> Result<String> {
        let input = input.unwrap_or("Execute the task");

        info!(
            "CliExecutor executing agent '{}' with binary '{}'",
            agent_id, self.config.binary
        );

        // Build arguments based on CLI type and input
        let args = self.build_args(input);

        // Run the CLI
        let result = self.run_cli(args).await?;

        // Post-process output if needed
        let output = self.process_output(&result);

        Ok(output)
    }
}

impl CliExecutor {
    /// Process CLI output to extract the relevant response.
    ///
    /// Some CLIs include progress messages, spinners, or metadata that
    /// we may want to filter out.
    fn process_output(&self, raw_output: &str) -> String {
        match self.config.binary.as_str() {
            "claude" => {
                // Claude CLI with -p flag outputs the response directly
                // Filter out any ANSI escape codes that might slip through
                Self::strip_ansi_codes(raw_output)
            }
            "aider" => {
                // Aider includes git commit messages and other metadata
                // Extract the main response
                Self::extract_aider_response(raw_output)
            }
            _ => {
                // Generic: strip ANSI codes and return
                Self::strip_ansi_codes(raw_output)
            }
        }
    }

    /// Strip ANSI escape codes from output.
    fn strip_ansi_codes(text: &str) -> String {
        // Simple regex-free ANSI stripping
        let mut result = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '\x1b' {
                // Skip escape sequence
                if chars.peek() == Some(&'[') {
                    chars.next(); // consume '['
                    // Skip until we hit a letter (the terminator)
                    while let Some(&next) = chars.peek() {
                        chars.next();
                        if next.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
            } else {
                result.push(c);
            }
        }

        result
    }

    /// Extract the main response from aider output.
    fn extract_aider_response(output: &str) -> String {
        // Aider output can include:
        // - Git status/commit info
        // - File change summaries
        // - The actual AI response
        //
        // We try to extract the meaningful content
        let lines: Vec<&str> = output.lines().collect();
        let mut response_lines = Vec::new();
        let mut in_response = false;

        for line in lines {
            // Skip common aider metadata lines
            if line.starts_with("Commit ")
                || line.starts_with("Applied ")
                || line.starts_with("Git diff:")
                || line.starts_with("───")
            {
                continue;
            }

            // Include content lines
            if !line.trim().is_empty() || in_response {
                in_response = true;
                response_lines.push(line);
            }
        }

        Self::strip_ansi_codes(&response_lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_executor_creation() {
        let config = CliExecutionConfig::default();
        let executor = CliExecutor::new(config);
        assert_eq!(executor.config.binary, "claude");
    }

    #[test]
    fn test_default_claude_executor() {
        let executor = CliExecutor::default_claude();
        assert_eq!(executor.config.binary, "claude");
        assert!(executor.config.args.contains(&"-p".to_string()));
        assert_eq!(executor.config.timeout_secs, 300);
    }

    #[test]
    fn test_build_args_claude() {
        let executor = CliExecutor::default_claude();
        let args = executor.build_args("Hello world");

        // Should have -p and the prompt
        assert!(args.contains(&"-p".to_string()));
        assert!(args.contains(&"Hello world".to_string()));
    }

    #[test]
    fn test_build_args_claude_no_duplicate_p() {
        let config = CliExecutionConfig {
            binary: "claude".to_string(),
            args: vec!["-p".to_string()],
            ..Default::default()
        };
        let executor = CliExecutor::new(config);
        let args = executor.build_args("Test");

        // Should not have duplicate -p
        let p_count = args.iter().filter(|a| *a == "-p").count();
        assert_eq!(p_count, 1);
    }

    #[test]
    fn test_build_args_aider() {
        let config = CliExecutionConfig {
            binary: "aider".to_string(),
            args: vec![],
            ..Default::default()
        };
        let executor = CliExecutor::new(config);
        let args = executor.build_args("Fix the bug");

        assert!(args.contains(&"--message".to_string()));
        assert!(args.contains(&"Fix the bug".to_string()));
        assert!(args.contains(&"--yes".to_string()));
    }

    #[test]
    fn test_build_args_generic() {
        let config = CliExecutionConfig {
            binary: "my-custom-cli".to_string(),
            args: vec!["--flag".to_string()],
            ..Default::default()
        };
        let executor = CliExecutor::new(config);
        let args = executor.build_args("Do something");

        assert_eq!(args, vec!["--flag", "Do something"]);
    }

    #[test]
    fn test_strip_ansi_codes() {
        let text_with_ansi = "\x1b[32mGreen text\x1b[0m and normal";
        let clean = CliExecutor::strip_ansi_codes(text_with_ansi);
        assert_eq!(clean, "Green text and normal");
    }

    #[test]
    fn test_strip_ansi_codes_complex() {
        let text = "\x1b[1m\x1b[34mBold Blue\x1b[0m\x1b[K";
        let clean = CliExecutor::strip_ansi_codes(text);
        assert_eq!(clean, "Bold Blue");
    }

    #[test]
    fn test_extract_aider_response() {
        let aider_output = r#"
Commit abc123: Fix the bug
Applied changes to src/main.rs
Git diff: +2 -1
───────────────────────

Here is the fixed code:
- Removed the bug
- Added proper error handling
"#;

        let response = CliExecutor::extract_aider_response(aider_output);

        // Should not include git metadata
        assert!(!response.contains("Commit abc123"));
        assert!(!response.contains("Applied changes"));
        assert!(!response.contains("Git diff"));

        // Should include the actual response
        assert!(response.contains("Here is the fixed code"));
    }

    #[tokio::test]
    async fn test_executor_with_nonexistent_binary() {
        let config = CliExecutionConfig {
            binary: "nonexistent-cli-tool-xyz".to_string(),
            args: vec![],
            timeout_secs: 5,
            ..Default::default()
        };
        let executor = CliExecutor::new(config);

        let result = executor.execute("test-agent", Some("test input")).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Failed to spawn") || err_msg.contains("not found"),
            "Error should mention spawn failure: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_executor_with_echo() {
        // Use 'echo' as a simple CLI that always exists
        let config = CliExecutionConfig {
            binary: "echo".to_string(),
            args: vec![],
            timeout_secs: 5,
            working_dir: None,
            use_pty: false,
        };
        let executor = CliExecutor::new(config);

        let result = executor.execute("test-agent", Some("Hello World")).await;
        assert!(result.is_ok(), "echo should succeed: {:?}", result);

        let output = result.unwrap();
        assert!(
            output.contains("Hello World"),
            "Output should contain input: {}",
            output
        );
    }

    #[tokio::test]
    async fn test_executor_timeout() {
        // Test timeout by creating a custom binary configuration that ignores input
        // We use "bash -c" as the binary and make input irrelevant
        let config = CliExecutionConfig {
            binary: "bash".to_string(),
            // Pre-configure args so input becomes irrelevant after joining
            args: vec![
                "-c".to_string(),
                "sleep 10; echo done; #".to_string(), // The "#" makes trailing input a comment
            ],
            timeout_secs: 1, // 1 second timeout
            working_dir: None,
            use_pty: false,
        };
        let executor = CliExecutor::new(config);

        let result = executor.execute("test-agent", Some("ignored")).await;
        assert!(result.is_err());

        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("timed out"),
            "Error should mention timeout: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_executor_exit_code_failure() {
        // Use 'false' command which always exits with code 1
        let config = CliExecutionConfig {
            binary: "false".to_string(),
            args: vec![],
            timeout_secs: 5,
            working_dir: None,
            use_pty: false,
        };
        let executor = CliExecutor::new(config);

        let result = executor.execute("test-agent", None).await;
        assert!(result.is_err());

        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("exited with code"),
            "Error should mention exit code: {}",
            err_msg
        );
    }

    #[test]
    fn test_working_dir_config() {
        let config = CliExecutionConfig {
            binary: "ls".to_string(),
            args: vec!["-la".to_string()],
            working_dir: Some("/tmp".to_string()),
            timeout_secs: 30,
            use_pty: false,
        };
        let executor = CliExecutor::new(config);

        assert_eq!(executor.config.working_dir, Some("/tmp".to_string()));
    }
}
