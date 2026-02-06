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

/// Codex CLI client (auth via ~/.codex/auth.json)
pub struct CodexClient {
    model: String,
}

impl CodexClient {
    /// Create a new Codex CLI client
    pub fn new() -> Self {
        Self {
            model: DEFAULT_MODEL.to_string(),
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

            let value: Value = serde_json::from_str(trimmed).map_err(|e| {
                AiError::Llm(format!("Failed to parse Codex CLI JSONL line: {e}"))
            })?;

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

        let output = Command::new("codex")
            .arg("exec")
            .arg("--json")
            .arg("--color")
            .arg("never")
            .arg("--full-auto")
            .arg("--skip-git-repo-check")
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
}
