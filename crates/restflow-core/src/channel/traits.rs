//! Channel Trait Definitions
//!
//! Defines the core traits for implementing communication channels.

use anyhow::Result;
use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

use super::types::{ChannelType, InboundMessage, OutboundMessage};

/// Universal communication channel trait
///
/// This trait defines the interface for any communication channel (Telegram, Discord, etc.)
/// that can send and optionally receive messages.
///
/// # Example
///
/// ```ignore
/// struct MyChannel { /* ... */ }
///
/// #[async_trait]
/// impl Channel for MyChannel {
///     fn channel_type(&self) -> ChannelType {
///         ChannelType::Telegram
///     }
///
///     fn is_configured(&self) -> bool {
///         !self.token.is_empty()
///     }
///
///     async fn send(&self, message: OutboundMessage) -> Result<()> {
///         // Send message via API
///         Ok(())
///     }
///
///     fn start_receiving(&self) -> Option<Pin<Box<dyn Stream<Item = InboundMessage> + Send>>> {
///         // Return a message stream or None if not supported
///         None
///     }
/// }
/// ```
#[async_trait]
pub trait Channel: Send + Sync {
    /// Get channel type
    fn channel_type(&self) -> ChannelType;

    /// Get channel display name
    fn name(&self) -> &str {
        self.channel_type().display_name()
    }

    /// Check if channel is properly configured
    fn is_configured(&self) -> bool;

    /// Check if channel supports bidirectional interaction
    fn supports_interaction(&self) -> bool {
        self.channel_type().supports_interaction()
    }

    /// Send a message to the channel
    async fn send(&self, message: OutboundMessage) -> Result<()>;

    /// Send a simple text message
    async fn send_text(&self, conversation_id: &str, text: &str) -> Result<()> {
        self.send(OutboundMessage::new(conversation_id, text)).await
    }

    /// Start receiving messages (returns None if channel doesn't support receiving)
    ///
    /// The returned stream should be spawned in a background task.
    /// Messages are yielded as they arrive from the channel.
    fn start_receiving(&self) -> Option<Pin<Box<dyn Stream<Item = InboundMessage> + Send>>>;
}

/// Channel that supports webhook-style message receiving
///
/// Implement this trait for channels that receive messages via HTTP webhooks
/// (e.g., Telegram webhook mode, Slack events API).
#[async_trait]
pub trait WebhookReceiver: Channel {
    /// Handle incoming webhook payload
    ///
    /// # Arguments
    /// * `payload` - Raw HTTP request body
    /// * `headers` - HTTP headers as key-value pairs
    ///
    /// # Returns
    /// Parsed inbound messages (may be empty if payload is not a message event)
    async fn handle_webhook(
        &self,
        payload: &[u8],
        headers: &[(String, String)],
    ) -> Result<Vec<InboundMessage>>;

    /// Verify webhook signature if applicable
    fn verify_signature(&self, payload: &[u8], signature: &str) -> bool {
        // Default: no verification
        let _ = (payload, signature);
        true
    }
}

/// Channel that supports long-polling or websocket receiving
///
/// Implement this trait for channels that actively poll for messages
/// (e.g., Telegram getUpdates, Discord Gateway).
#[async_trait]
pub trait StreamReceiver: Channel {
    /// Start the message polling/streaming loop
    ///
    /// This should spawn a background task that polls for messages
    /// and feeds them to the stream returned by `start_receiving()`.
    async fn start_polling(&self) -> Result<()>;

    /// Stop the message polling/streaming loop
    async fn stop_polling(&self) -> Result<()>;

    /// Check if polling is currently active
    fn is_polling(&self) -> bool;
}

/// Test/mock channel for unit testing
#[cfg(test)]
pub mod mock {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tokio::sync::mpsc;

    /// A mock channel for testing
    pub struct MockChannel {
        channel_type: ChannelType,
        configured: AtomicBool,
        sent_messages: Arc<tokio::sync::Mutex<Vec<OutboundMessage>>>,
        message_tx: Option<mpsc::UnboundedSender<InboundMessage>>,
    }

    impl MockChannel {
        /// Create a new mock channel
        pub fn new(channel_type: ChannelType) -> Self {
            Self {
                channel_type,
                configured: AtomicBool::new(true),
                sent_messages: Arc::new(tokio::sync::Mutex::new(Vec::new())),
                message_tx: None,
            }
        }

        /// Create an unconfigured mock channel
        pub fn unconfigured(channel_type: ChannelType) -> Self {
            let channel = Self::new(channel_type);
            channel.configured.store(false, Ordering::SeqCst);
            channel
        }

        /// Get all sent messages
        pub async fn get_sent_messages(&self) -> Vec<OutboundMessage> {
            self.sent_messages.lock().await.clone()
        }

        /// Clear sent messages
        pub async fn clear_sent_messages(&self) {
            self.sent_messages.lock().await.clear();
        }

        /// Enable receiving and return a sender to inject messages
        pub fn enable_receiving(&mut self) -> mpsc::UnboundedSender<InboundMessage> {
            let (tx, _rx) = mpsc::unbounded_channel();
            self.message_tx = Some(tx.clone());
            tx
        }
    }

    #[async_trait]
    impl Channel for MockChannel {
        fn channel_type(&self) -> ChannelType {
            self.channel_type
        }

        fn is_configured(&self) -> bool {
            self.configured.load(Ordering::SeqCst)
        }

        async fn send(&self, message: OutboundMessage) -> Result<()> {
            self.sent_messages.lock().await.push(message);
            Ok(())
        }

        fn start_receiving(&self) -> Option<Pin<Box<dyn Stream<Item = InboundMessage> + Send>>> {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mock::MockChannel;

    #[tokio::test]
    async fn test_mock_channel_send() {
        let channel = MockChannel::new(ChannelType::Telegram);
        
        let msg = OutboundMessage::new("chat-123", "Hello");
        channel.send(msg).await.unwrap();
        
        let sent = channel.get_sent_messages().await;
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].content, "Hello");
    }

    #[tokio::test]
    async fn test_mock_channel_unconfigured() {
        let channel = MockChannel::unconfigured(ChannelType::Discord);
        assert!(!channel.is_configured());
    }

    #[tokio::test]
    async fn test_channel_defaults() {
        let channel = MockChannel::new(ChannelType::Telegram);
        
        assert_eq!(channel.name(), "Telegram");
        assert!(channel.supports_interaction());
    }

    #[tokio::test]
    async fn test_send_text_convenience() {
        let channel = MockChannel::new(ChannelType::Telegram);
        
        channel.send_text("chat-456", "Quick message").await.unwrap();
        
        let sent = channel.get_sent_messages().await;
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].conversation_id, "chat-456");
        assert_eq!(sent[0].content, "Quick message");
    }
}
