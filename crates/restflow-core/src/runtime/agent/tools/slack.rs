//! Slack tool for runtime agents.

use super::ToolResult;
use async_trait::async_trait;
use restflow_ai::error::{AiError, Result};
use restflow_ai::tools::Tool;
use serde::Deserialize;
use serde_json::{Value, json};

const SLACK_API_BASE: &str = "https://slack.com/api";

#[derive(Debug, Deserialize)]
struct SlackInput {
    bot_token: String,
    channel: String,
    message: String,
    #[serde(default)]
    thread_ts: Option<String>,
}

/// Simple Slack send tool for runtime agent use.
pub struct SlackTool;

impl Default for SlackTool {
    fn default() -> Self {
        Self
    }
}

impl SlackTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for SlackTool {
    fn name(&self) -> &str {
        "slack"
    }

    fn description(&self) -> &str {
        "Send a message to a Slack channel."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "bot_token": {
                    "type": "string",
                    "description": "Slack bot token"
                },
                "channel": {
                    "type": "string",
                    "description": "Slack channel ID"
                },
                "message": {
                    "type": "string",
                    "description": "Message content"
                },
                "thread_ts": {
                    "type": "string",
                    "description": "Thread timestamp for threaded replies"
                }
            },
            "required": ["bot_token", "channel", "message"]
        })
    }

    fn supports_parallel(&self) -> bool {
        false
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let payload: SlackInput = serde_json::from_value(args)
            .map_err(|e| AiError::Tool(format!("Invalid input: {}", e)))?;

        if payload.bot_token.is_empty() {
            return Ok(ToolResult::error("bot_token is required"));
        }
        if payload.channel.is_empty() {
            return Ok(ToolResult::error("channel is required"));
        }
        if payload.message.is_empty() {
            return Ok(ToolResult::error("message is required"));
        }

        let mut body = json!({
            "channel": payload.channel,
            "text": payload.message,
        });
        if let Some(ref ts) = payload.thread_ts {
            body["thread_ts"] = json!(ts);
        }

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/chat.postMessage", SLACK_API_BASE))
            .header("Authorization", format!("Bearer {}", payload.bot_token))
            .json(&body)
            .send()
            .await
            .map_err(|e| AiError::Tool(format!("Slack API error: {}", e)))?;

        let result: Value = response
            .json()
            .await
            .map_err(|e| AiError::Tool(format!("Failed to parse Slack response: {}", e)))?;

        if result["ok"].as_bool() == Some(true) {
            Ok(ToolResult::success(json!("Message sent")))
        } else {
            let err = result["error"].as_str().unwrap_or("unknown");
            Ok(ToolResult::error(format!("Slack error: {}", err)))
        }
    }
}
