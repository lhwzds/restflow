//! Diagnostics tool backed by a diagnostics provider (LSP).

use async_trait::async_trait;
use lsp_types::Diagnostic;
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use restflow_ai::error::AiError;
use crate::Result;

use crate::{Tool, ToolOutput};
use restflow_ai::tools::store_traits::DiagnosticsProvider;

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

        self.provider.ensure_open(path).await.map_err(|error| {
            AiError::Tool(format!(
                "Diagnostics service unavailable: {error}. The language server may not be running."
            ))
        })?;

        let diagnostics = self
            .provider
            .wait_for_diagnostics(path, Duration::from_millis(timeout_ms))
            .await
            .map_err(|error| {
                AiError::Tool(format!(
                    "Diagnostics timed out or failed: {error}. The file may have syntax errors that prevent analysis, or the language server is overloaded."
                ))
            })?;

        Ok(ToolOutput::success(serde_json::to_value(diagnostics)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct MockDiagnosticsProvider {
        fail_open: AtomicBool,
        fail_wait: AtomicBool,
    }

    #[async_trait]
    impl DiagnosticsProvider for MockDiagnosticsProvider {
        async fn ensure_open(&self, _path: &Path) -> Result<()> {
            if self.fail_open.load(Ordering::Relaxed) {
                return Err(crate::ToolError::Tool("provider open error".to_string()));
            }
            Ok(())
        }

        async fn did_change(&self, _path: &Path, _content: &str) -> Result<()> {
            Ok(())
        }

        async fn wait_for_diagnostics(
            &self,
            _path: &Path,
            _timeout: Duration,
        ) -> Result<Vec<Diagnostic>> {
            if self.fail_wait.load(Ordering::Relaxed) {
                return Err(crate::ToolError::Tool("provider wait error".to_string()));
            }
            Ok(Vec::new())
        }

        async fn get_diagnostics(&self, _path: &Path) -> Result<Vec<Diagnostic>> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn execute_wraps_ensure_open_errors_with_service_guidance() {
        let provider = Arc::new(MockDiagnosticsProvider {
            fail_open: AtomicBool::new(true),
            fail_wait: AtomicBool::new(false),
        });
        let tool = DiagnosticsTool::new(provider);

        let error = tool
            .execute(json!({ "path": "src/lib.rs" }))
            .await
            .expect_err("expected diagnostics ensure_open error");
        let message = error.to_string();

        assert!(message.contains("Diagnostics service unavailable"));
        assert!(message.contains("provider open error"));
        assert!(message.contains("language server may not be running"));
    }

    #[tokio::test]
    async fn execute_wraps_wait_errors_with_timeout_guidance() {
        let provider = Arc::new(MockDiagnosticsProvider {
            fail_open: AtomicBool::new(false),
            fail_wait: AtomicBool::new(true),
        });
        let tool = DiagnosticsTool::new(provider);

        let error = tool
            .execute(json!({ "path": "src/lib.rs", "timeout_ms": 1000 }))
            .await
            .expect_err("expected diagnostics wait error");
        let message = error.to_string();

        assert!(message.contains("Diagnostics timed out or failed"));
        assert!(message.contains("provider wait error"));
        assert!(message.contains("language server is overloaded"));
    }
}
