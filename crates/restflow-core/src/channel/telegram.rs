//! Telegram Channel Implementation
//!
//! Implements bidirectional communication with Telegram via Bot API.
//! Supports both sending messages and receiving via long-polling.

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use super::traits::{Channel, StreamReceiver};
use super::types::{ChannelType, InboundMessage, OutboundMessage};

const TELEGRAM_API_BASE: &str = "https://api.telegram.org/bot";

/// Telegram channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    /// Bot token from @BotFather
    pub bot_token: String,
    /// Default chat ID for broadcasts (optional)
    pub default_chat_id: Option<String>,
    /// Polling timeout in seconds (default: 30)
    #[serde(default = "default_polling_timeout")]
    pub polling_timeout: u32,
}

fn default_polling_timeout() -> u32 {
    30
}

impl TelegramConfig {
    /// Create a new config with just the bot token
    pub fn new(bot_token: impl Into<String>) -> Self {
        Self {
            bot_token: bot_token.into(),
            default_chat_id: None,
            polling_timeout: default_polling_timeout(),
        }
    }

    /// Set default chat ID
    pub fn with_default_chat_id(mut self, chat_id: impl Into<String>) -> Self {
        self.default_chat_id = Some(chat_id.into());
        self
    }

    /// Set polling timeout
    pub fn with_polling_timeout(mut self, timeout: u32) -> Self {
        self.polling_timeout = timeout;
        self
    }
}

/// Telegram channel implementation
pub struct TelegramChannel {
    config: TelegramConfig,
    client: Client,
    /// Whether polling is active
    polling_active: Arc<AtomicBool>,
    /// Last update ID for long-polling
    last_update_id: Arc<AtomicI64>,
}

impl TelegramChannel {
    /// Create a new Telegram channel
    pub fn new(config: TelegramConfig) -> Self {
        Self {
            config,
            client: Client::new(),
            polling_active: Arc::new(AtomicBool::new(false)),
            last_update_id: Arc::new(AtomicI64::new(0)),
        }
    }

    /// Create with just bot token
    pub fn with_token(bot_token: impl Into<String>) -> Self {
        Self::new(TelegramConfig::new(bot_token))
    }

    /// Set default chat ID
    pub fn with_default_chat(mut self, chat_id: impl Into<String>) -> Self {
        self.config.default_chat_id = Some(chat_id.into());
        self
    }

    /// Get the API URL for a method
    fn api_url(&self, method: &str) -> String {
        format!("{}{}/{}", TELEGRAM_API_BASE, self.config.bot_token, method)
    }

    /// Format message with level emoji
    fn format_message(&self, message: &OutboundMessage) -> String {
        message.formatted_content()
    }

    /// Send message via Telegram API
    async fn send_message(
        &self,
        chat_id: &str,
        text: &str,
        parse_mode: Option<&str>,
        reply_to_message_id: Option<&str>,
    ) -> Result<TelegramMessageResponse> {
        let url = self.api_url("sendMessage");

        let mut params = serde_json::json!({
            "chat_id": chat_id,
            "text": text,
        });

        if let Some(mode) = parse_mode {
            params["parse_mode"] = serde_json::Value::String(mode.to_string());
        }

        // Extract numeric ID from our format (e.g., "tg_12345" -> 12345)
        if let Some(reply_id) = reply_to_message_id
            && let Some(numeric_id) = reply_id.strip_prefix("tg_")
            && let Ok(id) = numeric_id.parse::<i64>()
        {
            params["reply_to_message_id"] = serde_json::Value::Number(id.into());
        }

        let response = self.client.post(&url).json(&params).send().await?;

        if response.status().is_success() {
            let api_response: TelegramResponse<TelegramMessageResponse> = response.json().await?;
            if api_response.ok {
                Ok(api_response.result.unwrap())
            } else {
                Err(anyhow!(
                    "Telegram API error: {}",
                    api_response.description.unwrap_or_default()
                ))
            }
        } else {
            let error = response.text().await.unwrap_or_default();
            Err(anyhow!("Telegram HTTP error: {}", error))
        }
    }

