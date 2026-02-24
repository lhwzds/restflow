//! Audio transcription tool using OpenAI Whisper API.

use async_trait::async_trait;
use reqwest::Client;
use reqwest::StatusCode;
use reqwest::multipart::{Form, Part};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tokio::fs;

use crate::Result;
use crate::http_client::build_http_client;
use crate::{SecretResolver, Tool, ToolError, ToolOutput};

/// Configuration for transcribe tool security.
#[derive(Debug, Clone)]
pub struct TranscribeConfig {
    /// Allowed paths (security). Only files within these paths can be transcribed.
    pub allowed_paths: Vec<PathBuf>,
    /// Maximum file size in bytes (default 25MB for Whisper API).
    pub max_file_size: usize,
    /// Allowed audio file extensions (lowercase).
    pub allowed_extensions: Vec<String>,
}

impl Default for TranscribeConfig {
    fn default() -> Self {
        let mut allowed = vec![std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))];
        // Always allow the Telegram media download directory
        allowed.push(PathBuf::from("/tmp/restflow-media"));
        Self {
            allowed_paths: allowed,
            max_file_size: 25 * 1024 * 1024, // 25MB
            allowed_extensions: vec![
                "mp3".to_string(),
                "mp4".to_string(),
                "mpeg".to_string(),
                "mpga".to_string(),
                "m4a".to_string(),
                "wav".to_string(),
                "webm".to_string(),
                "ogg".to_string(),
                "oga".to_string(),
            ],
        }
    }
}

/// Check if a path is within any of the allowed directories.
fn is_path_allowed(path: &Path, allowed_paths: &[PathBuf]) -> bool {
    allowed_paths
        .iter()
        .any(|allowed| path.starts_with(allowed))
}

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
    config: TranscribeConfig,
}

impl TranscribeTool {
    pub fn new(secret_resolver: SecretResolver) -> std::result::Result<Self, reqwest::Error> {
        Self::with_config(secret_resolver, TranscribeConfig::default())
    }

    pub fn with_config(
        secret_resolver: SecretResolver,
        config: TranscribeConfig,
    ) -> std::result::Result<Self, reqwest::Error> {
        Ok(Self {
            client: build_http_client()?,
            secret_resolver,
            config,
        })
    }

    fn resolve_api_key(&self) -> Option<String> {
        (self.secret_resolver)("OPENAI_API_KEY")
    }

    fn validate_path(&self, file_path: &str) -> Result<()> {
        let path = Path::new(file_path);

        // Check if path is within allowed directories
        if !is_path_allowed(path, &self.config.allowed_paths) {
            return Err(crate::ToolError::Tool(format!(
                "Path '{}' is not within allowed directories. Only files in specified directories can be transcribed.",
                file_path
            )));
        }

        // Check file extension
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());

        if let Some(ext) = extension
            && !self.config.allowed_extensions.contains(&ext)
        {
            return Err(crate::ToolError::Tool(format!(
                "File extension '{}' is not allowed. Only audio files are permitted.",
                ext
            )));
        }

        Ok(())
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
                    "description": "Local path to an audio file (ogg, oga, wav, mp3, m4a, flac, webm)."
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
                ToolError::Tool(
                    "Missing OPENAI_API_KEY. Set it via manage_secrets tool with {operation: 'set', key: 'OPENAI_API_KEY', value: '...'}.".to_string(),
                )
            })?;

        // Validate path and extension before reading
        self.validate_path(&params.file_path)?;

        let audio_bytes = fs::read(&params.file_path)
            .await
            .map_err(|e| {
                ToolError::Tool(format!(
                    "Cannot read audio file '{}': {}. Verify the file exists. Supported formats: mp3, mp4, mpeg, mpga, m4a, wav, webm.",
                    params.file_path, e
                ))
            })?;

        // Validate file size before upload
        if audio_bytes.len() > self.config.max_file_size {
            return Err(crate::ToolError::Tool(format!(
                "File too large ({} bytes). Maximum size is {} bytes.",
                audio_bytes.len(),
                self.config.max_file_size
            )));
        }

        let filename = std::path::Path::new(&params.file_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("audio");

        let mut form = Form::new()
            .part(
                "file",
                Part::bytes(audio_bytes)
                    .file_name(filename.to_string())
                    .mime_str("application/octet-stream")
                    .map_err(|e| crate::ToolError::Other(e.into()))?,
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
                ToolError::Tool(format!(
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
                ToolError::Tool(
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
        let tool = TranscribeTool::new(resolver).unwrap();
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

    #[test]
    fn test_transcribe_config_default_values() {
        let config = TranscribeConfig::default();
        assert_eq!(config.max_file_size, 25 * 1024 * 1024);
        assert!(config.allowed_extensions.contains(&"mp3".to_string()));
        assert!(config.allowed_extensions.contains(&"wav".to_string()));
    }

    #[test]
    fn test_is_path_allowed() {
        let allowed = vec![PathBuf::from("/home/user/workspace")];

        // Path within allowed directory
        assert!(is_path_allowed(
            Path::new("/home/user/workspace/audio.mp3"),
            &allowed
        ));
        assert!(is_path_allowed(
            Path::new("/home/user/workspace/subfolder/test.wav"),
            &allowed
        ));

        // Path outside allowed directory
        assert!(!is_path_allowed(Path::new("/etc/passwd"), &allowed));
        assert!(!is_path_allowed(
            Path::new("/home/user/../etc/passwd"),
            &allowed
        ));
    }

    #[test]
    fn test_transcribe_tool_with_config() {
        let resolver: SecretResolver = Arc::new(|_| None);
        let config = TranscribeConfig {
            allowed_paths: vec![PathBuf::from("/tmp")],
            max_file_size: 1024,
            allowed_extensions: vec!["mp3".to_string()],
        };
        let tool = TranscribeTool::with_config(resolver, config).unwrap();
        let schema = tool.parameters_schema();
        assert_eq!(tool.name(), "transcribe");
        assert!(schema.get("properties").is_some());
    }

    #[test]
    fn test_validate_path_rejects_path_traversal() {
        let resolver: SecretResolver = Arc::new(|_| None);
        let config = TranscribeConfig {
            allowed_paths: vec![PathBuf::from("/home/user/workspace")],
            max_file_size: 25 * 1024 * 1024,
            allowed_extensions: vec!["mp3".to_string()],
        };
        let tool = TranscribeTool::with_config(resolver, config).unwrap();

        let result = tool.validate_path("/etc/passwd");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("not within allowed"),
            "Error: {}",
            err
        );
    }

    #[test]
    fn test_validate_path_rejects_non_audio_extension() {
        let resolver: SecretResolver = Arc::new(|_| None);
        let config = TranscribeConfig {
            allowed_paths: vec![PathBuf::from("/tmp")],
            max_file_size: 25 * 1024 * 1024,
            allowed_extensions: vec!["mp3".to_string()],
        };
        let tool = TranscribeTool::with_config(resolver, config).unwrap();

        // Test with .txt extension
        let result = tool.validate_path("/tmp/test.txt");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("extension") || err.to_string().contains("not allowed"),
            "Error: {}",
            err
        );
    }
}
