//! Tool for LSP-based diagnostics.

use async_trait::async_trait;
use lsp_types::DiagnosticSeverity;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::error::Result;
use crate::lsp::LspManager;

use super::traits::{Tool, ToolOutput};

/// Tool that retrieves diagnostics from LSP.
#[derive(Debug, Clone)]
pub struct DiagnosticsTool {
    lsp_manager: Arc<Mutex<LspManager>>,
}

impl DiagnosticsTool {
    /// Create a new diagnostics tool.
    pub fn new(lsp_manager: Arc<Mutex<LspManager>>) -> Self {
        Self { lsp_manager }
    }
}

#[async_trait]
impl Tool for DiagnosticsTool {
    fn name(&self) -> &str {
        "diagnostics"
    }

    fn description(&self) -> &str {
        "Get code diagnostics (errors, warnings) for a file using LSP. Use this after editing code."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to diagnose"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::error::AiError::Tool("path is required".to_string()))?;
        let path = std::path::Path::new(path);

        let content = tokio::fs::read_to_string(path).await?;

        let diagnostics = {
            let mut manager = self.lsp_manager.lock().await;
            manager.get_diagnostics(path, &content).await
        };

        let diagnostics = match diagnostics {
            Ok(list) => list,
            Err(err) => {
                return Ok(ToolOutput::error(format!(
                    "Failed to get diagnostics: {}",
                    err
                )))
            }
        };

        if diagnostics.is_empty() {
            return Ok(ToolOutput::success(serde_json::json!({
                "message": "No diagnostics found."
            })));
        }

        let formatted: Vec<Value> = diagnostics
            .iter()
            .map(|diag| {
                let severity = match diag.severity {
                    Some(DiagnosticSeverity::ERROR) => "error",
                    Some(DiagnosticSeverity::WARNING) => "warning",
                    Some(DiagnosticSeverity::INFORMATION) => "information",
                    Some(DiagnosticSeverity::HINT) => "hint",
                    None => "unknown",
                };

                serde_json::json!({
                    "severity": severity,
                    "message": diag.message,
                    "line": diag.range.start.line + 1,
                    "character": diag.range.start.character + 1
                })
            })
            .collect();

        Ok(ToolOutput::success(serde_json::json!({
            "diagnostics": formatted
        })))
    }
}
