//! Channel Router - Multi-channel message routing
//!
//! Routes messages to appropriate channels and tracks conversation context.

use anyhow::{Result, anyhow};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::traits::Channel;
use super::types::{
    ChannelType, ConversationContext, InboundMessage, MessageLevel, OutboundMessage,
};

/// Multi-channel router for sending and receiving messages
///
/// The router maintains a registry of channels and tracks conversation context
/// to enable automatic routing of replies to the correct channel.
///
/// # Example
///
/// ```ignore
/// use restflow_core::channel::{ChannelRouter, ChannelType, OutboundMessage};
///
/// let mut router = ChannelRouter::new();
/// router.register(telegram_channel);
///
/// // Send to specific channel
/// router.send_to(ChannelType::Telegram, message).await?;
///
/// // Reply to conversation (auto-routes to correct channel)
/// router.reply("conversation-id", "Hello!").await?;
/// ```
pub struct ChannelRouter {
    /// Registered channels
    channels: HashMap<ChannelType, Arc<dyn Channel>>,
    /// Conversation context cache
    conversations: Arc<RwLock<HashMap<String, ConversationContext>>>,
    /// Default conversation IDs per channel (for broadcasts)
    default_conversations: HashMap<ChannelType, String>,
}

impl ChannelRouter {
    /// Create a new channel router
    pub fn new() -> Self {
        Self {
            channels: HashMap::new(),
            conversations: Arc::new(RwLock::new(HashMap::new())),
            default_conversations: HashMap::new(),
        }
    }

