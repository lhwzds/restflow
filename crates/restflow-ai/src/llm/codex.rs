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

/// Codex CLI client (auth via ~/.codex/auth.json)
pub struct CodexClient {
    model: String,
}

impl CodexClient {
    /// Create a new Codex CLI client
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
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

    fn parse_jsonl_output(output: &str) -> Result<(String, Option<String>)> {
        let mut content_parts: Vec<String> = Vec::new();
        let mut thread_id: Option<String> = None;

        for line in output.lines().filter(|line| !line.trim().is_empty()) {
            let value: Value = serde_json::from_str(line)?;

            if let Some(error) = extract_error(&value) {
                return Err(AiError::Llm(error));
            }

            if let Some(id) = find_thread_id(&value) {
                thread_id = Some(id);
            }

            collect_texts(&value, &mut content_parts);
        }

        if content_parts.is_empty() {
            return Err(AiError::InvalidFormat(
                "Codex CLI returned no content".to_string(),
            ));
        }

        Ok((content_parts.join(""), thread_id))
    }
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
        info!(model = %self.model, "CodexClient: executing via CLI");

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

        let stdout = String::from_utf8_lossy(&output.stdout);
        let (content, thread_id) = Self::parse_jsonl_output(stdout.trim())?;
        debug!(
            thread_id = %thread_id.as_deref().unwrap_or("unknown"),
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

fn extract_error(value: &Value) -> Option<String> {
    if let Some(message) = value.get("error").and_then(|v| v.as_str()) {
        return Some(message.to_string());
    }

    if let Some(message) = value.get("error_message").and_then(|v| v.as_str()) {
        return Some(message.to_string());
    }

    if let Some(error) = value.get("error")
        && let Some(message) = error.get("message").and_then(|v| v.as_str())
    {
        return Some(message.to_string());
    }

    if value.get("type").and_then(|v| v.as_str()) == Some("error")
        && let Some(message) = value.get("message").and_then(|v| v.as_str())
    {
        return Some(message.to_string());
    }

    None
}

fn find_thread_id(value: &Value) -> Option<String> {
    if let Some(id) = value.get("thread_id").and_then(|v| v.as_str()) {
        return Some(id.to_string());
    }

    if let Some(id) = value.get("threadId").and_then(|v| v.as_str()) {
        return Some(id.to_string());
    }

    None
}

fn collect_texts(value: &Value, output: &mut Vec<String>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_texts(item, output);
            }
        }
        Value::Object(map) => {
            if map.get("type").and_then(|v| v.as_str()) == Some("output_text")
                && let Some(text) = map.get("text").and_then(|v| v.as_str())
            {
                output.push(text.to_string());
            }

            if let Some(text) = map.get("content").and_then(|v| v.as_str()) {
                output.push(text.to_string());
            }

            for value in map.values() {
                collect_texts(value, output);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_jsonl_output_with_thread_id() {
        let payload = r#"{"type":"response","thread_id":"thread_123","output":[{"type":"message","content":[{"type":"output_text","text":"Hello"}]}]}"#;
        let (content, thread_id) = CodexClient::parse_jsonl_output(payload).unwrap();
        assert_eq!(content, "Hello");
        assert_eq!(thread_id.as_deref(), Some("thread_123"));
    }

    #[test]
    fn parses_jsonl_output_with_content_string() {
        let payload = r#"{"content":"Hi from Codex"}"#;
        let (content, thread_id) = CodexClient::parse_jsonl_output(payload).unwrap();
        assert_eq!(content, "Hi from Codex");
        assert!(thread_id.is_none());
    }

    #[test]
    fn returns_error_from_payload() {
        let payload = r#"{"type":"error","message":"Missing auth"}"#;
        let err = CodexClient::parse_jsonl_output(payload).unwrap_err();
        match err {
            AiError::Llm(message) => assert!(message.contains("Missing auth")),
            _ => panic!("Expected LLM error"),
        }
    }
}
