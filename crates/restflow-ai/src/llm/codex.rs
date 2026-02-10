//! Codex CLI LLM provider

use async_trait::async_trait;
use serde_json::Value;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, info};

use crate::error::{AiError, Result};
use crate::llm::client::{
    CompletionRequest, CompletionResponse, FinishReason, LlmClient, Role, StreamResult,
};

const DEFAULT_MODEL: &str = "gpt-5.3-codex";
const DEFAULT_REASONING_EFFORT: &str = "medium";
const DEFAULT_EXECUTION_MODE: &str = "safe";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExecutionMode {
    Safe,
    Bypass,
}

impl ExecutionMode {
    fn from_str(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "safe" => Some(Self::Safe),
            "bypass" => Some(Self::Bypass),
            _ => None,
        }
    }
}

/// Codex CLI client (auth via ~/.codex/auth.json)
pub struct CodexClient {
    model: String,
    reasoning_effort: Option<String>,
    execution_mode: ExecutionMode,
}

impl CodexClient {
    /// Create a new Codex CLI client
    pub fn new() -> Self {
        Self {
            model: DEFAULT_MODEL.to_string(),
            reasoning_effort: Some(DEFAULT_REASONING_EFFORT.to_string()),
            execution_mode: ExecutionMode::from_str(DEFAULT_EXECUTION_MODE)
                .unwrap_or(ExecutionMode::Safe),
        }
    }
}

impl Default for CodexClient {
    fn default() -> Self {
        Self::new()
    }
}

impl CodexClient {
    /// Set the model to use
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Set reasoning effort override for Codex CLI.
    pub fn with_reasoning_effort(mut self, effort: impl Into<String>) -> Self {
        let effort = effort.into();
        let normalized = effort.trim();
        if !normalized.is_empty() {
            self.reasoning_effort = Some(normalized.to_string());
        }
        self
    }

    /// Set execution mode override for Codex CLI.
    ///
    /// Supported values:
    /// - `safe`: use `--full-auto`
    /// - `bypass`: use `--dangerously-bypass-approvals-and-sandbox`
    pub fn with_execution_mode(mut self, mode: impl AsRef<str>) -> Self {
        if let Some(parsed) = ExecutionMode::from_str(mode.as_ref()) {
            self.execution_mode = parsed;
        }
        self
    }

    fn build_cli_args(&self, prompt: &str) -> Vec<String> {
        let mut args = vec![
            "exec".to_string(),
            "--json".to_string(),
            "--color".to_string(),
            "never".to_string(),
            "--skip-git-repo-check".to_string(),
        ];

        match self.execution_mode {
            ExecutionMode::Safe => args.push("--full-auto".to_string()),
            ExecutionMode::Bypass => {
                args.push("--dangerously-bypass-approvals-and-sandbox".to_string())
            }
        }

        if let Some(effort) = self.reasoning_effort.as_ref() {
            let quoted_effort =
                serde_json::to_string(effort).unwrap_or_else(|_| "\"medium\"".to_string());
            args.push("-c".to_string());
            args.push(format!("model_reasoning_effort={quoted_effort}"));
        }

        args.push("--model".to_string());
        args.push(self.model.clone());
        // Ensure prompt content that starts with '-' is not parsed as CLI flags.
        args.push("--".to_string());
        args.push(prompt.to_string());
        args
    }

    fn build_prompt(messages: &[crate::llm::Message]) -> String {
        messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    fn parse_jsonl_output(output: &str) -> Result<(String, Option<String>)> {
        let mut content = String::new();
        let mut thread_id = None;

        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let value: Value = serde_json::from_str(trimmed)
                .map_err(|e| AiError::Llm(format!("Failed to parse Codex CLI JSONL line: {e}")))?;

            if thread_id.is_none()
                && let Some(id) = value.get("thread_id").and_then(|v| v.as_str())
            {
                thread_id = Some(id.to_string());
            }

            if let Some(err) = extract_error(&value) {
                return Err(AiError::Llm(format!("Codex CLI error: {err}")));
            }

            if let Some(text) = extract_text(&value) {
                content.push_str(text);
            }
        }

        if content.trim().is_empty() {
            return Err(AiError::Llm("Codex CLI returned empty output".to_string()));
        }

        Ok((content, thread_id))
    }
}

fn extract_error(value: &Value) -> Option<String> {
    value
        .get("error")
        .and_then(|v| v.as_str().map(|err| err.to_string()))
        .or_else(|| {
            value
                .get("error")
                .and_then(|v| v.get("message"))
                .and_then(|v| v.as_str())
                .map(|err| err.to_string())
        })
}

fn extract_text(value: &Value) -> Option<&str> {
    if let Some(item) = value.get("item") {
        let item_type = item.get("type").and_then(|v| v.as_str());
        if matches!(item_type, Some("agent_message" | "assistant_message")) {
            return item
                .get("text")
                .and_then(|v| v.as_str())
                .or_else(|| item.get("content").and_then(|v| v.as_str()));
        }
    }

    value
        .get("content")
        .and_then(|v| v.as_str())
        .or_else(|| value.get("text").and_then(|v| v.as_str()))
        .or_else(|| value.get("delta").and_then(|v| v.as_str()))
        .or_else(|| value.pointer("/message/content").and_then(|v| v.as_str()))
        .or_else(|| value.pointer("/data/content").and_then(|v| v.as_str()))
}

