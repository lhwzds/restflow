//! Telegram tool for sending messages.

use super::ToolResult;
use async_trait::async_trait;
use restflow_ai::error::{AiError, Result};
use restflow_ai::tools::Tool;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
struct TelegramInput {
    bot_token: String,
    chat_id: String,
    message: String,
    /// Message thread ID for Telegram forum/supergroup topics (optional)
    #[serde(default)]
    message_thread_id: Option<i64>,
}

pub struct TelegramTool;

impl Default for TelegramTool {
    fn default() -> Self {
        Self
    }
}

impl TelegramTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for TelegramTool {
    fn name(&self) -> &str {
        "telegram"
    }

    fn description(&self) -> &str {
        "Send a Telegram message using a bot token."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "bot_token": {
                    "type": "string",
                    "description": "Bot token"
                },
                "chat_id": {
                    "type": "string",
                    "description": "Chat ID"
                },
                "message": {
                    "type": "string",
                    "description": "Message content"
                },
                "message_thread_id": {
                    "type": "integer",
                    "description": "Unique identifier for the target message thread (topic) in a forum or supergroup"
                }
            },
            "required": ["bot_token", "chat_id", "message"]
        })
    }

    fn supports_parallel(&self) -> bool {
        false
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let payload: TelegramInput = serde_json::from_value(args)
            .map_err(|e| AiError::Tool(format!("Invalid input: {}", e)))?;

        if payload.bot_token.is_empty() {
            return Ok(ToolResult::error("bot_token is required"));
        }
        if payload.chat_id.is_empty() {
            return Ok(ToolResult::error("chat_id is required"));
        }
        if payload.message.is_empty() {
            return Ok(ToolResult::error("message is required"));
        }

        // Make Telegram API call
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            payload.bot_token
        );

        let mut params = serde_json::json!({
            "chat_id": payload.chat_id,
            "text": payload.message
        });
        
        if let Some(thread_id) = payload.message_thread_id {
            params["message_thread_id"] = json!(thread_id);
        }

        let client = reqwest::Client::new();
        let response = client
            .post(&url)
            .json(&params)
            .send()
            .await
            .map_err(|e| AiError::Tool(format!("Telegram API error: {}", e)))?;

        if response.status().is_success() {
            Ok(ToolResult::success(json!("Message sent")))
        } else {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            Ok(ToolResult::error(format!(
                "Telegram API error {}: {}",
                status, text
            )))
        }
    }
}
