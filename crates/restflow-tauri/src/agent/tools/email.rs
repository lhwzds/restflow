//! Email tool (placeholder - requires SMTP configuration).

use super::ToolResult;
use async_trait::async_trait;
use restflow_ai::error::{AiError, Result};
use restflow_ai::tools::Tool;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct EmailInput {
    to: String,
    subject: String,
    body: String,
}

pub struct EmailTool;

impl Default for EmailTool {
    fn default() -> Self {
        Self
    }
}

impl EmailTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for EmailTool {
    fn name(&self) -> &str {
        "email"
    }

    fn description(&self) -> &str {
        "Send an email (requires SMTP configuration)."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "to": {
                    "type": "string",
                    "description": "Recipient email address"
                },
                "subject": {
                    "type": "string",
                    "description": "Email subject"
                },
                "body": {
                    "type": "string",
                    "description": "Email body"
                }
            },
            "required": ["to", "subject", "body"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let payload: EmailInput = serde_json::from_value(args)
            .map_err(|e| AiError::Tool(format!("Invalid input: {}", e)))?;

        // Placeholder - in a real implementation, this would use SMTP
        tracing::info!(
            "Email tool called with to={}, subject={}",
            payload.to,
            payload.subject
        );

        Ok(ToolResult::error(
            "Email tool not configured - SMTP settings required",
        ))
    }
}
