//! PTY-based CLI Executor for interactive coding agents.
//!
//! This module provides `PtyCliExecutor`, which executes external CLI tools
//! in a proper pseudo-terminal (PTY) environment. This is essential for
//! interactive CLI tools like Claude Code that require TTY features.
//!
//! # Features
//!
//! - Full PTY support for interactive CLI tools
//! - Proper TTY emulation (terminal size, escape sequences)
//! - Streaming output capture with timeout support
//! - Works with Claude Code, aider, and other TTY-requiring CLIs
//!
//! # Example
//!
//! ```ignore
//! use restflow_tauri::agent_task::PtyCliExecutor;
//! use restflow_core::models::CliExecutionConfig;
//!
//! let config = CliExecutionConfig {
//!     binary: "claude".to_string(),
//!     args: vec!["--dangerously-skip-permissions".to_string()],
//!     working_dir: Some("/path/to/project".to_string()),
//!     timeout_secs: 300,
//!     use_pty: true,
//! };
//!
//! let executor = PtyCliExecutor::new(config);
//! let result = executor.execute("agent-1", Some("Fix the bug")).await?;
//! ```

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tokio::sync::oneshot;
use tracing::{debug, error, info, warn};

use restflow_core::models::CliExecutionConfig;

use super::runner::AgentExecutor;

/// Default PTY terminal size (columns)
const DEFAULT_COLS: u16 = 120;

/// Default PTY terminal size (rows)
const DEFAULT_ROWS: u16 = 40;

/// Maximum output buffer size (5MB)
const MAX_OUTPUT_SIZE: usize = 5_000_000;

/// Read buffer size
const READ_BUFFER_SIZE: usize = 4096;

/// PTY-based CLI executor for interactive coding agents.
///
/// Unlike `CliExecutor`, this executor creates a full PTY environment,
/// which is required for CLI tools that need terminal features like:
/// - Terminal size detection
/// - ANSI escape sequence handling
/// - Interactive prompts (even in non-interactive mode)
/// - Proper signal handling
pub struct PtyCliExecutor {
    config: CliExecutionConfig,
}

impl PtyCliExecutor {
    /// Create a new PtyCliExecutor with the given configuration.
    pub fn new(config: CliExecutionConfig) -> Self {
        Self { config }
    }

    /// Create a new PtyCliExecutor with default configuration for Claude CLI.
    ///
    /// This configuration is optimized for running Claude Code in a PTY:
    /// - Uses `--dangerously-skip-permissions` for non-interactive execution
    /// - 10-minute timeout for complex tasks
    /// - PTY mode enabled
    pub fn default_claude() -> Self {
        Self {
            config: CliExecutionConfig {
                binary: "claude".to_string(),
                args: vec!["--dangerously-skip-permissions".to_string()],
                working_dir: None,
                timeout_secs: 600,
                use_pty: true,
            },
        }
    }

    /// Create a new PtyCliExecutor for aider.
    pub fn default_aider() -> Self {
        Self {
            config: CliExecutionConfig {
                binary: "aider".to_string(),
                args: vec!["--yes".to_string()],
                working_dir: None,
                timeout_secs: 600,
                use_pty: true,
            },
        }
    }

