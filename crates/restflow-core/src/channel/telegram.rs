//! Telegram Channel Implementation
//!
//! Implements bidirectional communication with Telegram via Bot API.
//! Supports both sending messages and receiving via long-polling.

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use tokio::fs;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::chunk::chunk_markdown;
use super::traits::{Channel, StreamReceiver};
use super::types::{ChannelType, InboundMessage, OutboundMessage};

const TELEGRAM_API_BASE: &str = "https://api.telegram.org/bot";
/// Default timeout for Telegram API calls (seconds)
const API_TIMEOUT_SECS: u64 = 30;

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
    /// Persist Telegram offset when it changes.
    offset_persister: Option<Arc<dyn Fn(i64) + Send + Sync>>,
}

impl TelegramChannel {
    /// Parse conversation_id into (chat_id, thread_id)
    /// Format: "chat_id" or "chat_id:thread_id"
    fn parse_conversation_id(conversation_id: &str) -> (String, Option<i64>) {
        if let Some(colon_pos) = conversation_id.find(':') {
            let chat_id = &conversation_id[..colon_pos];
            let thread_part = &conversation_id[colon_pos + 1..];
            let thread_id = thread_part.parse::<i64>().ok();
            (chat_id.to_string(), thread_id)
        } else {
            (conversation_id.to_string(), None)
        }
    }

    fn build_conversation_id(chat_id: i64, message_thread_id: Option<i64>) -> String {
        match message_thread_id {
            Some(thread_id) => format!("{}:{}", chat_id, thread_id),
            None => chat_id.to_string(),
        }
    }