    /// Register a channel
    ///
    /// If a channel of the same type already exists, it will be replaced.
    pub fn register<C: Channel + 'static>(&mut self, channel: C) {
        let channel_type = channel.channel_type();
        info!("Registering channel: {:?}", channel_type);
        self.channels.insert(channel_type, Arc::new(channel));
    }

    /// Register a channel with a default conversation ID
    ///
    /// The default conversation is used for broadcasts to this channel.
    pub fn register_with_default<C: Channel + 'static>(
        &mut self,
        channel: C,
        default_conversation: impl Into<String>,
    ) {
        let channel_type = channel.channel_type();
        self.default_conversations
            .insert(channel_type, default_conversation.into());
        self.register(channel);
    }

    /// Check whether a channel has a configured default conversation ID.
    pub fn has_default_conversation(&self, channel_type: ChannelType) -> bool {
        self.default_conversations.contains_key(&channel_type)
    }

    /// Get a channel by type
    pub fn get(&self, channel_type: ChannelType) -> Option<&Arc<dyn Channel>> {
        self.channels.get(&channel_type)
    }

    /// Check if a channel is registered and configured
    pub fn is_available(&self, channel_type: ChannelType) -> bool {
        self.channels
            .get(&channel_type)
            .map(|c| c.is_configured())
            .unwrap_or(false)
    }

    /// Send message to a specific channel
    pub async fn send_to(&self, channel_type: ChannelType, message: OutboundMessage) -> Result<()> {
        let channel = self
            .channels
            .get(&channel_type)
            .ok_or_else(|| anyhow!("Channel {:?} not registered", channel_type))?;

        if !channel.is_configured() {
            return Err(anyhow!("Channel {:?} not configured", channel_type));
        }

        debug!(
            "Sending message to {:?} (conversation={})",
            channel_type, message.conversation_id
        );
        channel.send(message).await
    }

    /// Send text to the configured default conversation of a channel.
    pub async fn send_to_default(&self, channel_type: ChannelType, content: &str) -> Result<()> {
        let conversation_id = self
            .default_conversations
            .get(&channel_type)
            .ok_or_else(|| anyhow!("No default conversation configured for {:?}", channel_type))?;

        let message = OutboundMessage::new(conversation_id, content);
        self.send_to(channel_type, message).await
    }

    /// Send typing indicator to a specific channel
    pub async fn send_typing_to(
        &self,
        channel_type: ChannelType,
        conversation_id: &str,
    ) -> Result<()> {
        let channel = self
            .channels
            .get(&channel_type)
            .ok_or_else(|| anyhow!("Channel {:?} not registered", channel_type))?;

        if !channel.is_configured() {
            return Err(anyhow!("Channel {:?} not configured", channel_type));
        }

        channel.send_typing(conversation_id).await
    }

    /// Reply to a conversation (auto-selects the channel based on context)
    ///
    /// This method looks up the conversation context to determine which channel
    /// to use for the reply.
    pub async fn reply(&self, conversation_id: &str, content: &str) -> Result<()> {
        let channel_type = {
            let conversations = self.conversations.read().await;
            let context = conversations
                .get(conversation_id)
                .ok_or_else(|| anyhow!("Unknown conversation: {}", conversation_id))?;
            context.channel_type
        };

        let message = OutboundMessage::new(conversation_id, content);
        self.send_to(channel_type, message).await
    }

    /// Reply with a structured message
    pub async fn reply_message(
        &self,
        conversation_id: &str,
        message: OutboundMessage,
    ) -> Result<()> {
        let conversations = self.conversations.read().await;
        let context = conversations
            .get(conversation_id)
            .ok_or_else(|| anyhow!("Unknown conversation: {}", conversation_id))?;

        let channel_type = context.channel_type;
        drop(conversations); // Release lock before sending

        self.send_to(channel_type, message).await
    }

    /// Broadcast message to all configured channels
    ///
    /// Uses default conversation IDs where configured.
    /// Returns a list of (channel_type, result) pairs.
    pub async fn broadcast(
        &self,
        content: &str,
        level: MessageLevel,
    ) -> Vec<(ChannelType, Result<()>)> {
        let mut results = vec![];

        for (channel_type, channel) in &self.channels {
            if !channel.is_configured() {
                continue;
            }

            let target_conversations = self.resolve_targets(*channel_type).await;

            if target_conversations.is_empty() {
                warn!(
                    "Skipping broadcast to {:?} - no default or known conversations",
                    channel_type
                );
                continue;
            }

            for conversation_id in target_conversations {
                let mut message = OutboundMessage::new(&conversation_id, content).with_level(level);
                // Broadcast payloads often contain raw agent output; disable parse mode
                // to avoid Telegram entity parsing failures that drop notifications.
                message.parse_mode = None;
                let result = channel.send(message).await;
                results.push((*channel_type, result));
            }
        }

        results
    }

    /// Broadcast typing indicator to all configured channels.
    ///
    /// Uses default conversation IDs where configured.
    /// Returns a list of (channel_type, result) pairs.
    pub async fn broadcast_typing(&self) -> Vec<(ChannelType, Result<()>)> {
        let mut results = vec![];

        for (channel_type, channel) in &self.channels {
            if !channel.is_configured() {
                continue;
            }

            let target_conversations = self.resolve_targets(*channel_type).await;
            if target_conversations.is_empty() {
                continue;
            }

            for conversation_id in target_conversations {
                let result = channel.send_typing(&conversation_id).await;
                results.push((*channel_type, result));
            }
        }

        results
    }

    async fn resolve_targets(&self, channel_type: ChannelType) -> Vec<String> {
        if let Some(default_conv) = self.default_conversations.get(&channel_type) {
            return vec![default_conv.clone()];
        }
        if channel_type == ChannelType::Telegram {
            warn!(
                "Skipping Telegram broadcast - no default conversation configured (set TELEGRAM_CHAT_ID)"
            );
            return Vec::new();
        }

        let conversations = self.conversations.read().await;
        let mut targets = HashSet::new();
        for (conversation_id, context) in conversations.iter() {
            if context.channel_type == channel_type {
                targets.insert(conversation_id.clone());
            }
        }
        targets.into_iter().collect::<Vec<_>>()
    }

    /// Record conversation context when receiving a message
    ///
    /// This should be called when processing inbound messages to enable
    /// auto-routing of replies.
    pub async fn record_conversation(&self, message: &InboundMessage, task_id: Option<String>) {
        let mut conversations = self.conversations.write().await;
        let existing_task_id = if task_id.is_none() {
            conversations
                .get(&message.conversation_id)
                .and_then(|ctx| ctx.task_id.clone())
        } else {
            None
        };

        let mut context = ConversationContext::new(
            &message.conversation_id,
            message.channel_type,
            &message.sender_id,
        );

        if let Some(tid) = task_id.or(existing_task_id) {
            context = context.with_task_id(tid);
        }

        conversations.insert(message.conversation_id.clone(), context);
    }

    /// Update task association for a conversation
    pub async fn associate_task(&self, conversation_id: &str, task_id: &str) -> Result<()> {
        let mut conversations = self.conversations.write().await;
        let context = conversations
            .get_mut(conversation_id)
            .ok_or_else(|| anyhow!("Unknown conversation: {}", conversation_id))?;

        context.task_id = Some(task_id.to_string());
        context.touch();
        Ok(())
    }

    /// Clear task association for a conversation
    pub async fn clear_task(&self, conversation_id: &str) -> Result<()> {
        let mut conversations = self.conversations.write().await;
        if let Some(context) = conversations.get_mut(conversation_id) {
            context.task_id = None;
            context.touch();
        }
        Ok(())
    }

    /// Get conversation context
    pub async fn get_conversation(&self, conversation_id: &str) -> Option<ConversationContext> {
        self.conversations
            .read()
            .await
            .get(conversation_id)
            .cloned()
    }

    /// Find conversation by task ID
    pub async fn find_conversation_by_task(&self, task_id: &str) -> Option<ConversationContext> {
        self.conversations
            .read()
            .await
            .values()
            .find(|c| c.task_id.as_deref() == Some(task_id))
            .cloned()
    }

    /// Remove stale conversations older than max_age_ms
    pub async fn cleanup_stale_conversations(&self, max_age_ms: i64) -> usize {
        let mut conversations = self.conversations.write().await;
        let before = conversations.len();
        conversations.retain(|_, ctx| !ctx.is_stale(max_age_ms));
        let removed = before - conversations.len();

        if removed > 0 {
            debug!("Cleaned up {} stale conversations", removed);
        }

        removed
    }

    /// List all configured and ready channels
    pub fn list_configured(&self) -> Vec<ChannelType> {
        self.channels
            .iter()
            .filter(|(_, c)| c.is_configured())
            .map(|(t, _)| *t)
            .collect()
    }

    /// List channels that support interaction (bidirectional)
    pub fn list_interactive(&self) -> Vec<ChannelType> {
        self.channels
            .iter()
            .filter(|(_, c)| c.is_configured() && c.supports_interaction())
            .map(|(t, _)| *t)
            .collect()
    }

    /// Get total number of active conversations
    pub async fn conversation_count(&self) -> usize {
        self.conversations.read().await.len()
    }

    /// Get number of registered channels
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }
}