    /// Build the command arguments for the CLI.
    fn build_args(&self, input: &str) -> Vec<String> {
        let mut args = self.config.args.clone();

        match self.config.binary.as_str() {
            "claude" => {
                // Claude Code CLI: prompt goes at the end
                // --dangerously-skip-permissions is already in default args
                args.push(input.to_string());
            }
            "aider" => {
                // Aider: use --message for the prompt
                if !args.contains(&"--message".to_string()) && !args.contains(&"-m".to_string()) {
                    args.push("--message".to_string());
                    args.push(input.to_string());
                }
                // Ensure --yes is present
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

    /// Execute the CLI in a PTY environment.
    ///
    /// This spawns the CLI in a pseudo-terminal, which provides:
    /// - Proper TTY environment
    /// - Terminal size (TERM=xterm-256color)
    /// - Capture of all output including escape sequences
    async fn run_pty(&self, args: Vec<String>) -> Result<String> {
        let binary = self.config.binary.clone();
        let timeout_secs = self.config.timeout_secs;
        let working_dir = self.config.working_dir.clone();
        let timeout_duration = Duration::from_secs(timeout_secs);

        info!(
            "Executing PTY CLI: {} {} (timeout: {}s)",
            binary,
            args.join(" "),
            timeout_secs
        );

        // Create a channel to receive the result
        let (tx, rx) = oneshot::channel::<Result<String>>();

        // Spawn the PTY work in a blocking task
        let args_clone = args.clone();
        let binary_clone = binary.clone();
        tokio::task::spawn_blocking(move || {
            let result = run_pty_sync(&binary_clone, &args_clone, working_dir.as_deref(), timeout_duration);
            let _ = tx.send(result);
        });

        // Wait for the result with timeout
        match tokio::time::timeout(timeout_duration + Duration::from_secs(5), rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(anyhow!("PTY task was cancelled")),
            Err(_) => Err(anyhow!(
                "PTY execution timed out after {}s",
                timeout_secs + 5
            )),
        }
    }
}

/// Synchronous PTY execution (runs in blocking thread).
fn run_pty_sync(
    binary: &str,
    args: &[String],
    working_dir: Option<&str>,
    timeout: Duration,
) -> Result<String> {
    let pty_system = native_pty_system();

    // Create PTY with default size
    let pair = pty_system
        .openpty(PtySize {
            rows: DEFAULT_ROWS,
            cols: DEFAULT_COLS,
            pixel_width: 0,
            pixel_height: 0,
        })
        .context("Failed to open PTY")?;

    // Build the command
    let mut cmd = CommandBuilder::new(binary);
    cmd.args(args);

    // Set working directory if specified
    if let Some(dir) = working_dir {
        let expanded = expand_tilde(dir);
        cmd.cwd(expanded);
    }

    // Set terminal environment
    cmd.env("TERM", "xterm-256color");
    // Disable color for easier parsing
    cmd.env("NO_COLOR", "1");
    // Set non-interactive mode hints
    cmd.env("CI", "true");
    cmd.env("NONINTERACTIVE", "1");

    debug!("Spawning PTY command: {} {:?}", binary, args);

    // Spawn the process
    let _child = pair
        .slave
        .spawn_command(cmd)
        .context("Failed to spawn command in PTY")?;

    // Drop the slave to signal we're done with it
    drop(pair.slave);

    // Get reader for output
    let mut reader = pair
        .master
        .try_clone_reader()
        .context("Failed to get PTY reader")?;

    // Output buffer
    let output = Arc::new(Mutex::new(String::new()));
    let output_clone = output.clone();

    // Tracking for timeout
    let start_time = Instant::now();
    #[allow(unused_assignments)]
    let mut last_output_time = start_time;

    // Read output in a loop
    let mut buf = [0u8; READ_BUFFER_SIZE];
    let mut incomplete_utf8: Vec<u8> = Vec::new();
    let mut child_exited = false;

    // We'll poll for output with small timeouts
    loop {
        // Check if we've exceeded the timeout
        if start_time.elapsed() > timeout {
            warn!("PTY execution timed out after {:?}", timeout);
            // Try to kill the child process
            drop(pair.master);
            return Err(anyhow!(
                "PTY execution timed out after {}s",
                timeout.as_secs()
            ));
        }

        // Check for child exit status (non-blocking attempt)
        // portable_pty's Child doesn't have a try_wait, so we rely on EOF

        // Try to read with a short timeout (100ms)
        match reader.read(&mut buf) {
            Ok(0) => {
                // EOF - process exited
                child_exited = true;
                debug!("PTY EOF received, process exited");

                // Flush any remaining incomplete bytes
                if !incomplete_utf8.is_empty() {
                    let data = String::from_utf8_lossy(&incomplete_utf8).to_string();
                    if let Ok(mut out) = output_clone.lock() {
                        out.push_str(&data);
                    }
                }
                break;
            }
            Ok(n) => {
                last_output_time = Instant::now();

                // Prepend any incomplete UTF-8 bytes from previous read
                let mut bytes = std::mem::take(&mut incomplete_utf8);
                bytes.extend_from_slice(&buf[..n]);

                // Find valid UTF-8 boundary
                let valid_up_to = find_utf8_boundary(&bytes);

                if valid_up_to > 0 {
                    let data = String::from_utf8_lossy(&bytes[..valid_up_to]).to_string();

                    if let Ok(mut out) = output_clone.lock() {
                        // Check buffer size limit
                        if out.len() + data.len() > MAX_OUTPUT_SIZE {
                            warn!("Output buffer exceeded maximum size, truncating");
                            // Keep the last 90%
                            let keep_from = out.len().saturating_sub(MAX_OUTPUT_SIZE * 9 / 10);
                            *out = out[keep_from..].to_string();
                        }
                        out.push_str(&data);
                    }
                }

                // Save incomplete bytes for next iteration
                if valid_up_to < bytes.len() {
                    incomplete_utf8 = bytes[valid_up_to..].to_vec();
                }
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    // No data available yet, sleep briefly and retry
                    thread::sleep(Duration::from_millis(50));
                    continue;
                }
                // Real error
                error!("PTY read error: {}", e);
                break;
            }
        }

        // If no output for 30 seconds after start, might be stuck
        if start_time.elapsed() > Duration::from_secs(30)
            && last_output_time.elapsed() > Duration::from_secs(30)
        {
            warn!("No PTY output for 30 seconds, process may be stuck");
        }
    }

    // Get final output
    let final_output = output.lock().map(|o| o.clone()).unwrap_or_default();

    // Clean up the output (strip ANSI codes, normalize whitespace)
    let cleaned = strip_ansi_codes(&final_output);

    if child_exited {
        info!("PTY CLI completed with {} bytes of output", cleaned.len());
        Ok(cleaned)
    } else {
        Err(anyhow!("PTY process did not exit cleanly"))
    }
}

/// Find the last valid UTF-8 boundary in a byte slice.
fn find_utf8_boundary(bytes: &[u8]) -> usize {
    match std::str::from_utf8(bytes) {
        Ok(_) => bytes.len(),
        Err(e) => e.valid_up_to(),
    }
}

/// Expand tilde (~) to home directory.
fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{}{}", home, &path[1..]);
        }
    } else if path == "~" && let Ok(home) = std::env::var("HOME") {
        return home;
    }
    path.to_string()
}

