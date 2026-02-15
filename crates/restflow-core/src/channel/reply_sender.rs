//! Channel-based reply sender for the ReplyTool.
//!
//! Bridges the `ReplySender` trait (from restflow-ai) with the channel
//! infrastructure so agents can send intermediate messages to users
//! during execution.

use crate::channel::{ChannelRouter, ChannelType, OutboundMessage};
use restflow_ai::tools::ReplySender;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Sends agent replies through a `ChannelRouter`.
///
/// Created per-dispatch with the conversation context (channel type +
/// conversation ID) so the agent's intermediate messages reach the
/// correct user.
pub struct ChannelReplySender {
    router: Arc<ChannelRouter>,
    conversation_id: String,
    channel_type: ChannelType,
}

impl ChannelReplySender {
    pub fn new(
        router: Arc<ChannelRouter>,
        conversation_id: impl Into<String>,
        channel_type: ChannelType,
    ) -> Self {
        Self {
            router,
            conversation_id: conversation_id.into(),
            channel_type,
        }
    }
}

impl ReplySender for ChannelReplySender {
    fn send(&self, message: String) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
        let router = self.router.clone();
        let channel_type = self.channel_type;
        let response = OutboundMessage::plain(&self.conversation_id, message);

        Box::pin(async move { router.send_to(channel_type, response).await })
    }
}