    /// Poll for updates using long-polling
    async fn poll_updates(&self) -> Result<Vec<TelegramUpdate>> {
        let url = self.api_url("getUpdates");

        let offset = self.last_update_id.load(Ordering::SeqCst);
        let params = serde_json::json!({
            "offset": if offset > 0 { offset + 1 } else { 0 },
            "timeout": self.config.polling_timeout,
            "allowed_updates": ["message"],
        });

        let response = self
            .client
            .post(&url)
            .json(&params)
            .timeout(std::time::Duration::from_secs(
                self.config.polling_timeout as u64 + 10,
            ))
            .send()
            .await?;

        let body: TelegramResponse<Vec<TelegramUpdate>> = response.json().await?;

        if !body.ok {
            return Err(anyhow!(
                "Telegram API error: {:?}",
                body.description.unwrap_or_default()
            ));
        }

        let updates = body.result.unwrap_or_default();

        // Update last_update_id
        if let Some(last) = updates.last() {
            self.last_update_id.store(last.update_id, Ordering::SeqCst);
        }

        Ok(updates)
    }

    /// Convert Telegram update to InboundMessage
    fn convert_update(&self, update: TelegramUpdate) -> Option<InboundMessage> {
        let message = update.message?;
        let text = message.text?;
        let from = message.from?;

        let sender_name = from
            .username
            .clone()
            .or_else(|| {
                Some(format!(
                    "{}{}",
                    from.first_name.as_deref().unwrap_or(""),
                    from.last_name
                        .as_ref()
                        .map(|l| format!(" {}", l))
                        .unwrap_or_default()
                ))
            })
            .filter(|s| !s.is_empty());

        Some(
            InboundMessage::new(
                format!("tg_{}", message.message_id),
                ChannelType::Telegram,
                from.id.to_string(),
                message.chat.id.to_string(),
                text,
            )
            .with_sender_name(sender_name.unwrap_or_default())
            .with_metadata(serde_json::json!({
                "chat_type": message.chat.r#type,
                "chat_title": message.chat.title,
                "update_id": update.update_id,
            })),
        )
    }

    /// Test the connection by calling getMe
    pub async fn test_connection(&self) -> Result<TelegramUser> {
        let url = self.api_url("getMe");
        let response = self.client.get(&url).send().await?;

        let body: TelegramResponse<TelegramUser> = response.json().await?;

        if body.ok {
            Ok(body.result.unwrap())
        } else {
            Err(anyhow!(
                "Telegram API error: {}",
                body.description.unwrap_or_default()
            ))
        }
    }

    /// Send typing indicator (chat action) to show the bot is processing
    async fn send_typing_action(&self, chat_id: &str) -> Result<()> {
        let url = self.api_url("sendChatAction");

        let params = serde_json::json!({
            "chat_id": chat_id,
            "action": "typing",
        });

        let response = self.client.post(&url).json(&params).send().await?;

        if response.status().is_success() {
            let api_response: TelegramResponse<bool> = response.json().await?;
            if api_response.ok {
                debug!("Sent typing indicator to {}", chat_id);
                Ok(())
            } else {
                Err(anyhow!(
                    "Telegram API error: {}",
                    api_response.description.unwrap_or_default()
                ))
            }
        } else {
            let error = response.text().await.unwrap_or_default();
            Err(anyhow!("Telegram HTTP error: {}", error))
        }
    }
}

