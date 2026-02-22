//! Telegram notification tool for sending messages via Telegram Bot API.

use async_trait::async_trait;
use reqwest::Client;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::{Result, ToolError};
use crate::http_client::build_http_client;
use crate::tool::{Tool, ToolOutput};

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
    #[serde(default)]
    message_thread_id: Option<i64>,
}

/// Telegram notification tool for sending messages via Bot API.
pub struct TelegramTool {
    client: Client,
}

impl Default for TelegramTool {
    fn default() -> Self {
        Self::new()
    }
}

impl TelegramTool {
    pub fn new() -> Self {
        Self {
            client: build_http_client(),
        }
    }

    pub fn with_client(client: Client) -> Self {
        Self { client }
    }

    fn api_url(bot_token: &str, method: &str) -> String {
        format!("{}/bot{}/{}", TELEGRAM_API_BASE, bot_token, method)
    }

    fn sanitize_request_error(error: &reqwest::Error, bot_token: &str) -> String {
        let error_str = error.to_string();
        let sanitized = error_str.replace(bot_token, "***");

        if sanitized.contains("api.telegram.org/bot") && !sanitized.contains("***") {
            "Telegram request failed: network error".to_string()
        } else {
            format!("Telegram request failed: {}", sanitized)
        }
    }

    pub fn format_api_error(status: StatusCode, body: &Value) -> String {
        if let Some(error_desc) = body.get("description").and_then(|v| v.as_str()) {
            return format!("Telegram API error: {}", error_desc);
        }
        format!(
            "Telegram API returned HTTP {} with no error description. The bot token may be invalid or the chat_id incorrect.",
            status
        )
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
        if let Some(thread_id) = input.message_thread_id {
            payload["message_thread_id"] = json!(thread_id);
        }

        let response = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| ToolError::Tool(Self::sanitize_request_error(&e, &input.bot_token)))?;

        let status = response.status();
        let body: Value = response
            .json()
            .await
            .unwrap_or_else(|_| json!({"error": "Failed to parse response"}));

        if status.is_success() && body.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            Ok(body)
        } else {
            Err(ToolError::Tool(Self::format_api_error(status, &body)))
        }
    }
}

#[async_trait]
impl Tool for TelegramTool {
    fn name(&self) -> &str {
        "telegram_send"
    }

    fn description(&self) -> &str {
        "Send Telegram Bot API messages to a chat with optional formatting and notification flags. This is the PRIMARY Telegram tool."
    }

    fn parameters_schema(&self) -> Value {
        json!({
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

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: TelegramInput = match serde_json::from_value(input) {
            Ok(p) => p,
            Err(e) => return Ok(ToolOutput::error(format!("Invalid input: {}", e))),
        };

        if params.bot_token.is_empty() {
            return Ok(ToolOutput::error("bot_token is required"));
        }
        if params.chat_id.is_empty() {
            return Ok(ToolOutput::error("chat_id is required"));
        }
        if params.message.is_empty() {
            return Ok(ToolOutput::error("message is required"));
        }

        match self.send_message(&params).await {
            Ok(response) => {
                let message_id = response
                    .get("result")
                    .and_then(|r| r.get("message_id"))
                    .and_then(|id| id.as_i64());

                Ok(ToolOutput::success(json!({
                    "sent": true,
                    "message_id": message_id,
                    "chat_id": params.chat_id
                })))
            }
            Err(e) => Ok(ToolOutput::error(e.to_string())),
        }
    }
}

/// Helper function to send a notification using the Telegram tool.
pub async fn send_telegram_notification(
    bot_token: &str,
    chat_id: &str,
    message: &str,
    parse_mode: Option<&str>,
) -> std::result::Result<(), String> {
    let client = build_http_client();
    let url = format!("{}/bot{}/sendMessage", TELEGRAM_API_BASE, bot_token);

    let mut payload = json!({
        "chat_id": chat_id,
        "text": message,
    });

    if let Some(mode) = parse_mode {
        payload["parse_mode"] = json!(mode);
    }

    let response = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| {
            let error_str = e.to_string();
            let sanitized = error_str.replace(bot_token, "***");
            format!("Failed to send Telegram message: {}", sanitized)
        })?;
    let status = response.status();

    let body: Value = response.json().await.unwrap_or_else(|_| json!({}));

    if body.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        Ok(())
    } else {
        Err(TelegramTool::format_api_error(status, &body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telegram_tool_schema() {
        let tool = TelegramTool::new();
        assert_eq!(tool.name(), "telegram_send");
    }

    #[test]
    fn test_api_url_construction() {
        let url = TelegramTool::api_url("123:ABC", "sendMessage");
        assert_eq!(url, "https://api.telegram.org/bot123:ABC/sendMessage");
    }

    #[tokio::test]
    async fn test_telegram_tool_validation() {
        let tool = TelegramTool::new();
        let result = tool
            .execute(json!({
                "bot_token": "",
                "chat_id": "123",
                "message": "test"
            }))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("bot_token is required"));
    }
}
