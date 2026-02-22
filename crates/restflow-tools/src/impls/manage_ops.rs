//! Unified operational diagnostics tool for daemon status, health, background summary,
//! session summary, and log tail.

use async_trait::async_trait;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::{Result, ToolError};
use crate::{Tool, ToolOutput};
use restflow_ai::tools::store_traits::OpsProvider;

pub struct ManageOpsTool {
    provider: Arc<dyn OpsProvider>,
}

impl ManageOpsTool {
    pub fn new(provider: Arc<dyn OpsProvider>) -> Self {
        Self { provider }
    }
}

fn parse_limit(input: &Value, key: &str, default: usize, max: usize) -> usize {
    input
        .get(key)
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(default)
        .clamp(1, max)
}

#[async_trait]
impl Tool for ManageOpsTool {
    fn name(&self) -> &str {
        "manage_ops"
    }

    fn description(&self) -> &str {
        "Unified operational diagnostics and control entry for daemon status, health snapshot, background-agent summary, session summary, and log tail."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["daemon_status", "daemon_health", "background_summary", "session_summary", "log_tail"],
                    "description": "Operation to execute."
                },
                "status": {
                    "type": "string",
                    "description": "Optional status filter for background_summary."
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Optional row limit for summary operations."
                },
                "lines": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Number of lines for log_tail."
                },
                "path": {
                    "type": "string",
                    "description": "Optional log file path for log_tail. Must stay under ~/.restflow/logs."
                }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let operation = input
            .get("operation")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::Tool("Missing operation parameter".to_string()))?;

        let result = match operation {
            "daemon_status" => self.provider.daemon_status()?,
            "daemon_health" => self.provider.daemon_health().await?,
            "background_summary" => {
                let status = input.get("status").and_then(Value::as_str);
                let limit = parse_limit(&input, "limit", 5, 100);
                self.provider.background_summary(status, limit)?
            }
            "session_summary" => {
                let limit = parse_limit(&input, "limit", 10, 100);
                self.provider.session_summary(limit)?
            }
            "log_tail" => {
                let lines = parse_limit(&input, "lines", 100, 1000);
                let path = input.get("path").and_then(Value::as_str);
                self.provider.log_tail(lines, path)?
            }
            other => {
                return Err(ToolError::Tool(format!(
                    "Unknown operation: {}. Supported: daemon_status, daemon_health, background_summary, session_summary, log_tail",
                    other
                )));
            }
        };

        Ok(ToolOutput::success(result))
    }
}
