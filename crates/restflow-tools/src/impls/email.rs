//! Email sending tool.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::error::Result;
use crate::security::{SecurityGate, ToolAction};
use crate::tool::{Tool, ToolOutput, check_security};

#[derive(Debug, Deserialize)]
struct EmailInput {
    to: String,
    subject: String,
    body: String,
    html: Option<bool>,
}

/// Email sending tool (placeholder - requires SMTP config).
pub struct EmailTool {
    dry_run: bool,
    security_gate: Option<Arc<dyn SecurityGate>>,
    agent_id: Option<String>,
    task_id: Option<String>,
}

impl Default for EmailTool {
    fn default() -> Self {
        Self::new()
    }
}

impl EmailTool {
    pub fn new() -> Self {
        Self {
            dry_run: true,
            security_gate: None,
            agent_id: None,
            task_id: None,
        }
    }

    pub fn with_dry_run(dry_run: bool) -> Self {
        Self {
            dry_run,
            security_gate: None,
            agent_id: None,
            task_id: None,
        }
    }

    pub fn with_security(
        mut self,
        security_gate: Arc<dyn SecurityGate>,
        agent_id: impl Into<String>,
        task_id: impl Into<String>,
    ) -> Self {
        self.security_gate = Some(security_gate);
        self.agent_id = Some(agent_id.into());
        self.task_id = Some(task_id.into());
        self
    }
}

#[async_trait]
impl Tool for EmailTool {
    fn name(&self) -> &str {
        "send_email"
    }

    fn description(&self) -> &str {
        "Send an email via configured SMTP with recipient, subject, and body content. This is the PRIMARY email tool."
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
        })
    }

    fn supports_parallel(&self) -> bool {
        false
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: EmailInput = serde_json::from_value(input)?;

        let action = ToolAction {
            tool_name: "email".to_string(),
            operation: "send".to_string(),
            target: params.to.clone(),
            summary: format!("Send email to {}", params.to),
        };

        if let Some(message) = check_security(
            self.security_gate.as_deref(),
            action,
            self.agent_id.as_deref(),
            self.task_id.as_deref(),
        )
        .await?
        {
            return Ok(ToolOutput::error(message));
        }

        if self.dry_run {
            Ok(ToolOutput::success(json!({
                "sent": false,
                "dry_run": true,
                "to": params.to,
                "subject": params.subject,
                "body_length": params.body.len(),
                "html": params.html.unwrap_or(false),
                "message": "Email would be sent (dry run mode)"
            })))
        } else {
            Ok(ToolOutput::error(
                "SMTP is not configured. Email sending unavailable.",
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
        assert_eq!(tool.name(), "send_email");
    }

    #[tokio::test]
    async fn test_email_tool_dry_run() {
        let tool = EmailTool::new();
        let input = json!({
            "to": "test@example.com",
            "subject": "Test",
            "body": "Hello"
        });

        let result = tool.execute(input).await.unwrap();
        assert!(result.success);
        assert_eq!(result.result["dry_run"], true);
    }
}
