//! Transcribe tool for converting audio to text using OpenAI Whisper.

use async_trait::async_trait;
use reqwest::{Client, multipart};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::Path;

use crate::error::{AiError, Result};
use crate::http_client::build_http_client;
use crate::tools::traits::{SecretResolver, Tool, ToolOutput};

#[derive(Debug, Deserialize)]
struct TranscribeInput {
    file_path: String,
    language: Option<String>,
    model: Option<String>,
}

/// Tool for transcribing audio files using OpenAI Whisper.
pub struct TranscribeTool {
    client: Client,
    secret_resolver: SecretResolver,
}

impl TranscribeTool {
    pub fn new(secret_resolver: SecretResolver) -> Self {
        Self {
            client: build_http_client(),
            secret_resolver,
        }
    }

    pub fn with_client(mut self, client: Client) -> Self {
        self.client = client;
        self
    }
}

#[async_trait]
impl Tool for TranscribeTool {
    fn name(&self) -> &str {
        "transcribe"
    }

    fn description(&self) -> &str {
        "Transcribe audio files to text using OpenAI Whisper."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the audio file on disk"
                },
                "language": {
                    "type": "string",
                    "description": "Optional language code for transcription"
                },
                "model": {
                    "type": "string",
                    "description": "Optional Whisper model (default: whisper-1)"
                }
            },
            "required": ["file_path"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: TranscribeInput = serde_json::from_value(input)?;
        let Some(api_key) = (self.secret_resolver)("OPENAI_API_KEY") else {
            return Ok(ToolOutput::error("Missing OPENAI_API_KEY"));
        };

        let model = params.model.unwrap_or_else(|| "whisper-1".to_string());
        let file_bytes = tokio::fs::read(&params.file_path).await?;
        let file_name = Path::new(&params.file_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("audio");

        let part = multipart::Part::bytes(file_bytes).file_name(file_name.to_string());
        let mut form = multipart::Form::new()
            .text("model", model.clone())
            .part("file", part);

        if let Some(language) = params.language.as_ref() {
            form = form.text("language", language.clone());
        }

        let response = self
            .client
            .post("https://api.openai.com/v1/audio/transcriptions")
            .bearer_auth(api_key)
            .multipart(form)
            .send()
            .await;

        let response = match response {
            Ok(response) => response,
            Err(err) => return Ok(ToolOutput::error(err.to_string())),
        };

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Ok(ToolOutput::error(format!(
                "OpenAI API error ({}): {}",
                status.as_u16(),
                body
            )));
        }

        let parsed: Value = serde_json::from_str(&body).map_err(AiError::from)?;
        let text = parsed
            .get("text")
            .and_then(|value| value.as_str())
            .unwrap_or_default();

        Ok(ToolOutput::success(json!({
            "text": text,
            "file_path": params.file_path,
            "model": model
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_transcribe_schema() {
        let tool = TranscribeTool::new(Arc::new(|_| None));
        assert_eq!(tool.name(), "transcribe");
        assert!(!tool.description().is_empty());

        let schema = tool.parameters_schema();
        assert!(schema.get("properties").is_some());
    }

    #[tokio::test]
    async fn test_transcribe_requires_api_key() {
        let tool = TranscribeTool::new(Arc::new(|_| None));
        let output = tool
            .execute(json!({
                "file_path": "/tmp/does-not-exist.ogg"
            }))
            .await
            .unwrap();

        assert!(!output.success);
        assert!(output.error.unwrap_or_default().contains("OPENAI_API_KEY"));
    }
}
