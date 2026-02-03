//! Claude Code CLI LLM provider

use async_trait::async_trait;
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

        let output = Command::new("claude")
            .env("CLAUDE_CODE_OAUTH_TOKEN", &self.oauth_token)
            .arg("--print")
            .arg("--model")
            .arg(&self.model)
            .arg(&prompt)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| {
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
