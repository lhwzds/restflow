//! Audio transcription tool using OpenAI Whisper API.

use async_trait::async_trait;
use reqwest::Client;
use reqwest::StatusCode;
use reqwest::multipart::{Form, Part};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::fs;

use crate::error::{AiError, Result};
use crate::http_client::build_http_client;
use crate::tools::traits::{SecretResolver, Tool, ToolOutput};

#[derive(Debug, Deserialize)]
struct TranscribeInput {
    file_path: String,
    language: Option<String>,
    model: Option<String>,
}

/// Tool for transcribing audio files with OpenAI Whisper API.
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

    fn resolve_api_key(&self) -> Option<String> {
        (self.secret_resolver)("OPENAI_API_KEY")
    }

    fn format_api_error(status: StatusCode, error_text: &str) -> String {
        match status {
            StatusCode::UNAUTHORIZED => {
                "Invalid API key. Check OPENAI_API_KEY in manage_secrets.".to_string()
            }
            StatusCode::TOO_MANY_REQUESTS => "Rate limited, retry later.".to_string(),
            _ => {
                if error_text.trim().is_empty() {
                    format!("Transcription API returned HTTP {}.", status)
                } else {
                    error_text.to_string()
                }
            }
        }
    }
}

#[async_trait]
impl Tool for TranscribeTool {
    fn name(&self) -> &str {
        "transcribe"
    }

    fn description(&self) -> &str {
        "Convert a local audio file to text using OpenAI transcription models."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Local path to an audio file (ogg, wav, mp3, m4a, flac, webm)."
                },
                "language": {
                    "type": "string",
                    "description": "Optional language hint (e.g., 'en')."
                },
                "model": {
                    "type": "string",
                    "description": "Optional model name. Defaults to whisper-1."
                }
            },
            "required": ["file_path"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: TranscribeInput = serde_json::from_value(input)?;

        let api_key = self
            .resolve_api_key()
            .ok_or_else(|| {
                AiError::Tool(
                    "Missing OPENAI_API_KEY. Set it via manage_secrets tool with {operation: 'set', key: 'OPENAI_API_KEY', value: '...'}.".to_string(),
                )
            })?;

        let audio_bytes = fs::read(&params.file_path)
            .await
            .map_err(|e| {
                AiError::Tool(format!(
                    "Cannot read audio file '{}': {}. Verify the file exists. Supported formats: mp3, mp4, mpeg, mpga, m4a, wav, webm.",
                    params.file_path, e
                ))
            })?;

        let filename = std::path::Path::new(&params.file_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("audio");

        let mut form = Form::new()
            .part(
                "file",
                Part::bytes(audio_bytes)
                    .file_name(filename.to_string())
                    .mime_str("application/octet-stream")?,
            )
            .text(
                "model",
                params
                    .model
                    .clone()
                    .unwrap_or_else(|| "whisper-1".to_string()),
            );

        if let Some(language) = params.language.clone() {
            form = form.text("language", language);
        }

        let response = self
            .client
            .post("https://api.openai.com/v1/audio/transcriptions")
            .bearer_auth(api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|e| {
                AiError::Tool(format!(
                    "Transcription API request failed: {}. This may be a network issue or rate limit. Retry after a brief wait.",
                    e
                ))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Ok(ToolOutput::error(format!(
                "Transcription failed: {}",
                Self::format_api_error(status, &error_text)
            )));
        }

        let body: Value = response
            .json()
            .await
            .map_err(|_| {
                AiError::Tool(
                    "Transcription API returned an unexpected response format. This may indicate an API version mismatch. Retry or report the issue.".to_string(),
                )
            })?;

        let text = body
            .get("text")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();

        Ok(ToolOutput::success(json!({
            "text": text,
            "file_path": params.file_path,
            "model": params.model.unwrap_or_else(|| "whisper-1".to_string())
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_transcribe_schema() {
        let resolver: SecretResolver = Arc::new(|_| None);
        let tool = TranscribeTool::new(resolver);
        let schema = tool.parameters_schema();
        assert_eq!(tool.name(), "transcribe");
        assert!(schema.get("properties").is_some());
    }

    #[test]
    fn test_transcribe_api_error_mapping() {
        let unauthorized = TranscribeTool::format_api_error(StatusCode::UNAUTHORIZED, "ignored");
        assert!(unauthorized.contains("Invalid API key"));

        let rate_limited =
            TranscribeTool::format_api_error(StatusCode::TOO_MANY_REQUESTS, "ignored");
        assert!(rate_limited.contains("Rate limited"));

        let passthrough =
            TranscribeTool::format_api_error(StatusCode::BAD_REQUEST, "custom message");
        assert_eq!(passthrough, "custom message");
    }
}
