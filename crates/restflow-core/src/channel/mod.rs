//! Universal Communication Channel Layer
//!
//! This module provides a channel-agnostic communication layer for RestFlow.
//! It supports multiple communication channels (Telegram, Discord, Slack, etc.)
//! with both outbound (notifications) and inbound (user interaction) capabilities.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │            ChannelRouter                │
//! │  - Routes messages to correct channel   │
//! │  - Tracks conversation context          │
//! └─────────────────────────────────────────┘
//!              │
//!              ▼
//! ┌─────────────────────────────────────────┐
//! │         trait Channel                   │
//! │  - send(message)                        │
//! │  - start_receiving() -> Stream          │
//! └─────────────────────────────────────────┘
//!              │
//!    ┌─────────┼─────────┐
//!    ▼         ▼         ▼
//! Telegram  Discord   Slack  ...
//! ```
//!
//! # Features
//!
//! - **Universal Message Types**: `InboundMessage` and `OutboundMessage` work across all channels
//! - **Conversation Context**: Automatic tracking of which channel a user is using
//! - **Auto-Routing**: Reply to conversations without specifying the channel
//! - **Message Levels**: Info, Success, Warning, Error with appropriate formatting
//!
//! # Usage
//!
//! ## Basic Setup
//!
//! ```ignore
//! use restflow_core::channel::{ChannelRouter, ChannelType, OutboundMessage};
//!
//! // Create router and register channels
//! let mut router = ChannelRouter::new();
//! router.register(TelegramChannel::new(bot_token));
//!
//! // Send message to specific channel
//! let message = OutboundMessage::success("chat-123", "Task completed!");
//! router.send_to(ChannelType::Telegram, message).await?;
//! ```
//!
//! ## Auto-Routing Replies
//!
//! ```ignore
//! // When you receive an inbound message, record its context
//! router.record_conversation(&inbound_message, Some("task-id")).await;
//!
//! // Later, reply without specifying channel - it's auto-detected
//! router.reply("chat-123", "Processing your request...").await?;
//! ```
//!
//! ## Implementing a Channel
//!
//! ```ignore
//! use restflow_core::channel::{Channel, ChannelType, InboundMessage, OutboundMessage};
//!
//! struct MyChannel { /* config */ }
//!
//! #[async_trait]
//! impl Channel for MyChannel {
//!     fn channel_type(&self) -> ChannelType {
//!         ChannelType::Telegram // or appropriate type
//!     }
//!
//!     fn is_configured(&self) -> bool {
//!         // Check if credentials are set
//!         !self.token.is_empty()
//!     }
//!
//!     async fn send(&self, message: OutboundMessage) -> Result<()> {
//!         // Send via channel API
//!         Ok(())
//!     }
//!
//!     fn start_receiving(&self) -> Option<Pin<Box<dyn Stream<Item = InboundMessage> + Send>>> {
//!         // Return message stream or None
//!         None
//!     }
//! }
//! ```

mod reply_sender;
mod router;
pub mod telegram;
mod traits;
mod types;

pub use reply_sender::ChannelReplySender;
pub use router::ChannelRouter;
pub use telegram::{TelegramChannel, TelegramConfig};
pub use traits::{Channel, StreamReceiver, WebhookReceiver};
pub use types::{ChannelType, ConversationContext, InboundMessage, MessageLevel, OutboundMessage};

#[cfg(test)]
pub use traits::mock;