    /// Create a new Telegram channel
    pub fn new(config: TelegramConfig) -> Self {
        Self {
            config,
            client: Client::new(),
            polling_active: Arc::new(AtomicBool::new(false)),
            last_update_id: Arc::new(AtomicI64::new(0)),
            offset_persister: None,
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

    /// Restore the last processed Telegram update ID.
    pub fn with_last_update_id(self, update_id: i64) -> Self {
        self.last_update_id.store(update_id, Ordering::SeqCst);
        self
    }

    /// Persist offset after each successful polling batch.
    pub fn with_offset_persister(mut self, persister: Arc<dyn Fn(i64) + Send + Sync>) -> Self {
        self.offset_persister = Some(persister);
        self
    }

    /// Return current last processed update ID.
    pub fn last_update_id(&self) -> i64 {
        self.last_update_id.load(Ordering::SeqCst)
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
        message_thread_id: Option<i64>,
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

        // Add message_thread_id for Telegram forum/supergroup topics
        if let Some(thread_id) = message_thread_id {
            params["message_thread_id"] = serde_json::Value::Number(thread_id.into());
        }

        let response = self
            .client
            .post(&url)
            .json(&params)
            .timeout(std::time::Duration::from_secs(API_TIMEOUT_SECS))
            .send()
            .await?;

        if response.status().is_success() {
            let api_response: TelegramResponse<TelegramMessageResponse> = response.json().await?;
            if api_response.ok {
                Ok(api_response
                    .result
                    .ok_or_else(|| anyhow!("Telegram returned ok but no result"))?)
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
            if let Some(persister) = &self.offset_persister {
                persister(last.update_id);
            }
        }

        Ok(updates)
    }

    async fn download_telegram_file(&self, file_id: &str) -> Result<Option<String>> {
        #[cfg(test)]
        if file_id.starts_with("test-") {
            return Ok(Some(format!("/tmp/restflow-media/{}", file_id)));
        }

        let url = self.api_url("getFile");
        let params = serde_json::json!({
            "file_id": file_id,
        });

        let response = self
            .client
            .post(&url)
            .json(&params)
            .timeout(std::time::Duration::from_secs(API_TIMEOUT_SECS))
            .send()
            .await?;

        let body: TelegramResponse<TelegramFile> = response.json().await?;
        if !body.ok {
            return Err(anyhow!(
                "Telegram API error: {:?}",
                body.description.unwrap_or_default()
            ));
        }

        let file = match body.result {
            Some(file) => file,
            None => return Ok(None),
        };

        let file_path = match file.file_path {
            Some(path) => path,
            None => return Ok(None),
        };

        let file_url = format!(
            "https://api.telegram.org/file/bot{}/{}",
            self.config.bot_token, file_path
        );

        let response = self
            .client
            .get(&file_url)
            .timeout(std::time::Duration::from_secs(API_TIMEOUT_SECS))
            .send()
            .await?;

        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            return Err(anyhow!("Telegram file download error: {}", error));
        }

        let bytes = response.bytes().await?;
        let dir = "/tmp/restflow-media";
        fs::create_dir_all(dir).await?;

        let extension = Path::new(&file_path)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| format!(".{}", ext))
            .unwrap_or_default();

        let filename = format!("tg-{}{}", Uuid::new_v4(), extension);
        let local_path = format!("{}/{}", dir, filename);

        fs::write(&local_path, bytes).await?;

        Ok(Some(local_path))
    }

    /// Convert Telegram update to InboundMessage
    async fn convert_update(&self, update: TelegramUpdate) -> Option<InboundMessage> {
        let message = update.message?;
        let from = message.from?;
        let conversation_id =
            Self::build_conversation_id(message.chat.id, message.message_thread_id);

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

        let mut metadata = serde_json::json!({
            "chat_type": message.chat.r#type,
            "chat_title": message.chat.title,
            "update_id": update.update_id,
        });
        if let Some(thread_id) = message.message_thread_id {
            metadata["message_thread_id"] = serde_json::Value::Number(thread_id.into());
        }

        let inbound = |content: String| {
            InboundMessage::new(
                format!("tg_{}", message.message_id),
                ChannelType::Telegram,
                from.id.to_string(),
                conversation_id.clone(),
                content,
            )
            .with_sender_name(sender_name.clone().unwrap_or_default())
        };

        if let Some(text) = message.text {
            return Some(inbound(text).with_metadata(metadata));
        }

        if let Some(voice) = message.voice {
            let file_path = self
                .download_telegram_file(&voice.file_id)
                .await
                .ok()
                .flatten()?;
            metadata["media_type"] = serde_json::Value::String("voice".to_string());
            metadata["file_path"] = serde_json::Value::String(file_path.clone());
            let content = format!("[Voice message, {}s]", voice.duration);
            return Some(inbound(content).with_metadata(metadata));
        }

        if let Some(photos) = message.photo {
            let best = photos
                .iter()
                .max_by_key(|photo| photo.file_size.unwrap_or(0))?;
            let file_path = self
                .download_telegram_file(&best.file_id)
                .await
                .ok()
                .flatten()?;
            metadata["media_type"] = serde_json::Value::String("photo".to_string());
            metadata["file_path"] = serde_json::Value::String(file_path.clone());
            let caption = message.caption.clone();
            let content = match caption {
                Some(text) if !text.is_empty() => format!("[Photo] {}", text),
                _ => "[Photo]".to_string(),
            };
            return Some(inbound(content).with_metadata(metadata));
        }

        if let Some(video) = message.video {
            let file_path = self
                .download_telegram_file(&video.file_id)
                .await
                .ok()
                .flatten()?;
            metadata["media_type"] = serde_json::Value::String("video".to_string());
            metadata["file_path"] = serde_json::Value::String(file_path.clone());
            let caption = message.caption.clone();
            let content = match caption {
                Some(text) if !text.is_empty() => {
                    format!("[Video, {}s] {}", video.duration, text)
                }
                _ => format!("[Video, {}s]", video.duration),
            };
            return Some(inbound(content).with_metadata(metadata));
        }

        if let Some(video_note) = message.video_note {
            let file_path = self
                .download_telegram_file(&video_note.file_id)
                .await
                .ok()
                .flatten()?;
            metadata["media_type"] = serde_json::Value::String("video_note".to_string());
            metadata["file_path"] = serde_json::Value::String(file_path.clone());
            let content = format!("[Video note, {}s]", video_note.duration);
            return Some(inbound(content).with_metadata(metadata));
        }

        if let Some(document) = message.document {
            let file_path = self
                .download_telegram_file(&document.file_id)
                .await
                .ok()
                .flatten()?;
            metadata["media_type"] = serde_json::Value::String("document".to_string());
            metadata["file_path"] = serde_json::Value::String(file_path.clone());
            if let Some(file_name) = document.file_name.clone() {
                metadata["file_name"] = serde_json::Value::String(file_name.clone());
            }
            let caption = message.caption.clone();
            let label = document
                .file_name
                .clone()
                .unwrap_or_else(|| "document".to_string());
            let content = match caption {
                Some(text) if !text.is_empty() => format!("[Document: {}] {}", label, text),
                _ => format!("[Document: {}]", label),
            };
            return Some(inbound(content).with_metadata(metadata));
        }

        None
    }

    /// Test the connection by calling getMe
    pub async fn test_connection(&self) -> Result<TelegramUser> {
        let url = self.api_url("getMe");
        let response = self
            .client
            .get(&url)
            .timeout(std::time::Duration::from_secs(API_TIMEOUT_SECS))
            .send()
            .await?;

        let body: TelegramResponse<TelegramUser> = response.json().await?;

        if body.ok {
            Ok(body
                .result
                .ok_or_else(|| anyhow!("Telegram returned ok but no result"))?)
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

        let response = self
            .client
            .post(&url)
            .json(&params)
            .timeout(std::time::Duration::from_secs(API_TIMEOUT_SECS))
            .send()
            .await?;

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

        // Parse conversation_id to extract chat_id and thread_id
        let (chat_id, parsed_thread_id) = Self::parse_conversation_id(&message.conversation_id);

        // Use explicit message_thread_id if provided, otherwise use parsed thread_id
        let thread_id = message.message_thread_id.or(parsed_thread_id);

        let chunks = chunk_markdown(&formatted, None);
        for chunk in &chunks {
            self.send_message(
                &chat_id,
                chunk,
                parse_mode,
                message.reply_to.as_deref(),
                thread_id,
            )
            .await?;
        }

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
        let offset_persister = self.offset_persister.clone();
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
                offset_persister,
            };

            while polling_active.load(Ordering::SeqCst) {
                match channel.poll_updates().await {
                    Ok(updates) => {
                        for update in updates {
                            if let Some(message) = channel.convert_update(update).await {
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
struct TelegramFile {
    #[allow(dead_code)]
    file_id: String,
    #[allow(dead_code)]
    file_unique_id: String,
    #[allow(dead_code)]
    file_size: Option<i64>,
    file_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TelegramVoice {
    file_id: String,
    #[allow(dead_code)]
    file_unique_id: String,
    duration: u32,
    #[allow(dead_code)]
    mime_type: Option<String>,
    #[allow(dead_code)]
    file_size: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct TelegramPhotoSize {
    file_id: String,
    #[allow(dead_code)]
    file_unique_id: String,
    #[allow(dead_code)]
    width: u32,
    #[allow(dead_code)]
    height: u32,
    file_size: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct TelegramVideo {
    file_id: String,
    #[allow(dead_code)]
    file_unique_id: String,
    #[allow(dead_code)]
    width: u32,
    #[allow(dead_code)]
    height: u32,
    duration: u32,
    #[allow(dead_code)]
    mime_type: Option<String>,
    #[allow(dead_code)]
    file_size: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct TelegramDocument {
    file_id: String,
    #[allow(dead_code)]
    file_unique_id: String,
    file_name: Option<String>,
    #[allow(dead_code)]
    mime_type: Option<String>,
    #[allow(dead_code)]
    file_size: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct TelegramVideoNote {
    file_id: String,
    #[allow(dead_code)]
    file_unique_id: String,
    #[allow(dead_code)]
    length: u32,
    duration: u32,
    #[allow(dead_code)]
    file_size: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct TelegramMessage {
    message_id: i64,
    from: Option<TelegramUser>,
    chat: TelegramChat,
    #[allow(dead_code)]
    date: u64,
    message_thread_id: Option<i64>,
    text: Option<String>,
    caption: Option<String>,
    voice: Option<TelegramVoice>,
    photo: Option<Vec<TelegramPhotoSize>>,
    video: Option<TelegramVideo>,
    video_note: Option<TelegramVideoNote>,
    document: Option<TelegramDocument>,
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
    use std::sync::Arc;
    use std::sync::atomic::{AtomicI64, Ordering as AtomicOrdering};

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
    fn test_telegram_channel_restore_last_update_id() {
        let channel = TelegramChannel::with_token("test").with_last_update_id(123);
        assert_eq!(channel.last_update_id(), 123);
    }

    #[test]
    fn test_telegram_channel_offset_persister_callback() {
        let observed = Arc::new(AtomicI64::new(0));
        let observer = observed.clone();
        let persister: Arc<dyn Fn(i64) + Send + Sync> =
            Arc::new(move |value| observer.store(value, AtomicOrdering::SeqCst));

        let channel = TelegramChannel::with_token("test")
            .with_offset_persister(persister)
            .with_last_update_id(88);

        assert_eq!(channel.last_update_id(), 88);
        if let Some(callback) = &channel.offset_persister {
            callback(321);
        }
        assert_eq!(observed.load(AtomicOrdering::SeqCst), 321);
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

    #[tokio::test]
    async fn test_convert_update() {
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
                message_thread_id: None,
                text: Some("Hello world".to_string()),
                caption: None,
                voice: None,
                photo: None,
                video: None,
                video_note: None,
                document: None,
                reply_to_message: None,
            }),
        };

        let inbound = channel.convert_update(update).await.unwrap();
        assert_eq!(inbound.id, "tg_100");
        assert_eq!(inbound.sender_id, "42");
        assert_eq!(inbound.conversation_id, "999");
        assert_eq!(inbound.content, "Hello world");
        assert_eq!(inbound.sender_name, Some("johndoe".to_string()));
        assert_eq!(inbound.channel_type, ChannelType::Telegram);
    }

    #[tokio::test]
    async fn test_convert_update_no_username() {
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
                message_thread_id: None,
                text: Some("Hello".to_string()),
                caption: None,
                voice: None,
                photo: None,
                video: None,
                video_note: None,
                document: None,
                reply_to_message: None,
            }),
        };

        let inbound = channel.convert_update(update).await.unwrap();
        assert_eq!(inbound.sender_name, Some("John Doe".to_string()));
    }

    #[tokio::test]
    async fn test_convert_update_no_message() {
        let channel = TelegramChannel::with_token("test");

        let update = TelegramUpdate {
            update_id: 12345,
            message: None,
        };

        assert!(channel.convert_update(update).await.is_none());
    }

    #[tokio::test]
    async fn test_convert_update_no_text() {
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
                message_thread_id: None,
                text: None,
                caption: None,
                voice: None,
                photo: None,
                video: None,
                video_note: None,
                document: None,
                reply_to_message: None,
            }),
        };

        assert!(channel.convert_update(update).await.is_none());
    }

    #[tokio::test]
    async fn test_convert_update_forum_thread_conversation_id() {
        let channel = TelegramChannel::with_token("test");

        let update = TelegramUpdate {
            update_id: 22345,
            message: Some(TelegramMessage {
                message_id: 201,
                from: Some(TelegramUser {
                    id: 42,
                    is_bot: false,
                    first_name: Some("John".to_string()),
                    last_name: None,
                    username: Some("johndoe".to_string()),
                }),
                chat: TelegramChat {
                    id: -10012345,
                    r#type: "supergroup".to_string(),
                    title: Some("Forum".to_string()),
                    username: None,
                },
                date: 1234567890,
                message_thread_id: Some(7),
                text: Some("Thread message".to_string()),
                caption: None,
                voice: None,
                photo: None,
                video: None,
                video_note: None,
                document: None,
                reply_to_message: None,
            }),
        };

        let inbound = channel.convert_update(update).await.unwrap();
        assert_eq!(inbound.conversation_id, "-10012345:7");
        assert_eq!(
            inbound
                .metadata
                .unwrap()
                .get("message_thread_id")
                .and_then(|value| value.as_i64()),
            Some(7)
        );
    }

    #[tokio::test]
    async fn test_convert_update_voice() {
        let channel = TelegramChannel::with_token("test");

        let update = TelegramUpdate {
            update_id: 12345,
            message: Some(TelegramMessage {
                message_id: 101,
                from: Some(TelegramUser {
                    id: 7,
                    is_bot: false,
                    first_name: Some("Ada".to_string()),
                    last_name: None,
                    username: None,
                }),
                chat: TelegramChat {
                    id: 777,
                    r#type: "private".to_string(),
                    title: None,
                    username: None,
                },
                date: 1234567890,
                message_thread_id: None,
                text: None,
                caption: None,
                voice: Some(TelegramVoice {
                    file_id: "test-voice".to_string(),
                    file_unique_id: "voice-1".to_string(),
                    duration: 5,
                    mime_type: None,
                    file_size: Some(12),
                }),
                photo: None,
                video: None,
                video_note: None,
                document: None,
                reply_to_message: None,
            }),
        };

        let inbound = channel.convert_update(update).await.unwrap();
        assert_eq!(inbound.content, "[Voice message, 5s]");
        let metadata = inbound.metadata.unwrap();
        assert_eq!(metadata["media_type"], "voice");
        assert_eq!(metadata["file_path"], "/tmp/restflow-media/test-voice");
    }

    #[tokio::test]
    async fn test_convert_update_photo() {
        let channel = TelegramChannel::with_token("test");

        let update = TelegramUpdate {
            update_id: 12345,
            message: Some(TelegramMessage {
                message_id: 102,
                from: Some(TelegramUser {
                    id: 8,
                    is_bot: false,
                    first_name: Some("Lin".to_string()),
                    last_name: None,
                    username: None,
                }),
                chat: TelegramChat {
                    id: 888,
                    r#type: "private".to_string(),
                    title: None,
                    username: None,
                },
                date: 1234567890,
                message_thread_id: None,
                text: None,
                caption: Some("Look".to_string()),
                voice: None,
                photo: Some(vec![
                    TelegramPhotoSize {
                        file_id: "test-photo-small".to_string(),
                        file_unique_id: "photo-1".to_string(),
                        width: 90,
                        height: 90,
                        file_size: Some(10),
                    },
                    TelegramPhotoSize {
                        file_id: "test-photo-large".to_string(),
                        file_unique_id: "photo-2".to_string(),
                        width: 180,
                        height: 180,
                        file_size: Some(20),
                    },
                ]),
                video: None,
                video_note: None,
                document: None,
                reply_to_message: None,
            }),
        };

        let inbound = channel.convert_update(update).await.unwrap();
        assert_eq!(inbound.content, "[Photo] Look");
        let metadata = inbound.metadata.unwrap();
        assert_eq!(metadata["media_type"], "photo");
        assert_eq!(
            metadata["file_path"],
            "/tmp/restflow-media/test-photo-large"
        );
    }

    #[tokio::test]
    async fn test_convert_update_video() {
        let channel = TelegramChannel::with_token("test");

        let update = TelegramUpdate {
            update_id: 12345,
            message: Some(TelegramMessage {
                message_id: 103,
                from: Some(TelegramUser {
                    id: 9,
                    is_bot: false,
                    first_name: Some("Sam".to_string()),
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
                message_thread_id: None,
                text: None,
                caption: None,
                voice: None,
                photo: None,
                video: Some(TelegramVideo {
                    file_id: "test-video".to_string(),
                    file_unique_id: "video-1".to_string(),
                    width: 640,
                    height: 480,
                    duration: 12,
                    mime_type: None,
                    file_size: Some(100),
                }),
                video_note: None,
                document: None,
                reply_to_message: None,
            }),
        };

        let inbound = channel.convert_update(update).await.unwrap();
        assert_eq!(inbound.content, "[Video, 12s]");
        let metadata = inbound.metadata.unwrap();
        assert_eq!(metadata["media_type"], "video");
        assert_eq!(metadata["file_path"], "/tmp/restflow-media/test-video");
    }
}

// Add tests for thread reply functionality
#[cfg(test)]
mod thread_tests {
    use super::*;

    #[test]
    fn test_parse_conversation_id_simple() {
        let (chat_id, thread_id) = TelegramChannel::parse_conversation_id("123456789");
        assert_eq!(chat_id, "123456789");
        assert_eq!(thread_id, None);
    }

    #[test]
    fn test_parse_conversation_id_with_thread() {
        let (chat_id, thread_id) = TelegramChannel::parse_conversation_id("-10012345:7");
        assert_eq!(chat_id, "-10012345");
        assert_eq!(thread_id, Some(7));
    }

    #[test]
    fn test_parse_conversation_id_invalid_thread() {
        let (chat_id, thread_id) = TelegramChannel::parse_conversation_id("123456:invalid");
        assert_eq!(chat_id, "123456");
        assert_eq!(thread_id, None);
    }

    #[test]
    fn test_outbound_message_with_thread_id() {
        let msg = OutboundMessage::new("-10012345", "Test message").with_message_thread_id(7);

        assert_eq!(msg.conversation_id, "-10012345");
        assert_eq!(msg.message_thread_id, Some(7));
    }
}
