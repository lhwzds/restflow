//! Slack send tool.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::http_client::build_http_client;
use crate::{Result, ToolError};
use crate::{Tool, ToolOutput};

const SLACK_API_BASE: &str = "https://slack.com/api";

#[derive(Debug, Deserialize)]
struct SlackInput {
    bot_token: String,
    channel: String,
    message: String,
    #[serde(default)]
    thread_ts: Option<String>,
}

/// Tool that sends messages to Slack channels.
pub struct SlackTool {
    client: reqwest::Client,
}

impl SlackTool {
    pub fn new() -> std::result::Result<Self, reqwest::Error> {
        Ok(Self {
            client: build_http_client()?,
        })
    }
}

fn sanitize_token(text: &str, bot_token: &str) -> String {
    if bot_token.is_empty() || text.is_empty() {
        return text.to_string();
    }

    text.replace(bot_token, "***")
}

fn format_request_error_message(raw_error: &str, bot_token: &str) -> String {
    let sanitized = sanitize_token(raw_error, bot_token);
    format!("Slack request failed: {}", sanitized)
}

fn format_api_error_message(raw_error: &str, bot_token: &str) -> String {
    let sanitized = sanitize_token(raw_error, bot_token);
    format!("Slack API error: {}", sanitized)
}

#[async_trait]
impl Tool for SlackTool {
    fn name(&self) -> &str {
        "slack_send"
    }

    fn description(&self) -> &str {
        "Send a message to a Slack channel. Requires bot_token and channel."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "bot_token": {
                    "type": "string",
                    "description": "Slack bot token (xoxb-...)"
                },
                "channel": {
                    "type": "string",
                    "description": "The Slack channel ID to send the message to"
                },
                "message": {
                    "type": "string",
                    "description": "The message content to send"
                },
                "thread_ts": {
                    "type": "string",
                    "description": "Optional thread timestamp to reply in a thread"
                }
            },
            "required": ["bot_token", "channel", "message"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: SlackInput = match serde_json::from_value(input) {
            Ok(p) => p,
            Err(e) => return Ok(ToolOutput::error(format!("Invalid input: {}", e))),
        };

        if params.bot_token.is_empty() {
            return Ok(ToolOutput::error("bot_token is required"));
        }
        if params.channel.is_empty() {
            return Ok(ToolOutput::error("channel is required"));
        }
        if params.message.is_empty() {
            return Ok(ToolOutput::error("message is required"));
        }

        let SlackInput {
            bot_token,
            channel,
            message,
            thread_ts,
        } = params;

        let mut body = json!({
            "channel": channel.clone(),
            "text": message,
        });
        if let Some(ts) = thread_ts {
            body["thread_ts"] = json!(ts);
        }

        let resp = self
            .client
            .post(format!("{}/chat.postMessage", SLACK_API_BASE))
            .header("Authorization", format!("Bearer {}", bot_token.as_str()))
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                ToolError::Tool(format_request_error_message(&e.to_string(), &bot_token))
            })?;

        let result: Value = resp
            .json()
            .await
            .unwrap_or_else(|_| json!({"error": "Failed to parse response"}));

        if result["ok"].as_bool() == Some(true) {
            let ts = result["ts"].as_str();
            Ok(ToolOutput::success(json!({
                "sent": true,
                "ts": ts,
                "channel": channel
            })))
        } else {
            let err = result["error"].as_str().unwrap_or("unknown");
            Ok(ToolOutput::error(format_api_error_message(err, &bot_token)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let tool = SlackTool::new().unwrap();
        assert_eq!(tool.name(), "slack_send");
    }

    #[test]
    fn test_schema_has_required_fields() {
        let tool = SlackTool::new().unwrap();
        let schema = tool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("channel")));
        assert!(required.contains(&json!("message")));
        assert!(required.contains(&json!("bot_token")));
    }

    #[tokio::test]
    async fn test_slack_tool_validation() {
        let tool = SlackTool::new().unwrap();
        let result = tool
            .execute(json!({
                "bot_token": "",
                "channel": "C123",
                "message": "test"
            }))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("bot_token is required"));
    }

    #[test]
    fn test_slack_error_sanitization_replaces_token() {
        let token = "xoxb-secret-token";
        let api_error = format!("invalid auth for {}", token);
        let sanitized = super::format_api_error_message(&api_error, token);
        assert!(!sanitized.contains(token));
        assert!(sanitized.contains("***"));
    }

    #[test]
    fn test_slack_request_error_message_masks_token() {
        let token = "xoxb-other-token";
        let raw = format!("https://slack.com/api?token={}", token);
        let msg = super::format_request_error_message(&raw, token);
        assert!(!msg.contains(token));
        assert!(msg.contains("request failed"));
    }
}
