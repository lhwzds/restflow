//! Telegram notification tool for sending messages via Telegram Bot API

use async_trait::async_trait;
use reqwest::Client;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::Result;
use crate::http_client::build_http_client;
use crate::tools::traits::{Tool, ToolOutput};

const TELEGRAM_API_BASE: &str = "https://api.telegram.org";

#[derive(Debug, Deserialize)]
struct TelegramInput {
    /// Bot token for authentication
    bot_token: String,
    /// Chat ID to send message to
    chat_id: String,
    /// Message text to send
    message: String,
    /// Parse mode (optional): "Markdown", "MarkdownV2", or "HTML"
    #[serde(default)]
    parse_mode: Option<String>,
    /// Disable link preview (optional)
    #[serde(default)]
    disable_web_page_preview: bool,
    /// Disable notification sound (optional)
    #[serde(default)]
    disable_notification: bool,
}

/// Telegram notification tool for sending messages via Bot API
pub struct TelegramTool {
    client: Client,
}

impl Default for TelegramTool {
    fn default() -> Self {
        Self::new()
    }
}

impl TelegramTool {
    /// Create a new Telegram tool with default client
    pub fn new() -> Self {
        Self {
            client: build_http_client(),
        }
    }

    /// Create with a custom reqwest client
    pub fn with_client(client: Client) -> Self {
        Self { client }
    }

    /// Build the Telegram API URL for a specific method
    fn api_url(bot_token: &str, method: &str) -> String {
        format!("{}/bot{}/{}", TELEGRAM_API_BASE, bot_token, method)
    }

    fn format_api_error(status: StatusCode, body: &Value) -> String {
        if let Some(error_desc) = body.get("description").and_then(|v| v.as_str()) {
            return format!("Telegram API error: {}", error_desc);
        }
        format!(
            "Telegram API returned HTTP {} with no error description. The bot token may be invalid or the chat_id incorrect. Verify with manage_secrets.",
            status
        )
    }

    /// Send a message via Telegram Bot API
    async fn send_message(&self, input: &TelegramInput) -> Result<Value> {
        let url = Self::api_url(&input.bot_token, "sendMessage");

        let mut payload = json!({
            "chat_id": input.chat_id,
            "text": input.message,
        });

        // Add optional parameters
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
            Err(crate::error::AiError::Tool(Self::format_api_error(
                status, &body,
            )))
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

        // Validate required fields
        if params.bot_token.is_empty() {
            return Ok(ToolOutput::error("bot_token is required"));
        }
        if params.chat_id.is_empty() {
            return Ok(ToolOutput::error("chat_id is required"));
        }
        if params.message.is_empty() {
            return Ok(ToolOutput::error("message is required"));
        }

        // Send the message
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

/// Helper function to send a notification using the Telegram tool
///
/// This is a convenience function for use by the task runner to send
/// notifications without going through the full tool interface.
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
        .map_err(|e| format!("Failed to send Telegram message: {}", e))?;
    let status = response.status();

    let body: Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse Telegram response: {}", e))?;

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
        assert!(!tool.description().is_empty());

        let schema = tool.parameters_schema();
        let props = schema.get("properties").expect("should have properties");
        assert!(props.get("bot_token").is_some());
        assert!(props.get("chat_id").is_some());
        assert!(props.get("message").is_some());

        let required = schema.get("required").expect("should have required");
        let required_arr = required.as_array().expect("required should be array");
        assert_eq!(required_arr.len(), 3);
    }

    #[test]
    fn test_api_url_construction() {
        let url = TelegramTool::api_url("123:ABC", "sendMessage");
        assert_eq!(url, "https://api.telegram.org/bot123:ABC/sendMessage");
    }

    #[test]
    fn test_format_api_error_with_description() {
        let message = TelegramTool::format_api_error(
            StatusCode::BAD_REQUEST,
            &json!({"description": "chat not found"}),
        );
        assert_eq!(message, "Telegram API error: chat not found");
    }

    #[test]
    fn test_format_api_error_without_description() {
        let message = TelegramTool::format_api_error(StatusCode::UNAUTHORIZED, &json!({}));
        assert!(message.contains("Telegram API returned HTTP 401"));
        assert!(message.contains("bot token may be invalid"));
    }

    #[tokio::test]
    async fn test_telegram_tool_validation() {
        let tool = TelegramTool::new();

        // Test empty bot_token
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

        // Test empty chat_id
        let result = tool
            .execute(json!({
                "bot_token": "token",
                "chat_id": "",
                "message": "test"
            }))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("chat_id is required"));

        // Test empty message
        let result = tool
            .execute(json!({
                "bot_token": "token",
                "chat_id": "123",
                "message": ""
            }))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("message is required"));
    }

    #[test]
    fn test_telegram_input_deserialization() {
        let input: TelegramInput = serde_json::from_value(json!({
            "bot_token": "test_token",
            "chat_id": "12345",
            "message": "Hello, World!",
            "parse_mode": "Markdown",
            "disable_notification": true
        }))
        .unwrap();

        assert_eq!(input.bot_token, "test_token");
        assert_eq!(input.chat_id, "12345");
        assert_eq!(input.message, "Hello, World!");
        assert_eq!(input.parse_mode, Some("Markdown".to_string()));
        assert!(input.disable_notification);
        assert!(!input.disable_web_page_preview);
    }

    #[test]
    fn test_telegram_input_defaults() {
        let input: TelegramInput = serde_json::from_value(json!({
            "bot_token": "token",
            "chat_id": "123",
            "message": "test"
        }))
        .unwrap();

        assert!(input.parse_mode.is_none());
        assert!(!input.disable_notification);
        assert!(!input.disable_web_page_preview);
    }
}
