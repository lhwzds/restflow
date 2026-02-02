//! Telegram notification tool for sending messages via Telegram Bot API.

use super::{Tool, ToolDefinition, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};

const TELEGRAM_API_BASE: &str = "https://api.telegram.org";

#[derive(Debug, Deserialize)]
struct TelegramInput {
    bot_token: String,
    chat_id: String,
    message: String,
    #[serde(default)]
    parse_mode: Option<String>,
    #[serde(default)]
    disable_web_page_preview: bool,
    #[serde(default)]
    disable_notification: bool,
}

pub struct TelegramTool {
    client: Client,
}

impl TelegramTool {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    pub fn with_client(client: Client) -> Self {
        Self { client }
    }

    fn api_url(bot_token: &str, method: &str) -> String {
        format!("{}/bot{}/{}", TELEGRAM_API_BASE, bot_token, method)
    }

    async fn send_message(&self, input: &TelegramInput) -> Result<Value> {
        let url = Self::api_url(&input.bot_token, "sendMessage");

        let mut payload = json!({
            "chat_id": input.chat_id,
            "text": input.message,
        });

        if let Some(ref parse_mode) = input.parse_mode {
            payload["parse_mode"] = json!(parse_mode);
        }
        if input.disable_web_page_preview {
            payload["disable_web_page_preview"] = json!(true);
        }
        if input.disable_notification {
            payload["disable_notification"] = json!(true);
        }

        let response = self.client.post(&url).json(&payload).send().await?;
        let status = response.status();
        let body: Value = response
            .json()
            .await
            .unwrap_or_else(|_| json!({"error": "Failed to parse response"}));

        if status.is_success() && body.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            Ok(body)
        } else {
            let error_desc = body
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            Err(anyhow::anyhow!("Telegram API error: {}", error_desc))
        }
    }
}

impl Default for TelegramTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for TelegramTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "telegram_send".to_string(),
            description: "Send a message to a Telegram chat via Bot API.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "bot_token": {
                        "type": "string",
                        "description": "Telegram Bot API token"
                    },
                    "chat_id": {
                        "type": "string",
                        "description": "Telegram chat ID to send message to"
                    },
                    "message": {
                        "type": "string",
                        "description": "Message text to send"
                    },
                    "parse_mode": {
                        "type": "string",
                        "enum": ["Markdown", "MarkdownV2", "HTML"],
                        "description": "Optional message parsing mode for formatting"
                    },
                    "disable_web_page_preview": {
                        "type": "boolean",
                        "description": "Disable link preview in the message"
                    },
                    "disable_notification": {
                        "type": "boolean",
                        "description": "Send the message silently without notification sound"
                    }
                },
                "required": ["bot_token", "chat_id", "message"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let params: TelegramInput = match serde_json::from_value(args) {
            Ok(p) => p,
            Err(e) => return Ok(ToolResult::error(format!("Invalid input: {}", e))),
        };

        if params.bot_token.is_empty() {
            return Ok(ToolResult::error("bot_token is required"));
        }
        if params.chat_id.is_empty() {
            return Ok(ToolResult::error("chat_id is required"));
        }
        if params.message.is_empty() {
            return Ok(ToolResult::error("message is required"));
        }

        match self.send_message(&params).await {
            Ok(response) => {
                let message_id = response
                    .get("result")
                    .and_then(|r| r.get("message_id"))
                    .and_then(|id| id.as_i64());
                let payload = json!({
                    "sent": true,
                    "message_id": message_id,
                    "chat_id": params.chat_id
                });
                Ok(ToolResult::success(payload.to_string()))
            }
            Err(e) => Ok(ToolResult::error(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telegram_tool_schema() {
        let tool = TelegramTool::new();
        assert_eq!(tool.definition().name, "telegram_send");
        assert!(!tool.definition().description.is_empty());
    }
}