#[async_trait]
impl LlmClient for CodexClient {
    fn provider(&self) -> &str {
        "codex-cli"
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        info!("CodexClient: executing via CLI");

        let prompt = Self::build_prompt(&request.messages);
        let args = self.build_cli_args(&prompt);

        let output = Command::new("codex")
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| {
                AiError::Llm(format!(
                    "Failed to run codex CLI: {}. Install with: npm install -g @openai/codex",
                    e
                ))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AiError::Llm(format!("Codex CLI error: {}", stderr)));
        }

        let raw_output = String::from_utf8_lossy(&output.stdout).to_string();
        let (content, thread_id) = Self::parse_jsonl_output(&raw_output)?;
        debug!(
            content_len = content.len(),
            thread_id = thread_id.as_deref().unwrap_or("n/a"),
            "Codex CLI response parsed"
        );

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
                "Streaming not supported with Codex CLI".to_string()
            ));
        })
    }

    fn supports_streaming(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_jsonl_output() {
        let output = r#"{"type":"response.output_text.delta","delta":"Hello "}
{"type":"response.output_text.delta","delta":"world"}
{"type":"response.completed","thread_id":"thread_123"}
"#;

        let (content, thread_id) = CodexClient::parse_jsonl_output(output).unwrap();
        assert_eq!(content, "Hello world");
        assert_eq!(thread_id, Some("thread_123".to_string()));
    }

    #[test]
    fn test_parse_jsonl_output_with_message_content() {
        let output = r#"{"message":{"content":"Hi"}}"#;
        let (content, thread_id) = CodexClient::parse_jsonl_output(output).unwrap();
        assert_eq!(content, "Hi");
        assert!(thread_id.is_none());
    }

    #[test]
    fn test_parse_jsonl_output_error() {
        let output = r#"{"error":"invalid"}"#;
        let err = CodexClient::parse_jsonl_output(output).unwrap_err();
        assert!(err.to_string().contains("Codex CLI error"));
    }

    #[test]
    fn test_parse_jsonl_output_with_item_text_ignores_reasoning() {
        let output = r#"{"type":"thread.started","thread_id":"thread_abc"}
{"type":"item.completed","item":{"id":"item_0","type":"reasoning","text":"Thinking..."}}
{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"Hello from Codex"}}
{"type":"turn.completed"}
"#;

        let (content, thread_id) = CodexClient::parse_jsonl_output(output).unwrap();
        assert_eq!(content, "Hello from Codex");
        assert_eq!(thread_id, Some("thread_abc".to_string()));
    }

    #[test]
    fn test_build_cli_args_defaults_to_medium_reasoning_effort() {
        let client = CodexClient::new().with_model("gpt-5.3-codex");
        let args = client.build_cli_args("hello");

        assert!(
            args.windows(2)
                .any(|pair| { pair[0] == "-c" && pair[1] == "model_reasoning_effort=\"medium\"" })
        );
    }

    #[test]
    fn test_build_cli_args_with_reasoning_effort() {
        let client = CodexClient::new()
            .with_model("gpt-5.3-codex")
            .with_reasoning_effort("xhigh");
        let args = client.build_cli_args("hello");

        assert!(
            args.windows(2)
                .any(|pair| { pair[0] == "-c" && pair[1] == "model_reasoning_effort=\"xhigh\"" })
        );
    }

    #[test]
    fn test_build_cli_args_defaults_to_safe_execution_mode() {
        let client = CodexClient::new().with_model("gpt-5.3-codex");
        let args = client.build_cli_args("hello");
        assert!(args.iter().any(|arg| arg == "--full-auto"));
        assert!(
            !args
                .iter()
                .any(|arg| arg == "--dangerously-bypass-approvals-and-sandbox")
        );
    }

    #[test]
    fn test_build_cli_args_with_bypass_execution_mode() {
        let client = CodexClient::new()
            .with_model("gpt-5.3-codex")
            .with_execution_mode("bypass");
        let args = client.build_cli_args("hello");
        assert!(
            args.iter()
                .any(|arg| arg == "--dangerously-bypass-approvals-and-sandbox")
        );
        assert!(!args.iter().any(|arg| arg == "--full-auto"));
    }

    #[test]
    fn test_build_cli_args_inserts_double_dash_before_prompt() {
        let client = CodexClient::new().with_model("gpt-5.3-codex");
        let prompt = "- starts-with-dash";
        let args = client.build_cli_args(prompt);

        let separator_index = args
            .iter()
            .position(|arg| arg == "--")
            .expect("args should include option separator");
        assert_eq!(args.get(separator_index + 1), Some(&prompt.to_string()));
    }
}
