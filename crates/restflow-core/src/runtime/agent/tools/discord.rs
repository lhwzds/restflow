//! Discord tool for runtime agents.

use super::ToolResult;
use async_trait::async_trait;
use restflow_ai::error::{AiError, Result};
use restflow_ai::tools::Tool;
use serde::Deserialize;
use serde_json::{Value, json};

const DISCORD_API_BASE: &str = "https://discord.com/api/v10";

#[derive(Debug, Deserialize)]
struct DiscordInput {
    bot_token: String,
    channel_id: String,
    message: String,
}

/// Simple Discord send tool for runtime agent use.
pub struct DiscordTool;

impl Default for DiscordTool {
    fn default() -> Self {
        Self
    }
}

impl DiscordTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for DiscordTool {
    fn name(&self) -> &str {
        "discord"
    }

    fn description(&self) -> &str {
        "Send a message to a Discord channel."
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
                    "description": "Discord channel ID"
                },
                "message": {
                    "type": "string",
                    "description": "Message content"
                }
            },
            "required": ["bot_token", "channel_id", "message"]
        })
    }

    fn supports_parallel(&self) -> bool {
        false
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let payload: DiscordInput = serde_json::from_value(args)
            .map_err(|e| AiError::Tool(format!("Invalid input: {}", e)))?;

        if payload.bot_token.is_empty() {
            return Ok(ToolResult::error("bot_token is required"));
        }
        if payload.channel_id.is_empty() {
            return Ok(ToolResult::error("channel_id is required"));
        }
        if payload.message.is_empty() {
            return Ok(ToolResult::error("message is required"));
        }

        let client = reqwest::Client::new();
        let response = client
            .post(format!(
                "{}/channels/{}/messages",
                DISCORD_API_BASE, payload.channel_id
            ))
            .header("Authorization", format!("Bot {}", payload.bot_token))
            .json(&json!({ "content": payload.message }))
            .send()
            .await
            .map_err(|e| AiError::Tool(format!("Discord API error: {}", e)))?;

        if response.status().is_success() {
            Ok(ToolResult::success(json!("Message sent")))
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            Ok(ToolResult::error(format!(
                "Discord error ({}): {}",
                status, body
            )))
        }
    }
}
