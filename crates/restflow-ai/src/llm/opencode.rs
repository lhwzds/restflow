//! OpenCode CLI LLM provider

use async_trait::async_trait;
use serde_json::Value;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, info};

use crate::error::{AiError, Result};
use crate::llm::client::{
    CompletionRequest, CompletionResponse, FinishReason, LlmClient, Role, StreamResult,
};

const DEFAULT_MODEL: &str = "opencode";

/// OpenCode CLI client (auth via env vars)
pub struct OpenCodeClient {
    model: String,
    provider_env: Option<(String, String)>,
}

impl OpenCodeClient {
    /// Create a new OpenCode CLI client
    pub fn new() -> Self {
        Self {
            model: DEFAULT_MODEL.to_string(),
            provider_env: None,
        }
    }
}

impl Default for OpenCodeClient {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenCodeClient {
    /// Set the model to use
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Set provider env var for auth
    pub fn with_provider_env(mut self, env_var: impl Into<String>, value: impl Into<String>) -> Self {
        self.provider_env = Some((env_var.into(), value.into()));
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

    fn parse_json_output(output: &str) -> Result<String> {
        let value: Value = serde_json::from_str(output.trim()).map_err(|e| {
            AiError::Llm(format!("Failed to parse OpenCode output: {e}"))
        })?;

        value
            .get("response")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| AiError::Llm("OpenCode output missing 'response' field".to_string()))
    }
}

#[async_trait]
impl LlmClient for OpenCodeClient {
    fn provider(&self) -> &str {
        "opencode-cli"
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        info!("OpenCodeClient: executing via CLI");

        let prompt = Self::build_prompt(&request.messages);

        let mut cmd = Command::new("opencode");
        cmd.arg("-p")
            .arg(&prompt)
            .arg("-f")
            .arg("json")
            .arg("-q")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some((env_var, key)) = &self.provider_env {
            cmd.env(env_var, key);
        }

        let output = cmd.output().await.map_err(|e| {
            AiError::Llm(format!(
                "Failed to run opencode CLI: {}. Install with: go install github.com/opencode-ai/opencode@latest",
                e
            ))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AiError::Llm(format!("OpenCode CLI error: {}", stderr)));
        }

        let raw_output = String::from_utf8_lossy(&output.stdout).to_string();
        let content = Self::parse_json_output(&raw_output)?;
        debug!(content_len = content.len(), "OpenCode CLI response parsed");

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
                "Streaming not supported with OpenCode CLI".to_string()
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
    fn test_parse_json_output() {
        let output = r#"{"response": "Hello world"}"#;
        let content = OpenCodeClient::parse_json_output(output).unwrap();
        assert_eq!(content, "Hello world");
    }

    #[test]
    fn test_parse_json_output_missing_response() {
        let output = r#"{"error": "oops"}"#;
        let err = OpenCodeClient::parse_json_output(output).unwrap_err();
        assert!(err.to_string().contains("missing 'response'"));
    }

    #[test]
    fn test_parse_json_output_with_whitespace() {
        let output = " {\"response\": \"Hi\"} \n";
        let content = OpenCodeClient::parse_json_output(output).unwrap();
        assert_eq!(content, "Hi");
    }

    #[test]
    fn test_build_prompt() {
        let messages = vec![
            crate::llm::Message {
                role: Role::System,
                content: "sys".to_string(),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
            crate::llm::Message {
                role: Role::User,
                content: "Hello".to_string(),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
            crate::llm::Message {
                role: Role::Assistant,
                content: "World".to_string(),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
        ];

        let prompt = OpenCodeClient::build_prompt(&messages);
        assert_eq!(prompt, "Hello\n\nWorld");
    }

    #[test]
    fn test_opencode_provider_model() {
        let client = OpenCodeClient::new();
        assert_eq!(client.provider(), "opencode-cli");
        assert_eq!(client.model(), "opencode");
    }
}
