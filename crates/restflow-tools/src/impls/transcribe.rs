//! Audio transcription tool using OpenAI transcription models.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

use crate::Result;
use crate::audio::transcription::{
    SegmentedTranscriptionConfig, SegmentedTranscriptionOptions, TimestampGranularity,
    transcribe_audio_file,
};
use crate::{SecretResolver, Tool, ToolError, ToolOutput};

/// Configuration for transcribe tool security.
#[derive(Debug, Clone)]
pub struct TranscribeConfig {
    /// Allowed paths (security). Only files within these paths can be transcribed.
    pub allowed_paths: Vec<PathBuf>,
    /// Maximum direct upload size in bytes before chunking is required.
    pub max_file_size: usize,
    /// Allowed audio file extensions (lowercase).
    pub allowed_extensions: Vec<String>,
}

impl Default for TranscribeConfig {
    fn default() -> Self {
        let mut allowed = Vec::new();
        if let Some(media_dir) = ensure_default_media_dir() {
            allowed.push(media_dir);
        }
        Self {
            allowed_paths: allowed,
            max_file_size: 25 * 1024 * 1024,
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

fn ensure_default_media_dir() -> Option<PathBuf> {
    let restflow_dir = std::env::var("RESTFLOW_DIR")
        .ok()
        .filter(|dir| !dir.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".restflow")))?;
    let media_dir = restflow_dir.join("media");
    fs::create_dir_all(&media_dir).ok()?;
    Some(media_dir)
}

impl TranscribeConfig {
    pub fn for_workspace_root(workspace_root: impl Into<PathBuf>) -> Self {
        let workspace_root = workspace_root.into();
        let mut config = Self::default();
        config.allowed_paths.insert(0, workspace_root);
        config
    }
}

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
    include_segments: Option<bool>,
    chunk_long_audio: Option<bool>,
}

/// Tool for transcribing audio files with OpenAI transcription models.
pub struct TranscribeTool {
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
            secret_resolver,
            config,
        })
    }

    fn resolve_api_key(&self) -> Option<String> {
        (self.secret_resolver)("OPENAI_API_KEY")
    }

    fn validate_path(&self, file_path: &str) -> Result<()> {
        if self.config.allowed_paths.is_empty() {
            return Err(crate::ToolError::Tool(
                "This tool requires an explicit workspace root or allowed path.".to_string(),
            ));
        }

        let path = Path::new(file_path);

        if !is_path_allowed(path, &self.config.allowed_paths) {
            return Err(crate::ToolError::Tool(format!(
                "Path '{}' is not within allowed directories. Only files in specified directories can be transcribed.",
                file_path
            )));
        }

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

    fn build_transcription_config(&self) -> SegmentedTranscriptionConfig {
        SegmentedTranscriptionConfig {
            max_direct_upload_bytes: self.config.max_file_size as u64,
            ..SegmentedTranscriptionConfig::default()
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
                },
                "include_segments": {
                    "type": "boolean",
                    "description": "When true, return segment timestamps using whisper-1. Defaults to false."
                },
                "chunk_long_audio": {
                    "type": "boolean",
                    "description": "When true, automatically chunk oversized audio files before transcription. Defaults to false."
                }
            },
            "required": ["file_path"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: TranscribeInput = serde_json::from_value(input)?;

        let api_key = self.resolve_api_key().ok_or_else(|| {
            ToolError::Tool(
                "Missing OPENAI_API_KEY. Set it via manage_secrets tool with {operation: 'set', key: 'OPENAI_API_KEY', value: '...'}.".to_string(),
            )
        })?;

        self.validate_path(&params.file_path)?;

        let options = SegmentedTranscriptionOptions {
            model: params
                .model
                .clone()
                .unwrap_or_else(|| "whisper-1".to_string()),
            language: params.language.clone(),
            timestamp_granularity: if params.include_segments.unwrap_or(false) {
                TimestampGranularity::Segment
            } else {
                TimestampGranularity::None
            },
            chunk_long_audio: params.chunk_long_audio.unwrap_or(false),
        };

        let output = transcribe_audio_file(
            &api_key,
            &params.file_path,
            &options,
            &self.build_transcription_config(),
        )
        .await
        .map_err(|error| ToolError::Tool(error.to_string()))?;

        Ok(ToolOutput::success(json!({
            "text": output.text,
            "file_path": params.file_path,
            "model": output.model,
            "segments": output.segments,
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
        assert!(schema["properties"].get("include_segments").is_some());
        assert!(schema["properties"].get("chunk_long_audio").is_some());
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

        assert!(is_path_allowed(
            Path::new("/home/user/workspace/audio.mp3"),
            &allowed
        ));
        assert!(is_path_allowed(
            Path::new("/home/user/workspace/subfolder/test.wav"),
            &allowed
        ));

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

        let result = tool.validate_path("/tmp/test.txt");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("extension") || err.to_string().contains("not allowed"),
            "Error: {}",
            err
        );
    }

    #[test]
    fn test_validate_path_requires_allowed_root() {
        let resolver: SecretResolver = Arc::new(|_| None);
        let config = TranscribeConfig {
            allowed_paths: vec![],
            max_file_size: 25 * 1024 * 1024,
            allowed_extensions: vec!["mp3".to_string()],
        };
        let tool = TranscribeTool::with_config(resolver, config).unwrap();

        let result = tool.validate_path("/tmp/test.mp3");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("workspace root or allowed path")
        );
    }
}
