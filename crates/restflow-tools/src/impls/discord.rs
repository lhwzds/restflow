//! Discord send tool.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::http_client::build_http_client;
use crate::{Result, ToolError};
use crate::{Tool, ToolOutput};

const DISCORD_API_BASE: &str = "https://discord.com/api/v10";

#[derive(Debug, Deserialize)]
struct DiscordInput {
    bot_token: String,
    channel_id: String,
    message: String,
}

/// Tool that sends messages to Discord channels.
pub struct DiscordTool {
    client: reqwest::Client,
}

impl DiscordTool {
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
    format!("Discord request failed: {}", sanitized)
}

fn format_api_error_message(
    raw_error: &str,
    bot_token: &str,
    status: reqwest::StatusCode,
) -> String {
    let sanitized = sanitize_token(raw_error, bot_token);
    format!("Discord API error ({}): {}", status, sanitized)
}

#[async_trait]
impl Tool for DiscordTool {
    fn name(&self) -> &str {
        "discord_send"
    }

    fn description(&self) -> &str {
        "Send a message to a Discord channel. Requires bot_token and channel_id."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "bot_token": {
                    "type": "string",
                    "description": "Discord bot token"
                },
                "channel_id": {
                    "type": "string",
                    "description": "The Discord channel ID to send the message to"
                },
                "message": {
                    "type": "string",
                    "description": "The message content to send"
                }
            },
            "required": ["bot_token", "channel_id", "message"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: DiscordInput = match serde_json::from_value(input) {
            Ok(p) => p,
            Err(e) => return Ok(ToolOutput::error(format!("Invalid input: {}", e))),
        };

        if params.bot_token.is_empty() {
            return Ok(ToolOutput::error("bot_token is required"));
        }
        if params.channel_id.is_empty() {
            return Ok(ToolOutput::error("channel_id is required"));
        }
        if params.message.is_empty() {
            return Ok(ToolOutput::error("message is required"));
        }

        let DiscordInput {
            bot_token,
            channel_id,
            message,
        } = params;
        let channel_id_for_response = channel_id.clone();

        let resp = self
            .client
            .post(format!(
                "{}/channels/{}/messages",
                DISCORD_API_BASE, channel_id
            ))
            .header("Authorization", format!("Bot {}", bot_token.as_str()))
            .json(&json!({ "content": message }))
            .send()
            .await
            .map_err(|e| {
                ToolError::Tool(format_request_error_message(&e.to_string(), &bot_token))
            })?;

        let status = resp.status();
        let body: Value = resp
            .json()
            .await
            .unwrap_or_else(|_| json!({"error": "Failed to parse response"}));

        if status.is_success() {
            let message_id = body.get("id").and_then(|v| v.as_str());
            Ok(ToolOutput::success(json!({
                "sent": true,
                "message_id": message_id,
                "channel_id": channel_id_for_response
            })))
        } else {
            let err = body["message"].as_str().unwrap_or("Unknown error");
            Ok(ToolOutput::error(format_api_error_message(
                err, &bot_token, status,
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let tool = DiscordTool::new().unwrap();
        assert_eq!(tool.name(), "discord_send");
    }

    #[test]
    fn test_schema_has_required_fields() {
        let tool = DiscordTool::new().unwrap();
        let schema = tool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("channel_id")));
        assert!(required.contains(&json!("message")));
        assert!(required.contains(&json!("bot_token")));
    }

    #[tokio::test]
    async fn test_discord_tool_validation() {
        let tool = DiscordTool::new().unwrap();
        let result = tool
            .execute(json!({
                "bot_token": "",
                "channel_id": "123",
                "message": "test"
            }))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("bot_token is required"));
    }

    #[test]
    fn test_discord_request_error_masks_token() {
        let token = "discord-token";
        let raw = format!("network failure for {}", token);
        let message = super::format_request_error_message(&raw, token);
        assert!(!message.contains(token));
        assert!(message.contains("request failed"));
    }

    #[test]
    fn test_discord_api_error_masks_token() {
        let token = "discord-secret";
        let raw = format!("bot token {} invalid", token);
        let message =
            super::format_api_error_message(&raw, token, reqwest::StatusCode::UNAUTHORIZED);
        assert!(!message.contains(token));
        assert!(message.contains("***"));
    }
}
