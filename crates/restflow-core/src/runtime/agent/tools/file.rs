//! File system tool for reading and writing files.

use crate::runtime::agent::tools::ToolResult;
use async_trait::async_trait;
use restflow_ai::error::{AiError, Result};
use restflow_ai::tools::Tool;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

/// Configuration for file tool.
#[derive(Debug, Clone)]
pub struct FileConfig {
    /// Allowed paths (security).
    pub allowed_paths: Vec<PathBuf>,
    /// Whether write operations are allowed.
    pub allow_write: bool,
    /// Maximum bytes allowed for a single file read.
    pub max_read_bytes: usize,
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            allowed_paths: vec![std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))],
            allow_write: true,
            max_read_bytes: 1_000_000,
        }
    }
}

pub struct FileTool {
    config: FileConfig,
}

impl FileTool {
    pub fn new(config: FileConfig) -> Self {
        Self { config }
    }

    fn is_path_allowed(&self, path: &Path) -> bool {
        self.config
            .allowed_paths
            .iter()
            .any(|allowed| path.starts_with(allowed))
    }
}

#[async_trait]
impl Tool for FileTool {
    fn name(&self) -> &str {
        "file"
    }

    fn description(&self) -> &str {
        "Read or write files. Supports read and write actions with security checks."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["read", "write"],
                    "description": "The file operation to perform"
                },
                "path": {
                    "type": "string",
                    "description": "Path to the file"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write (for write action)"
                }
            },
            "required": ["action", "path"]
        })
    }

    fn supports_parallel_for(&self, input: &Value) -> bool {
        let action = input.get("action").and_then(|value| value.as_str());
        !matches!(action, Some("write"))
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AiError::Tool("Missing 'action' argument".to_string()))?;

        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AiError::Tool("Missing 'path' argument".to_string()))?;

        let path = Path::new(path);

        if !self.is_path_allowed(path) {
            return Ok(ToolResult::error("Path not allowed"));
        }

        match action {
            "read" => {
                let metadata = fs::metadata(path)
                    .map_err(|e| AiError::Tool(format!("Failed to read file metadata: {}", e)))?;
                if metadata.len() as usize > self.config.max_read_bytes {
                    return Ok(ToolResult::error(format!(
                        "File too large ({} bytes), limit is {} bytes",
                        metadata.len(),
                        self.config.max_read_bytes
                    )));
                }
                let content = fs::read_to_string(path)
                    .map_err(|e| AiError::Tool(format!("Failed to read file: {}", e)))?;
                Ok(ToolResult::success(json!(content)))
            }
            "write" => {
                if !self.config.allow_write {
                    return Ok(ToolResult::error("Write operations not allowed"));
                }

                let content = args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AiError::Tool("Missing 'content' argument".to_string()))?;

                fs::write(path, content)
                    .map_err(|e| AiError::Tool(format!("Failed to write file: {}", e)))?;

                Ok(ToolResult::success(json!(format!(
                    "Wrote {} bytes",
                    content.len()
                ))))
            }
            _ => Ok(ToolResult::error(format!("Unknown action: {}", action))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_read_rejects_file_larger_than_limit() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("large.txt");
        fs::write(&file_path, "1234567890").unwrap();

        let tool = FileTool::new(FileConfig {
            allowed_paths: vec![temp_dir.path().to_path_buf()],
            allow_write: true,
            max_read_bytes: 5,
        });

        let result = tool
            .execute(json!({
                "action": "read",
                "path": file_path.to_string_lossy().to_string()
            }))
            .await
            .unwrap();

        assert!(!result.success);
        let error = result.error.unwrap_or_default();
        assert!(error.contains("File too large"));
    }

    #[test]
    fn test_file_config_default_max_read_bytes() {
        assert_eq!(FileConfig::default().max_read_bytes, 1_000_000);
    }
}