/// Strip ANSI escape codes from output.
fn strip_ansi_codes(s: &str) -> String {
    // Regex pattern for ANSI escape sequences
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Start of escape sequence
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                // Skip until we hit a letter (the command character)
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else if chars.peek() == Some(&']') {
                // OSC sequence (Operating System Command)
                chars.next(); // consume ']'
                // Skip until BEL (\x07) or ST (ESC \)
                while let Some(next) = chars.next() {
                    if next == '\x07' {
                        break;
                    }
                    if next == '\x1b' && chars.peek() == Some(&'\\') {
                        chars.next();
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

#[async_trait]
impl AgentExecutor for PtyCliExecutor {
    async fn execute(&self, agent_id: &str, input: Option<&str>) -> Result<String> {
        let prompt = input.unwrap_or("Execute the configured task");

        debug!(
            "PtyCliExecutor: executing agent '{}' with prompt: {}...",
            agent_id,
            &prompt[..std::cmp::min(50, prompt.len())]
        );

        let args = self.build_args(prompt);
        self.run_pty(args).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi_codes() {
        let input = "\x1b[32mGreen\x1b[0m Normal";
        let output = strip_ansi_codes(input);
        assert_eq!(output, "Green Normal");
    }

    #[test]
    fn test_strip_ansi_codes_complex() {
        let input = "\x1b[1;31;40mBold Red on Black\x1b[0m";
        let output = strip_ansi_codes(input);
        assert_eq!(output, "Bold Red on Black");
    }

    #[test]
    fn test_strip_ansi_osc() {
        let input = "\x1b]0;Window Title\x07Normal text";
        let output = strip_ansi_codes(input);
        assert_eq!(output, "Normal text");
    }

    #[test]
    fn test_expand_tilde() {
        // This test depends on HOME being set
        if std::env::var("HOME").is_ok() {
            let expanded = expand_tilde("~/test");
            assert!(!expanded.starts_with("~/"));
            assert!(expanded.contains("test"));
        }
    }

    #[test]
    fn test_find_utf8_boundary() {
        let valid = "Hello".as_bytes();
        assert_eq!(find_utf8_boundary(valid), 5);

        // Incomplete UTF-8 sequence (first byte of a 2-byte char)
        let incomplete = &[0x48, 0x65, 0x6c, 0x6c, 0x6f, 0xC2]; // "Hello" + incomplete
        assert_eq!(find_utf8_boundary(incomplete), 5);
    }

    #[test]
    fn test_build_args_claude() {
        let executor = PtyCliExecutor::default_claude();
        let args = executor.build_args("Write hello world");
        assert!(args.contains(&"--dangerously-skip-permissions".to_string()));
        assert!(args.contains(&"Write hello world".to_string()));
    }

    #[test]
    fn test_build_args_aider() {
        let executor = PtyCliExecutor::default_aider();
        let args = executor.build_args("Fix the bug");
        assert!(args.contains(&"--message".to_string()));
        assert!(args.contains(&"--yes".to_string()));
        assert!(args.contains(&"Fix the bug".to_string()));
    }

    #[tokio::test]
    async fn test_pty_executor_echo() {
        // Simple test using echo command
        let config = CliExecutionConfig {
            binary: "echo".to_string(),
            args: vec![],
            working_dir: None,
            timeout_secs: 10,
            use_pty: true,
        };

        let executor = PtyCliExecutor::new(config);
        let result = executor.execute("test", Some("hello")).await;

        assert!(result.is_ok(), "Echo should succeed: {:?}", result);
        let output = result.unwrap();
        assert!(
            output.contains("hello"),
            "Output should contain 'hello': {}",
            output
        );
    }

    #[tokio::test]
    async fn test_pty_executor_timeout() {
        // Test that timeout works using 'cat' which blocks waiting for input
        // Note: In PTY mode, cat will wait for stdin indefinitely
        let config = CliExecutionConfig {
            binary: "bash".to_string(),
            args: vec!["-c".to_string()],
            working_dir: None,
            timeout_secs: 1,
            use_pty: true,
        };

        let executor = PtyCliExecutor::new(config);
        // The prompt becomes the bash command to execute
        let result = executor.execute("test", Some("sleep 10")).await;

        assert!(result.is_err(), "Should timeout: {:?}", result);
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("timeout") || err.contains("timed out"),
            "Error should mention timeout: {}",
            err
        );
    }
}
