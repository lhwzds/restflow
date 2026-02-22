//! Discord send tool for AI agents.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::Result;
use crate::http_client::build_http_client;
use crate::tools::traits::{Tool, ToolOutput};

const DISCORD_API_BASE: &str = "https://discord.com/api/v10";

#[derive(Debug, Deserialize)]
struct DiscordInput {
    /// Discord bot token
    bot_token: String,
    /// Discord channel ID
    channel_id: String,
    /// Message content
    message: String,
}

/// Tool that sends messages to Discord channels.
pub struct DiscordTool {
    client: reqwest::Client,
}

impl Default for DiscordTool {
    fn default() -> Self {
        Self::new()
    }
}

impl DiscordTool {
    pub fn new() -> Self {
        Self {
            client: build_http_client(),
        }
    }
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

    fn supports_parallel(&self) -> bool {
        false
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

        let resp = self
            .client
            .post(format!(
                "{}/channels/{}/messages",
                DISCORD_API_BASE, params.channel_id
            ))
            .header("Authorization", format!("Bot {}", params.bot_token))
            .json(&json!({ "content": params.message }))
            .send()
            .await
            .map_err(|e| crate::error::AiError::Tool(e.to_string()))?;

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
                "channel_id": params.channel_id
            })))
        } else {
            let err = body["message"].as_str().unwrap_or("Unknown error");
            Ok(ToolOutput::error(format!(
                "Discord API error ({}): {}",
                status, err
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let tool = DiscordTool::new();
        assert_eq!(tool.name(), "discord_send");
    }

    #[test]
    fn test_schema_has_required_fields() {
        let tool = DiscordTool::new();
        let schema = tool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("channel_id")));
        assert!(required.contains(&json!("message")));
        assert!(required.contains(&json!("bot_token")));
    }

    #[tokio::test]
    async fn test_discord_tool_validation() {
        let tool = DiscordTool::new();

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

        let result = tool
            .execute(json!({
                "bot_token": "token",
                "channel_id": "",
                "message": "test"
            }))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("channel_id is required"));
    }
}
