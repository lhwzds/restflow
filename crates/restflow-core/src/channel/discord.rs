//! Discord channel implementation.
//!
//! Uses the Discord Gateway WebSocket for receiving messages and REST API for sending.

use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::stream::StreamExt;
use reqwest::Client;
use serde_json::{Value, json};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use super::chunk::chunk_markdown;
use super::traits::Channel;
use super::types::{ChannelType, InboundMessage, OutboundMessage};

const DISCORD_API_BASE: &str = "https://discord.com/api/v10";
const DISCORD_MAX_MESSAGE_LEN: usize = 2000;

/// Intents: GUILDS (1) | GUILD_MESSAGES (512) | MESSAGE_CONTENT (32768) | DIRECT_MESSAGES (4096)
const GATEWAY_INTENTS: u64 = 1 | 512 | 4096 | 32768;

/// Discord channel configuration.
#[derive(Debug, Clone)]
pub struct DiscordConfig {
    pub bot_token: String,
    pub default_channel_id: Option<String>,
}

/// Discord channel that receives via Gateway WebSocket and sends via REST API.
pub struct DiscordChannel {
    config: DiscordConfig,
    client: Client,
    polling: Arc<AtomicBool>,
}

impl DiscordChannel {
    pub fn new(config: DiscordConfig) -> Self {
        Self {
            config,
            client: Client::new(),
            polling: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn with_token(token: &str) -> Self {
        Self::new(DiscordConfig {
            bot_token: token.to_string(),
            default_channel_id: None,
        })
    }

    pub fn with_default_channel(mut self, channel_id: String) -> Self {
        self.config.default_channel_id = Some(channel_id);
        self
    }

    /// Send a message to a Discord channel via REST API.
    async fn send_message(&self, channel_id: &str, text: &str) -> Result<()> {
        let chunks = chunk_markdown(text, Some(DISCORD_MAX_MESSAGE_LEN));
        for chunk in chunks {
            let resp = self
                .client
                .post(format!(
                    "{}/channels/{}/messages",
                    DISCORD_API_BASE, channel_id
                ))
                .header("Authorization", format!("Bot {}", self.config.bot_token))
                .json(&json!({ "content": chunk }))
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                warn!("Discord send failed ({}): {}", status, body);
            }
        }
        Ok(())
    }

    /// Start the Gateway WebSocket connection and return a message stream.
    fn start_gateway(
        &self,
    ) -> Option<Pin<Box<dyn tokio_stream::Stream<Item = InboundMessage> + Send>>> {
        let token = self.config.bot_token.clone();
        let client = self.client.clone();
        let polling = self.polling.clone();

        if polling.swap(true, Ordering::SeqCst) {
            warn!("Discord gateway already running");
            return None;
        }

        let (tx, rx) = mpsc::channel::<InboundMessage>(256);

        tokio::spawn(async move {
            let _guard = scopeguard::guard((), |_| {
                polling.store(false, Ordering::SeqCst);
            });

            // Get gateway URL
            let gateway_url = match Self::fetch_gateway_url(&client, &token).await {
                Ok(url) => url,
                Err(e) => {
                    error!("Failed to get Discord gateway URL: {}", e);
                    return;
                }
            };

            info!("Connecting to Discord Gateway: {}", gateway_url);

            let ws_stream = match tokio_tungstenite::connect_async(&gateway_url).await {
                Ok((stream, _)) => stream,
                Err(e) => {
                    error!("Failed to connect to Discord Gateway: {}", e);
                    return;
                }
            };

            let (mut ws_write, mut ws_read) = ws_stream.split();

            // Read Hello (opcode 10) to get heartbeat interval
            let heartbeat_interval = match ws_read.next().await {
                Some(Ok(msg)) => {
                    let text = msg.to_text().unwrap_or("{}");
                    let payload: Value = serde_json::from_str(text).unwrap_or_default();
                    if payload["op"].as_u64() == Some(10) {
                        payload["d"]["heartbeat_interval"].as_u64().unwrap_or(41250)
                    } else {
                        warn!("Expected Hello (op 10), got: {}", text);
                        41250
                    }
                }
                _ => {
                    error!("No Hello from Discord Gateway");
                    return;
                }
            };

            debug!("Discord heartbeat interval: {}ms", heartbeat_interval);

            // Send Identify (opcode 2)
            let identify = json!({
                "op": 2,
                "d": {
                    "token": token,
                    "intents": GATEWAY_INTENTS,
                    "properties": {
                        "os": "linux",
                        "browser": "restflow",
                        "device": "restflow"
                    }
                }
            });

            use futures::SinkExt;
            use tokio_tungstenite::tungstenite::Message as WsMessage;

            if let Err(e) = ws_write
                .send(WsMessage::Text(identify.to_string().into()))
                .await
            {
                error!("Failed to send Identify: {}", e);
                return;
            }

            // Spawn heartbeat task
            let heartbeat_write = Arc::new(tokio::sync::Mutex::new(ws_write));
            let hb_write = heartbeat_write.clone();
            let hb_polling = polling.clone();
            tokio::spawn(async move {
                let mut interval =
                    tokio::time::interval(std::time::Duration::from_millis(heartbeat_interval));
                loop {
                    interval.tick().await;
                    if !hb_polling.load(Ordering::SeqCst) {
                        break;
                    }
                    let heartbeat = json!({"op": 1, "d": null});
                    let mut writer = hb_write.lock().await;
                    if let Err(e) = writer
                        .send(WsMessage::Text(heartbeat.to_string().into()))
                        .await
                    {
                        warn!("Discord heartbeat failed: {}", e);
                        break;
                    }
                }
            });

            // Read messages
            while let Some(msg_result) = ws_read.next().await {
                if !polling.load(Ordering::SeqCst) {
                    break;
                }

                let msg = match msg_result {
                    Ok(m) => m,
                    Err(e) => {
                        warn!("Discord WebSocket error: {}", e);
                        break;
                    }
                };

                let text = match msg.to_text() {
                    Ok(t) => t,
                    Err(_) => continue,
                };

                let payload: Value = match serde_json::from_str(text) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                // Only handle MESSAGE_CREATE (type "t")
                if payload["t"].as_str() != Some("MESSAGE_CREATE") {
                    continue;
                }

                let data = &payload["d"];

                // Skip bot messages
                if data["author"]["bot"].as_bool() == Some(true) {
                    continue;
                }

                let message_id = match data["id"].as_str() {
                    Some(id) => id,
                    None => continue,
                };

                let content = data["content"].as_str().unwrap_or("");
                if content.is_empty() {
                    continue;
                }

                let channel_id = data["channel_id"].as_str().unwrap_or("");
                let author_id = data["author"]["id"].as_str().unwrap_or("");
                let author_name = data["author"]["username"].as_str().map(|s| s.to_string());

                // Build conversation ID (channel_id or channel_id:thread_id)
                let conversation_id = if let Some(thread_id) = data["message_reference"]
                    ["message_id"]
                    .as_str()
                {
                    format!("{}:{}", channel_id, thread_id)
                } else {
                    channel_id.to_string()
                };

                let mut inbound = InboundMessage::new(
                    format!("dc_{}", message_id),
                    ChannelType::Discord,
                    author_id,
                    &conversation_id,
                    content,
                );
                inbound.sender_name = author_name;

                if tx.send(inbound).await.is_err() {
                    debug!("Discord message channel closed");
                    break;
                }
            }

            info!("Discord gateway connection ended");
        });

        Some(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn fetch_gateway_url(client: &Client, token: &str) -> Result<String> {
        let resp = client
            .get(format!("{}/gateway/bot", DISCORD_API_BASE))
            .header("Authorization", format!("Bot {}", token))
            .send()
            .await
            .context("Failed to get Discord gateway URL")?;

        let body: Value = resp.json().await?;
        let url = body["url"]
            .as_str()
            .context("Missing 'url' in gateway response")?;
        Ok(format!("{}/?v=10&encoding=json", url))
    }
}

#[async_trait]
impl Channel for DiscordChannel {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Discord
    }

    fn is_configured(&self) -> bool {
        !self.config.bot_token.is_empty()
    }

    async fn send(&self, message: OutboundMessage) -> Result<()> {
        self.send_message(&message.conversation_id, &message.content)
            .await
    }

    fn start_receiving(
        &self,
    ) -> Option<Pin<Box<dyn tokio_stream::Stream<Item = InboundMessage> + Send>>> {
        self.start_gateway()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discord_config() {
        let config = DiscordConfig {
            bot_token: "test-token".to_string(),
            default_channel_id: Some("123456".to_string()),
        };
        assert_eq!(config.bot_token, "test-token");
        assert_eq!(config.default_channel_id, Some("123456".to_string()));
    }

    #[test]
    fn test_discord_channel_is_configured() {
        let channel = DiscordChannel::with_token("test-token");
        assert!(channel.is_configured());

        let empty = DiscordChannel::with_token("");
        assert!(!empty.is_configured());
    }

    #[test]
    fn test_discord_channel_type() {
        let channel = DiscordChannel::with_token("test");
        assert_eq!(channel.channel_type(), ChannelType::Discord);
    }

    #[test]
    fn test_message_id_format() {
        let id = format!("dc_{}", "1234567890");
        assert!(id.starts_with("dc_"));
    }

    #[test]
    fn test_conversation_id_with_thread() {
        let channel_id = "123";
        let thread_id = "456";
        let conv_id = format!("{}:{}", channel_id, thread_id);
        assert_eq!(conv_id, "123:456");
    }

    #[test]
    fn test_with_default_channel() {
        let ch = DiscordChannel::with_token("t").with_default_channel("ch123".into());
        assert_eq!(ch.config.default_channel_id, Some("ch123".to_string()));
    }

    #[test]
    fn test_with_token_constructor() {
        let ch = DiscordChannel::with_token("my-bot-token");
        assert_eq!(ch.config.bot_token, "my-bot-token");
        assert!(ch.config.default_channel_id.is_none());
    }

    #[test]
    fn test_not_configured_with_empty_token() {
        let ch = DiscordChannel::with_token("");
        assert!(!ch.is_configured());
    }

    #[test]
    fn test_gateway_intents() {
        // GUILDS=1, GUILD_MESSAGES=512, DIRECT_MESSAGES=4096, MESSAGE_CONTENT=32768
        assert_eq!(GATEWAY_INTENTS & 1, 1);
        assert_eq!(GATEWAY_INTENTS & 512, 512);
        assert_eq!(GATEWAY_INTENTS & 4096, 4096);
        assert_eq!(GATEWAY_INTENTS & 32768, 32768);
        assert_eq!(GATEWAY_INTENTS, 1 | 512 | 4096 | 32768);
    }

    #[test]
    fn test_max_message_len() {
        assert_eq!(DISCORD_MAX_MESSAGE_LEN, 2000);
    }

    #[test]
    fn test_channel_type_is_discord() {
        let ch = DiscordChannel::with_token("t");
        assert_eq!(ch.channel_type(), ChannelType::Discord);
    }

    #[test]
    fn test_gateway_prevents_double_start() {
        let ch = DiscordChannel::with_token("t");
        // Simulate first start by setting polling to true
        ch.polling.store(true, Ordering::SeqCst);
        // Second call should return None
        assert!(ch.start_gateway().is_none());
    }
}
