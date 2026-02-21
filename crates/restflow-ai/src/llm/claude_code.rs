//! Claude Code CLI LLM provider

use async_trait::async_trait;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, info};

use super::cli_utils;

use crate::error::{AiError, Result};
use crate::llm::client::{
    CompletionRequest, CompletionResponse, FinishReason, LlmClient, StreamResult,
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

    fn build_cli_command(&self, prompt: &str) -> Result<Command> {
        let executable =
            cli_utils::resolve_executable("claude", "RESTFLOW_CLAUDE_BIN", &cli_utils::standard_fallbacks("claude"))?;
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

        let prompt = cli_utils::build_prompt(&request.messages);

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
        cli_utils::unsupported_stream("Claude Code CLI")
    }

    fn supports_streaming(&self) -> bool {
        false
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
