//! Email sending tool (dry-run placeholder).

use super::{Tool, ToolDefinition, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
struct EmailInput {
    to: String,
    subject: String,
    body: String,
    html: Option<bool>,
}

pub struct EmailTool {
    dry_run: bool,
}

impl EmailTool {
    pub fn new() -> Self {
        Self { dry_run: true }
    }

    pub fn with_dry_run(dry_run: bool) -> Self {
        Self { dry_run }
    }
}

impl Default for EmailTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for EmailTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "send_email".to_string(),
            description: "Send an email to a recipient. Requires SMTP configuration.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "to": {
                        "type": "string",
                        "description": "Recipient email address"
                    },
                    "subject": {
                        "type": "string",
                        "description": "Email subject line"
                    },
                    "body": {
                        "type": "string",
                        "description": "Email body content"
                    },
                    "html": {
                        "type": "boolean",
                        "description": "Whether body is HTML (default: false)"
                    }
                },
                "required": ["to", "subject", "body"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let params: EmailInput = serde_json::from_value(args)?;

        if self.dry_run {
            let payload = json!({
                "sent": false,
                "dry_run": true,
                "to": params.to,
                "subject": params.subject,
                "body_length": params.body.len(),
                "html": params.html.unwrap_or(false),
                "message": "Email would be sent (dry run mode)"
            });
            Ok(ToolResult::success(payload.to_string()))
        } else {
            Ok(ToolResult::error(
                "SMTP not configured. Set dry_run=false only when SMTP is configured.",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_tool_schema() {
        let tool = EmailTool::new();
        assert_eq!(tool.definition().name, "send_email");
        assert!(!tool.definition().description.is_empty());
    }
}
