//! File read/write tool with path restrictions.

use super::{Tool, ToolDefinition, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use tokio::fs;

/// Configuration for file tool security.
#[derive(Debug, Clone)]
pub struct FileConfig {
    /// Allowed base directories (files outside these are blocked).
    pub allowed_paths: Vec<PathBuf>,

    /// Maximum file size to read (bytes).
    pub max_read_size: usize,

    /// Whether to allow writes.
    pub allow_write: bool,
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            allowed_paths: vec![],
            max_read_size: 1024 * 1024,
            allow_write: false,
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
        if self.config.allowed_paths.is_empty() {
            return true;
        }

        let canonical = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => return false,
        };

        self.config
            .allowed_paths
            .iter()
            .any(|allowed| canonical.starts_with(allowed))
    }
}

#[async_trait]
impl Tool for FileTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "file".to_string(),
            description: "Read or write files. Supports 'read' and 'write' operations.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["read", "write"],
                        "description": "The operation to perform"
                    },
                    "path": {
                        "type": "string",
                        "description": "The file path"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write (required for write operation)"
                    }
                },
                "required": ["operation", "path"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let operation = args
            .get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'operation' argument"))?;
        let path_str = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' argument"))?;
        let path = Path::new(path_str);

        if !self.is_path_allowed(path) {
            return Ok(ToolResult::error("Path not allowed"));
        }

        match operation {
            "read" => {
                let metadata = fs::metadata(path)
                    .await
                    .map_err(|e| anyhow::anyhow!("Cannot access file: {}", e))?;
                if metadata.len() as usize > self.config.max_read_size {
                    return Ok(ToolResult::error(format!(
                        "File too large ({} bytes, max {})",
                        metadata.len(),
                        self.config.max_read_size
                    )));
                }
                let content = fs::read_to_string(path)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to read file: {}", e))?;
                Ok(ToolResult::success(content))
            }
            "write" => {
                if !self.config.allow_write {
                    return Ok(ToolResult::error("Write operations not allowed"));
                }
                let content = args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'content' for write operation"))?;
                fs::write(path, content)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to write file: {}", e))?;
                Ok(ToolResult::success(format!(
                    "Written {} bytes to {}",
                    content.len(),
                    path_str
                )))
            }
            _ => Ok(ToolResult::error(format!(
                "Unknown operation: {}",
                operation
            ))),
        }
    }
}
