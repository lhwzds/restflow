//! Claude Code CLI LLM provider

use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, info};

use crate::error::{AiError, Result};
use crate::llm::client::{
    CompletionRequest, CompletionResponse, FinishReason, LlmClient, Role, StreamResult,
};

/// Claude Code CLI client (OAuth via CLAUDE_CODE_OAUTH_TOKEN)
pub struct ClaudeCodeClient {
    oauth_token: String,
    model: String,
}

impl ClaudeCodeClient {
    /// Create a new Claude Code client
    pub fn new(oauth_token: impl Into<String>) -> Self {
        Self {
            oauth_token: oauth_token.into(),
            model: "claude-sonnet-4-20250514".to_string(),
        }
    }

    /// Set the model to use
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    fn build_prompt(messages: &[crate::llm::Message]) -> String {
        messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    fn build_cli_command(&self, prompt: &str) -> Result<Command> {
        let executable = resolve_executable("claude", "RESTFLOW_CLAUDE_BIN", &claude_fallbacks())?;
        let mut cmd = Command::new(executable);
        cmd.env("CLAUDE_CODE_OAUTH_TOKEN", &self.oauth_token)
            .env_remove("CLAUDECODE")
            .arg("--print")
            .arg("--permission-mode")
            .arg("bypassPermissions")
            .arg("--dangerously-skip-permissions")
            .arg("--model")
            .arg(&self.model)
            .arg(prompt)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        Ok(cmd)
    }
}

#[async_trait]
impl LlmClient for ClaudeCodeClient {
    fn provider(&self) -> &str {
        "claude-code"
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        info!("ClaudeCodeClient: executing via CLI");

        let prompt = Self::build_prompt(&request.messages);

        let mut cmd = self.build_cli_command(&prompt)?;
        let output = cmd.output().await.map_err(|e| {
            AiError::Llm(format!(
                "Failed to run claude CLI: {}. Install with: npm install -g @anthropic-ai/claude-code",
                e
            ))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AiError::Llm(format!("Claude CLI error: {}", stderr)));
        }

        let content = String::from_utf8_lossy(&output.stdout).trim().to_string();
        debug!("Claude CLI response: {} chars", content.len());

        Ok(CompletionResponse {
            content: Some(content),
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            usage: None,
        })
    }

    fn complete_stream(&self, _request: CompletionRequest) -> StreamResult {
        Box::pin(async_stream::stream! {
            yield Err(AiError::Llm(
                "Streaming not supported with Claude Code CLI".to_string()
            ));
        })
    }

    fn supports_streaming(&self) -> bool {
        false
    }
}

fn claude_fallbacks() -> Vec<PathBuf> {
    let mut paths = vec![
        PathBuf::from("/opt/homebrew/bin/claude"),
        PathBuf::from("/usr/local/bin/claude"),
        PathBuf::from("/usr/bin/claude"),
    ];
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".local").join("bin").join("claude"));
    }
    paths
}

fn resolve_executable(name: &str, override_env: &str, fallbacks: &[PathBuf]) -> Result<PathBuf> {
    if let Ok(raw) = std::env::var(override_env)
        && !raw.trim().is_empty()
    {
        let path = PathBuf::from(raw);
        if is_executable(&path) {
            return Ok(path);
        }
        return Err(AiError::Llm(format!(
            "{} points to non-executable path: {}",
            override_env,
            path.display()
        )));
    }

    if let Some(path) = resolve_from_path(name) {
        return Ok(path);
    }

    for fallback in fallbacks {
        if is_executable(fallback) {
            return Ok(fallback.clone());
        }
    }

    Err(AiError::Llm(format!(
        "Failed to locate '{}' executable in PATH or fallback locations",
        name
    )))
}

fn resolve_from_path(name: &str) -> Option<PathBuf> {
    let path_value = std::env::var_os("PATH")?;
    for entry in std::env::split_paths(&path_value) {
        let candidate = entry.join(name);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn is_executable(path: &Path) -> bool {
    if !path.exists() || !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(path) {
            return metadata.permissions().mode() & 0o111 != 0;
        }
        false
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::ClaudeCodeClient;
    use std::sync::{Mutex, OnceLock};

    const CLAUDE_BIN_ENV: &str = "RESTFLOW_CLAUDE_BIN";

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    fn set_test_claude_bin() {
        let test_exe = std::env::current_exe().unwrap();
        unsafe {
            std::env::set_var(CLAUDE_BIN_ENV, test_exe);
        }
    }

    #[test]
    fn build_cli_command_includes_permission_bypass_flags() {
        let _lock = env_lock();
        set_test_claude_bin();
        let client = ClaudeCodeClient::new("token").with_model("claude-sonnet-4-5");
        let cmd = client.build_cli_command("hello").unwrap();

        let args: Vec<String> = cmd
            .as_std()
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect();

        assert!(
            args.windows(2)
                .any(|w| { w[0] == "--permission-mode" && w[1] == "bypassPermissions" })
        );
        assert!(
            args.iter()
                .any(|arg| arg == "--dangerously-skip-permissions")
        );
        unsafe {
            std::env::remove_var(CLAUDE_BIN_ENV);
        }
    }

    #[test]
    fn build_cli_command_removes_nested_session_env() {
        let _lock = env_lock();
        set_test_claude_bin();
        let client = ClaudeCodeClient::new("token").with_model("claude-sonnet-4-5");
        let cmd = client.build_cli_command("hello").unwrap();

        let envs: Vec<(String, Option<String>)> = cmd
            .as_std()
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().to_string(),
                    v.map(|x| x.to_string_lossy().to_string()),
                )
            })
            .collect();
        assert!(
            envs.iter().any(|(k, v)| k == "CLAUDECODE" && v.is_none()),
            "expected CLAUDECODE to be explicitly removed"
        );
        unsafe {
            std::env::remove_var(CLAUDE_BIN_ENV);
        }
    }
}
