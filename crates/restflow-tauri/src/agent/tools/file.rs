//! File system tool for reading and writing files.

use crate::agent::tools::ToolResult;
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
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            allowed_paths: vec![std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))],
            allow_write: true,
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