#[async_trait]
impl Channel for TelegramChannel {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Telegram
    }

    fn is_configured(&self) -> bool {
        !self.config.bot_token.is_empty()
    }

    async fn send(&self, message: OutboundMessage) -> Result<()> {
        let formatted = self.format_message(&message);
        let parse_mode = message.parse_mode.as_deref();

        self.send_message(
            &message.conversation_id,
            &formatted,
            parse_mode,
            message.reply_to.as_deref(),
        )
        .await?;

        Ok(())
    }

    async fn send_typing(&self, conversation_id: &str) -> Result<()> {
        self.send_typing_action(conversation_id).await
    }

    fn start_receiving(&self) -> Option<Pin<Box<dyn Stream<Item = InboundMessage> + Send>>> {
        if !self.is_configured() {
            return None;
        }

        let (tx, rx) = mpsc::unbounded_channel();
        let polling_active = self.polling_active.clone();
        let last_update_id = self.last_update_id.clone();
        let config = self.config.clone();
        let client = self.client.clone();

        // Spawn polling task
        tokio::spawn(async move {
            polling_active.store(true, Ordering::SeqCst);
            info!("Starting Telegram polling");

            let channel = TelegramChannel {
                config,
                client,
                polling_active: polling_active.clone(),
                last_update_id,
            };

            while polling_active.load(Ordering::SeqCst) {
                match channel.poll_updates().await {
                    Ok(updates) => {
                        for update in updates {
                            if let Some(message) = channel.convert_update(update) {
                                debug!(
                                    "Received Telegram message: {} from {}",
                                    message.id, message.sender_id
                                );
                                if tx.send(message).is_err() {
                                    warn!("Message receiver dropped, stopping polling");
                                    polling_active.store(false, Ordering::SeqCst);
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Telegram polling error: {}", e);
                        // Back off on error
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    }
                }
            }

            info!("Telegram polling stopped");
        });

        Some(Box::pin(
            tokio_stream::wrappers::UnboundedReceiverStream::new(rx),
        ))
    }
}

#[async_trait]
impl StreamReceiver for TelegramChannel {
    async fn start_polling(&self) -> Result<()> {
        if self.polling_active.load(Ordering::SeqCst) {
            return Ok(()); // Already polling
        }
        self.polling_active.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn stop_polling(&self) -> Result<()> {
        self.polling_active.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn is_polling(&self) -> bool {
        self.polling_active.load(Ordering::SeqCst)
    }
}

// ============================================================================
// Telegram API Types
// ============================================================================

#[derive(Debug, Deserialize)]
struct TelegramResponse<T> {
    ok: bool,
    result: Option<T>,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TelegramUpdate {
    update_id: i64,
    message: Option<TelegramMessage>,
}

#[derive(Debug, Deserialize)]
struct TelegramMessage {
    message_id: i64,
    from: Option<TelegramUser>,
    chat: TelegramChat,
    #[allow(dead_code)]
    date: u64,
    text: Option<String>,
    #[allow(dead_code)]
    reply_to_message: Option<Box<TelegramMessage>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TelegramUser {
    pub id: i64,
    pub is_bot: bool,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub username: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TelegramChat {
    id: i64,
    r#type: String,
    title: Option<String>,
    #[allow(dead_code)]
    username: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TelegramMessageResponse {
    #[allow(dead_code)]
    message_id: i64,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telegram_config_builder() {
        let config = TelegramConfig::new("test-token")
            .with_default_chat_id("12345")
            .with_polling_timeout(60);

        assert_eq!(config.bot_token, "test-token");
        assert_eq!(config.default_chat_id, Some("12345".to_string()));
        assert_eq!(config.polling_timeout, 60);
    }

    #[test]
    fn test_telegram_channel_is_configured() {
        let channel = TelegramChannel::with_token("test-token");
        assert!(channel.is_configured());

        let empty = TelegramChannel::with_token("");
        assert!(!empty.is_configured());
    }

    #[test]
    fn test_telegram_channel_type() {
        let channel = TelegramChannel::with_token("test");
        assert_eq!(channel.channel_type(), ChannelType::Telegram);
        assert!(channel.supports_interaction());
    }

    #[test]
    fn test_format_message() {
        let channel = TelegramChannel::with_token("test");

        let msg = OutboundMessage::success("123", "Task completed").with_title("Build Job");
        let formatted = channel.format_message(&msg);

        assert!(formatted.contains("✅"));
        assert!(formatted.contains("*Build Job*"));
        assert!(formatted.contains("Task completed"));
    }

    #[test]
    fn test_format_message_levels() {
        let channel = TelegramChannel::with_token("test");

        let info = OutboundMessage::new("123", "Info message");
        assert!(channel.format_message(&info).contains("ℹ️"));

        let warning = OutboundMessage::warning("123", "Warning message");
        assert!(channel.format_message(&warning).contains("⚠️"));

        let error = OutboundMessage::error("123", "Error message");
        assert!(channel.format_message(&error).contains("❌"));
    }

    #[test]
    fn test_api_url() {
        let channel = TelegramChannel::with_token("123:ABC");
        assert_eq!(
            channel.api_url("sendMessage"),
            "https://api.telegram.org/bot123:ABC/sendMessage"
        );
    }

    #[test]
    fn test_convert_update() {
        let channel = TelegramChannel::with_token("test");

        let update = TelegramUpdate {
            update_id: 12345,
            message: Some(TelegramMessage {
                message_id: 100,
                from: Some(TelegramUser {
                    id: 42,
                    is_bot: false,
                    first_name: Some("John".to_string()),
                    last_name: Some("Doe".to_string()),
                    username: Some("johndoe".to_string()),
                }),
                chat: TelegramChat {
                    id: 999,
                    r#type: "private".to_string(),
                    title: None,
                    username: None,
                },
                date: 1234567890,
                text: Some("Hello world".to_string()),
                reply_to_message: None,
            }),
        };

        let inbound = channel.convert_update(update).unwrap();
        assert_eq!(inbound.id, "tg_100");
        assert_eq!(inbound.sender_id, "42");
        assert_eq!(inbound.conversation_id, "999");
        assert_eq!(inbound.content, "Hello world");
        assert_eq!(inbound.sender_name, Some("johndoe".to_string()));
        assert_eq!(inbound.channel_type, ChannelType::Telegram);
    }

    #[test]
    fn test_convert_update_no_username() {
        let channel = TelegramChannel::with_token("test");

        let update = TelegramUpdate {
            update_id: 12345,
            message: Some(TelegramMessage {
                message_id: 100,
                from: Some(TelegramUser {
                    id: 42,
                    is_bot: false,
                    first_name: Some("John".to_string()),
                    last_name: Some("Doe".to_string()),
                    username: None,
                }),
                chat: TelegramChat {
                    id: 999,
                    r#type: "private".to_string(),
                    title: None,
                    username: None,
                },
                date: 1234567890,
                text: Some("Hello".to_string()),
                reply_to_message: None,
            }),
        };

        let inbound = channel.convert_update(update).unwrap();
        assert_eq!(inbound.sender_name, Some("John Doe".to_string()));
    }

    #[test]
    fn test_convert_update_no_message() {
        let channel = TelegramChannel::with_token("test");

        let update = TelegramUpdate {
            update_id: 12345,
            message: None,
        };

        assert!(channel.convert_update(update).is_none());
    }

    #[test]
    fn test_convert_update_no_text() {
        let channel = TelegramChannel::with_token("test");

        let update = TelegramUpdate {
            update_id: 12345,
            message: Some(TelegramMessage {
                message_id: 100,
                from: Some(TelegramUser {
                    id: 42,
                    is_bot: false,
                    first_name: Some("John".to_string()),
                    last_name: None,
                    username: None,
                }),
                chat: TelegramChat {
                    id: 999,
                    r#type: "private".to_string(),
                    title: None,
                    username: None,
                },
                date: 1234567890,
                text: None, // No text (e.g., photo message)
                reply_to_message: None,
            }),
        };

        assert!(channel.convert_update(update).is_none());
    }
}
