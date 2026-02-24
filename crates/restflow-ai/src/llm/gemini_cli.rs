//! Gemini CLI LLM provider

use async_trait::async_trait;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, info};

use crate::error::Result;
use crate::llm::client::{
    CompletionRequest, CompletionResponse, FinishReason, LlmClient, StreamResult,
};

use super::cli_utils;

const DEFAULT_MODEL: &str = "gemini-2.5-pro";

/// Gemini CLI client (auth via OAuth in ~/.gemini or GEMINI_API_KEY)
pub struct GeminiCliClient {
    model: String,
    api_key: Option<String>,
}

impl GeminiCliClient {
    /// Create a new Gemini CLI client
    pub fn new() -> Self {
        Self {
            model: DEFAULT_MODEL.to_string(),
            api_key: None,
        }
    }

    /// Set the model to use
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Inject GEMINI_API_KEY for CLI execution
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    fn parse_json_output(output: &str) -> Result<String> {
        cli_utils::parse_json_response(output, "Gemini")
    }
}

impl Default for GeminiCliClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmClient for GeminiCliClient {
    fn provider(&self) -> &str {
        "gemini-cli"
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        info!("GeminiCliClient: executing via CLI");

        let prompt = cli_utils::build_prompt(&request.messages);
        let mut cmd = Command::new("gemini");
        cmd.arg("-p")
            .arg(&prompt)
            .arg("-o")
            .arg("json")
            .arg("-y")
            .arg("-m")
            .arg(&self.model)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(api_key) = &self.api_key {
            cmd.env("GEMINI_API_KEY", api_key);
        }

        let raw_output = cli_utils::execute_cli_command(
            cmd,
            "Gemini",
            "Install with: npm install -g @google/gemini-cli",
        )
        .await?;
        let content = Self::parse_json_output(&raw_output)?;
        debug!(content_len = content.len(), "Gemini CLI response parsed");

        Ok(CompletionResponse {
            content: Some(content),
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            usage: None,
        })
    }

    fn complete_stream(&self, _request: CompletionRequest) -> StreamResult {
        cli_utils::unsupported_stream("Gemini CLI")
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
        let output = r#"{"response":"Hello from Gemini"}"#;
        let content = GeminiCliClient::parse_json_output(output).unwrap();
        assert_eq!(content, "Hello from Gemini");
    }

    #[test]
    fn test_parse_json_output_missing_response() {
        let output = r#"{"error":"auth failed"}"#;
        assert!(GeminiCliClient::parse_json_output(output).is_err());
    }

    #[test]
    fn test_parse_json_output_whitespace() {
        let output = " {\"response\": \"Hi\"} \n";
        let content = GeminiCliClient::parse_json_output(output).unwrap();
        assert_eq!(content, "Hi");
    }

    #[test]
    fn test_gemini_cli_provider_model() {
        let client = GeminiCliClient::new();
        assert_eq!(client.provider(), "gemini-cli");
        assert_eq!(client.model(), "gemini-2.5-pro");
    }

    #[test]
    fn test_gemini_cli_with_model() {
        let client = GeminiCliClient::new().with_model("gemini-2.5-flash");
        assert_eq!(client.model(), "gemini-2.5-flash");
    }
}