impl Default for ChannelRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::traits::mock::MockChannel;
    use async_trait::async_trait;
    use futures::Stream;
    use std::pin::Pin;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    struct CaptureChannel {
        channel_type: ChannelType,
        sent: Arc<Mutex<Vec<OutboundMessage>>>,
        typing: Arc<Mutex<Vec<String>>>,
    }

    impl CaptureChannel {
        fn new(
            channel_type: ChannelType,
            sent: Arc<Mutex<Vec<OutboundMessage>>>,
            typing: Arc<Mutex<Vec<String>>>,
        ) -> Self {
            Self {
                channel_type,
                sent,
                typing,
            }
        }
    }

    #[async_trait]
    impl Channel for CaptureChannel {
        fn channel_type(&self) -> ChannelType {
            self.channel_type
        }

        fn is_configured(&self) -> bool {
            true
        }

        async fn send(&self, message: OutboundMessage) -> Result<()> {
            self.sent.lock().await.push(message);
            Ok(())
        }

        async fn send_typing(&self, conversation_id: &str) -> Result<()> {
            self.typing.lock().await.push(conversation_id.to_string());
            Ok(())
        }

        fn start_receiving(&self) -> Option<Pin<Box<dyn Stream<Item = InboundMessage> + Send>>> {
            None
        }
    }

    #[tokio::test]
    async fn test_router_registration() {
        let mut router = ChannelRouter::new();
        router.register(MockChannel::new(ChannelType::Telegram));

        assert!(router.get(ChannelType::Telegram).is_some());
        assert!(router.get(ChannelType::Discord).is_none());
        assert!(router.is_available(ChannelType::Telegram));
    }

    #[tokio::test]
    async fn test_send_to_channel() {
        let mut router = ChannelRouter::new();
        let channel = MockChannel::new(ChannelType::Telegram);
        router.register(channel);

        let message = OutboundMessage::new("chat-123", "Hello");
        router
            .send_to(ChannelType::Telegram, message)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_send_to_default_channel() {
        let mut router = ChannelRouter::new();
        let channel = MockChannel::new(ChannelType::Telegram);
        router.register_with_default(channel, "chat-default");

        router
            .send_to_default(ChannelType::Telegram, "Hello")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_send_to_unregistered_channel() {
        let router = ChannelRouter::new();
        let message = OutboundMessage::new("chat-123", "Hello");

        let result = router.send_to(ChannelType::Telegram, message).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_send_to_unconfigured_channel() {
        let mut router = ChannelRouter::new();
        router.register(MockChannel::unconfigured(ChannelType::Telegram));

        let message = OutboundMessage::new("chat-123", "Hello");
        let result = router.send_to(ChannelType::Telegram, message).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_conversation_tracking() {
        let router = ChannelRouter::new();

        let inbound = InboundMessage::new(
            "msg-1",
            ChannelType::Telegram,
            "user-123",
            "chat-456",
            "Hello",
        );

        router
            .record_conversation(&inbound, Some("task-1".to_string()))
            .await;

        let context = router.get_conversation("chat-456").await.unwrap();
        assert_eq!(context.channel_type, ChannelType::Telegram);
        assert_eq!(context.user_id, "user-123");
        assert_eq!(context.task_id, Some("task-1".to_string()));
    }

    #[tokio::test]
    async fn test_conversation_tracking_telegram_thread_does_not_inherit_task_from_main_chat() {
        let router = ChannelRouter::new();

        let legacy = InboundMessage::new(
            "msg-1",
            ChannelType::Telegram,
            "user-123",
            "chat-456",
            "Hello",
        );
        router
            .record_conversation(&legacy, Some("task-1".to_string()))
            .await;

        let thread_message = InboundMessage::new(
            "msg-2",
            ChannelType::Telegram,
            "user-123",
            "chat-456:9",
            "Hello from thread",
        );
        router.record_conversation(&thread_message, None).await;

        let context = router.get_conversation("chat-456:9").await.unwrap();
        assert_eq!(context.task_id, None);
    }

    #[tokio::test]
    async fn test_conversation_tracking_telegram_main_chat_does_not_inherit_task_from_thread() {
        let router = ChannelRouter::new();

        let thread = InboundMessage::new(
            "msg-1",
            ChannelType::Telegram,
            "user-123",
            "chat-456:9",
            "Hello from thread",
        );
        router
            .record_conversation(&thread, Some("task-1".to_string()))
            .await;

        let main_chat = InboundMessage::new(
            "msg-2",
            ChannelType::Telegram,
            "user-123",
            "chat-456",
            "Hello from main chat",
        );
        router.record_conversation(&main_chat, None).await;

        let context = router.get_conversation("chat-456").await.unwrap();
        assert_eq!(context.task_id, None);
    }

    #[tokio::test]
    async fn test_reply_auto_routing() {
        let mut router = ChannelRouter::new();
        router.register(MockChannel::new(ChannelType::Telegram));

        // First, record a conversation
        let inbound = InboundMessage::new(
            "msg-1",
            ChannelType::Telegram,
            "user-123",
            "chat-456",
            "Hello",
        );
        router.record_conversation(&inbound, None).await;

        // Now reply - should auto-route to Telegram
        router.reply("chat-456", "Hi back!").await.unwrap();
    }

    #[tokio::test]
    async fn test_reply_unknown_conversation() {
        let router = ChannelRouter::new();

        let result = router.reply("unknown-conv", "Hello").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_find_conversation_by_task() {
        let router = ChannelRouter::new();

        let inbound = InboundMessage::new(
            "msg-1",
            ChannelType::Telegram,
            "user-123",
            "chat-456",
            "Hello",
        );
        router
            .record_conversation(&inbound, Some("task-99".to_string()))
            .await;

        let found = router.find_conversation_by_task("task-99").await;
        assert!(found.is_some());
        assert_eq!(found.unwrap().conversation_id, "chat-456");

        let not_found = router.find_conversation_by_task("task-nonexistent").await;
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_associate_and_clear_task() {
        let router = ChannelRouter::new();

        // Record conversation without task
        let inbound = InboundMessage::new(
            "msg-1",
            ChannelType::Telegram,
            "user-123",
            "chat-456",
            "Hello",
        );
        router.record_conversation(&inbound, None).await;

        // Associate task
        router.associate_task("chat-456", "task-1").await.unwrap();
        let ctx = router.get_conversation("chat-456").await.unwrap();
        assert_eq!(ctx.task_id, Some("task-1".to_string()));

        // Clear task
        router.clear_task("chat-456").await.unwrap();
        let ctx = router.get_conversation("chat-456").await.unwrap();
        assert_eq!(ctx.task_id, None);
    }

    #[tokio::test]
    async fn test_list_channels() {
        let mut router = ChannelRouter::new();
        router.register(MockChannel::new(ChannelType::Telegram));
        router.register(MockChannel::new(ChannelType::Discord));
        router.register(MockChannel::unconfigured(ChannelType::Slack));
        router.register(MockChannel::new(ChannelType::Email));

        let configured = router.list_configured();
        assert_eq!(configured.len(), 3); // Telegram, Discord, Email

        let interactive = router.list_interactive();
        assert_eq!(interactive.len(), 2); // Telegram, Discord (Email doesn't support interaction)
    }

    #[tokio::test]
    async fn test_broadcast_with_defaults() {
        let mut router = ChannelRouter::new();
        router.register_with_default(MockChannel::new(ChannelType::Telegram), "default-chat");

        let results = router.broadcast("Announcement", MessageLevel::Info).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].1.is_ok());
    }

    #[tokio::test]
    async fn test_broadcast_without_default_uses_recorded_conversations() {
        let mut router = ChannelRouter::new();
        router.register(MockChannel::new(ChannelType::Discord));

        let inbound_a =
            InboundMessage::new("msg-1", ChannelType::Discord, "user-1", "chat-1", "Hello");
        let inbound_b =
            InboundMessage::new("msg-2", ChannelType::Discord, "user-2", "chat-2", "Hi");
        router.record_conversation(&inbound_a, None).await;
        router.record_conversation(&inbound_b, None).await;

        let results = router.broadcast("Announcement", MessageLevel::Info).await;
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|(_, result)| result.is_ok()));
    }

    #[tokio::test]
    async fn test_broadcast_without_default_skips_telegram_recorded_conversations() {
        let mut router = ChannelRouter::new();
        router.register(MockChannel::new(ChannelType::Telegram));

        let inbound_a =
            InboundMessage::new("msg-1", ChannelType::Telegram, "user-1", "chat-1", "Hello");
        let inbound_b =
            InboundMessage::new("msg-2", ChannelType::Telegram, "user-2", "chat-2", "Hi");
        router.record_conversation(&inbound_a, None).await;
        router.record_conversation(&inbound_b, None).await;

        let results = router.broadcast("Announcement", MessageLevel::Info).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_broadcast_uses_plain_parse_mode() {
        let mut router = ChannelRouter::new();
        let sent = Arc::new(Mutex::new(Vec::new()));
        let typing = Arc::new(Mutex::new(Vec::new()));
        router.register_with_default(
            CaptureChannel::new(ChannelType::Telegram, Arc::clone(&sent), typing),
            "default-chat",
        );

        let results = router
            .broadcast("Output with `raw` markdown *chars*", MessageLevel::Plain)
            .await;
        assert_eq!(results.len(), 1);
        assert!(results[0].1.is_ok());

        let captured = sent.lock().await;
        assert_eq!(captured.len(), 1);
        assert!(captured[0].parse_mode.is_none());
    }

    #[tokio::test]
    async fn test_broadcast_typing_uses_default_conversation() {
        let mut router = ChannelRouter::new();
        let sent = Arc::new(Mutex::new(Vec::new()));
        let typing = Arc::new(Mutex::new(Vec::new()));
        router.register_with_default(
            CaptureChannel::new(
                ChannelType::Telegram,
                Arc::clone(&sent),
                Arc::clone(&typing),
            ),
            "default-chat",
        );

        let results = router.broadcast_typing().await;
        assert_eq!(results.len(), 1);
        assert!(results[0].1.is_ok());
        let typing_calls = typing.lock().await;
        assert_eq!(typing_calls.as_slice(), ["default-chat"]);
    }

    #[tokio::test]
    async fn test_cleanup_stale_conversations() {
        let router = ChannelRouter::new();

        // Add a conversation
        let inbound = InboundMessage::new(
            "msg-1",
            ChannelType::Telegram,
            "user-123",
            "chat-456",
            "Hello",
        );
        router.record_conversation(&inbound, None).await;

        // Manually make it stale
        {
            let mut conversations = router.conversations.write().await;
            if let Some(ctx) = conversations.get_mut("chat-456") {
                ctx.last_activity = chrono::Utc::now().timestamp_millis() - 100000;
            }
        }

        // Clean up with shorter threshold
        let removed = router.cleanup_stale_conversations(1000).await;
        assert_eq!(removed, 1);
        assert_eq!(router.conversation_count().await, 0);
    }
}
