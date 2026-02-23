//! Slack channel implementation.
//!
//! Uses Slack Socket Mode (WebSocket) for receiving messages and Web API for sending.

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

const SLACK_API_BASE: &str = "https://slack.com/api";
const SLACK_MAX_MESSAGE_LEN: usize = 39000;

/// Slack channel configuration.
#[derive(Debug, Clone)]
pub struct SlackConfig {
    /// Bot User OAuth Token (xoxb-...)
    pub bot_token: String,
    /// App-Level Token for Socket Mode (xapp-...)
    pub app_token: String,
    /// Default channel ID for notifications.
    pub default_channel_id: Option<String>,
}

/// Slack channel using Socket Mode for receiving and Web API for sending.
pub struct SlackChannel {
    config: SlackConfig,
    client: Client,
    polling: Arc<AtomicBool>,
}

impl SlackChannel {
    pub fn new(config: SlackConfig) -> Self {
        Self {
            config,
            client: Client::new(),
            polling: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn with_tokens(bot_token: &str, app_token: &str) -> Self {
        Self::new(SlackConfig {
            bot_token: bot_token.to_string(),
            app_token: app_token.to_string(),
            default_channel_id: None,
        })
    }

    pub fn with_default_channel(mut self, channel_id: String) -> Self {
        self.config.default_channel_id = Some(channel_id);
        self
    }

    /// Send a message via Slack Web API.
    async fn send_message(
        &self,
        channel: &str,
        text: &str,
        thread_ts: Option<&str>,
    ) -> Result<()> {
        let chunks = chunk_markdown(text, Some(SLACK_MAX_MESSAGE_LEN));
        for chunk in chunks {
            let mut body = json!({
                "channel": channel,
                "text": chunk,
            });
            if let Some(ts) = thread_ts {
                body["thread_ts"] = json!(ts);
            }

            let resp = self
                .client
                .post(format!("{}/chat.postMessage", SLACK_API_BASE))
                .header(
                    "Authorization",
                    format!("Bearer {}", self.config.bot_token),
                )
                .json(&body)
                .send()
                .await?;

            let result: Value = resp.json().await?;
            if result["ok"].as_bool() != Some(true) {
                let err = result["error"].as_str().unwrap_or("unknown");
                warn!("Slack send failed: {}", err);
            }
        }
        Ok(())
    }

    /// Open a Socket Mode connection and return a message stream.
    fn start_socket_mode(
        &self,
    ) -> Option<Pin<Box<dyn tokio_stream::Stream<Item = InboundMessage> + Send>>> {
        let app_token = self.config.app_token.clone();
        let client = self.client.clone();
        let polling = self.polling.clone();

        if polling.swap(true, Ordering::SeqCst) {
            warn!("Slack Socket Mode already running");
            return None;
        }

        let (tx, rx) = mpsc::channel::<InboundMessage>(256);

        tokio::spawn(async move {
            let _guard = scopeguard::guard((), |_| {
                polling.store(false, Ordering::SeqCst);
            });

            // Get WebSocket URL via apps.connections.open
            let wss_url = match Self::open_connection(&client, &app_token).await {
                Ok(url) => url,
                Err(e) => {
                    error!("Failed to open Slack Socket Mode connection: {}", e);
                    return;
                }
            };

            info!("Connecting to Slack Socket Mode");

            let ws_stream = match tokio_tungstenite::connect_async(&wss_url).await {
                Ok((stream, _)) => stream,
                Err(e) => {
                    error!("Failed to connect to Slack Socket Mode: {}", e);
                    return;
                }
            };

            let (mut ws_write, mut ws_read) = ws_stream.split();

            // Read messages
            while let Some(msg_result) = ws_read.next().await {
                if !polling.load(Ordering::SeqCst) {
                    break;
                }

                let msg = match msg_result {
                    Ok(m) => m,
                    Err(e) => {
                        warn!("Slack WebSocket error: {}", e);
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

                let msg_type = payload["type"].as_str().unwrap_or("");

                // ACK all envelopes
                if let Some(envelope_id) = payload["envelope_id"].as_str() {
                    let ack = json!({"envelope_id": envelope_id});
                    use futures::SinkExt;
                    use tokio_tungstenite::tungstenite::Message as WsMessage;
                    if let Err(e) = ws_write
                        .send(WsMessage::Text(ack.to_string().into()))
                        .await
                    {
                        warn!("Failed to ACK Slack envelope: {}", e);
                    }
                }

                // Handle disconnect
                if msg_type == "disconnect" {
                    info!("Slack requested disconnect, will reconnect");
                    break;
                }

                // Only process events_api messages
                if msg_type != "events_api" {
                    continue;
                }

                let event = &payload["payload"]["event"];
                let event_type = event["type"].as_str().unwrap_or("");

                if event_type != "message" {
                    continue;
                }

                // Skip bot messages
                if event["bot_id"].as_str().is_some() {
                    continue;
                }

                // Skip message subtypes (edits, joins, etc.)
                if event["subtype"].as_str().is_some() {
                    continue;
                }

                let msg_text = event["text"].as_str().unwrap_or("");
                if msg_text.is_empty() {
                    continue;
                }

                let ts = event["ts"].as_str().unwrap_or("");
                let channel_id = event["channel"].as_str().unwrap_or("");
                let user_id = event["user"].as_str().unwrap_or("");

                // Build conversation ID (channel or channel:thread_ts)
                let conversation_id =
                    if let Some(thread_ts) = event["thread_ts"].as_str() {
                        format!("{}:{}", channel_id, thread_ts)
                    } else {
                        channel_id.to_string()
                    };

                let inbound = InboundMessage::new(
                    format!("sk_{}", ts),
                    ChannelType::Slack,
                    user_id,
                    &conversation_id,
                    msg_text,
                );

                if tx.send(inbound).await.is_err() {
                    debug!("Slack message channel closed");
                    break;
                }
            }

            info!("Slack Socket Mode connection ended");
        });

        Some(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn open_connection(client: &Client, app_token: &str) -> Result<String> {
        let resp = client
            .post(format!("{}/apps.connections.open", SLACK_API_BASE))
            .header("Authorization", format!("Bearer {}", app_token))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .send()
            .await
            .context("Failed to open Slack Socket Mode connection")?;

        let body: Value = resp.json().await?;
        if body["ok"].as_bool() != Some(true) {
            let err = body["error"].as_str().unwrap_or("unknown");
            anyhow::bail!("Slack apps.connections.open failed: {}", err);
        }

        body["url"]
            .as_str()
            .map(|s| s.to_string())
            .context("Missing 'url' in Slack connection response")
    }
}

#[async_trait]
impl Channel for SlackChannel {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Slack
    }

    fn is_configured(&self) -> bool {
        !self.config.bot_token.is_empty() && !self.config.app_token.is_empty()
    }

    async fn send(&self, message: OutboundMessage) -> Result<()> {
        // Parse thread_ts from conversation_id if present (format: channel:thread_ts)
        let parts: Vec<&str> = message.conversation_id.splitn(2, ':').collect();
        let channel = parts[0];
        let thread_ts = parts.get(1).copied();
        self.send_message(channel, &message.content, thread_ts)
            .await
    }

    fn start_receiving(
        &self,
    ) -> Option<Pin<Box<dyn tokio_stream::Stream<Item = InboundMessage> + Send>>> {
        self.start_socket_mode()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slack_config() {
        let config = SlackConfig {
            bot_token: "xoxb-test".to_string(),
            app_token: "xapp-test".to_string(),
            default_channel_id: Some("C123".to_string()),
        };
        assert_eq!(config.bot_token, "xoxb-test");
        assert_eq!(config.app_token, "xapp-test");
    }

    #[test]
    fn test_slack_channel_is_configured() {
        let channel = SlackChannel::with_tokens("xoxb-test", "xapp-test");
        assert!(channel.is_configured());

        let no_app = SlackChannel::with_tokens("xoxb-test", "");
        assert!(!no_app.is_configured());

        let no_bot = SlackChannel::with_tokens("", "xapp-test");
        assert!(!no_bot.is_configured());
    }

    #[test]
    fn test_slack_channel_type() {
        let channel = SlackChannel::with_tokens("bot", "app");
        assert_eq!(channel.channel_type(), ChannelType::Slack);
    }

    #[test]
    fn test_message_id_format() {
        let ts = "1234567890.123456";
        let id = format!("sk_{}", ts);
        assert!(id.starts_with("sk_"));
    }

    #[test]
    fn test_conversation_id_parsing() {
        let conv = "C123:1234567890.123456";
        let parts: Vec<&str> = conv.splitn(2, ':').collect();
        assert_eq!(parts[0], "C123");
        assert_eq!(parts[1], "1234567890.123456");
    }

    #[test]
    fn test_with_default_channel() {
        let ch = SlackChannel::with_tokens("bot", "app").with_default_channel("C456".into());
        assert_eq!(ch.config.default_channel_id, Some("C456".to_string()));
    }

    #[test]
    fn test_with_tokens_constructor() {
        let ch = SlackChannel::with_tokens("xoxb-bot", "xapp-app");
        assert_eq!(ch.config.bot_token, "xoxb-bot");
        assert_eq!(ch.config.app_token, "xapp-app");
        assert!(ch.config.default_channel_id.is_none());
    }

    #[test]
    fn test_requires_both_tokens() {
        assert!(!SlackChannel::with_tokens("bot", "").is_configured());
        assert!(!SlackChannel::with_tokens("", "app").is_configured());
        assert!(!SlackChannel::with_tokens("", "").is_configured());
        assert!(SlackChannel::with_tokens("bot", "app").is_configured());
    }

    #[test]
    fn test_max_message_len() {
        assert_eq!(SLACK_MAX_MESSAGE_LEN, 39000);
    }

    #[test]
    fn test_conversation_id_without_thread() {
        let conv = "C123";
        let parts: Vec<&str> = conv.splitn(2, ':').collect();
        assert_eq!(parts[0], "C123");
        assert_eq!(parts.get(1), None);
    }

    #[test]
    fn test_conversation_id_with_colon_in_thread_ts() {
        // splitn(2, ':') should only split on the first colon
        let conv = "C123:1234567890.123456";
        let parts: Vec<&str> = conv.splitn(2, ':').collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], "C123");
        assert_eq!(parts[1], "1234567890.123456");
    }

    #[test]
    fn test_socket_mode_prevents_double_start() {
        let ch = SlackChannel::with_tokens("bot", "app");
        // Simulate first start by setting polling to true
        ch.polling.store(true, Ordering::SeqCst);
        // Second call should return None
        assert!(ch.start_socket_mode().is_none());
    }
}
