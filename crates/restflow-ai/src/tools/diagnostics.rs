//! Diagnostics tool backed by a diagnostics provider (LSP).

use async_trait::async_trait;
use lsp_types::Diagnostic;
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crate::error::{AiError, Result};

use super::traits::{Tool, ToolOutput};

/// Provider interface for diagnostics.
#[async_trait]
pub trait DiagnosticsProvider: Send + Sync {
    async fn ensure_open(&self, path: &Path) -> Result<()>;
    async fn did_change(&self, path: &Path, content: &str) -> Result<()>;
    async fn wait_for_diagnostics(&self, path: &Path, timeout: Duration)
    -> Result<Vec<Diagnostic>>;
    async fn get_diagnostics(&self, path: &Path) -> Result<Vec<Diagnostic>>;
}

/// Tool for querying diagnostics from the provider.
#[derive(Clone)]
pub struct DiagnosticsTool {
    provider: Arc<dyn DiagnosticsProvider>,
}

impl DiagnosticsTool {
    pub fn new(provider: Arc<dyn DiagnosticsProvider>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl Tool for DiagnosticsTool {
    fn name(&self) -> &str {
        "diagnostics"
    }

    fn description(&self) -> &str {
        "Return language-server diagnostics for a file path, including errors and warnings."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path to fetch diagnostics for"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Max time to wait for diagnostics",
                    "default": 5000,
                    "minimum": 0
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AiError::Tool("Missing 'path' argument".to_string()))?;
        let timeout_ms = args
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(5000);

        let path = Path::new(path);

        self.provider.ensure_open(path).await?;

        let diagnostics = self
            .provider
            .wait_for_diagnostics(path, Duration::from_millis(timeout_ms))
            .await?;

        Ok(ToolOutput::success(serde_json::to_value(diagnostics)?))
    }
}
