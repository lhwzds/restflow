//! Channel Message Handler
//!
//! Processes inbound messages from channels and routes them to appropriate
//! handlers (commands, task forwarder, or help).

use anyhow::Result;
use std::sync::Arc;
use tokio_stream::StreamExt;
use tracing::{debug, error, info, warn};

use restflow_core::channel::{ChannelRouter, InboundMessage};

use super::commands::{handle_command, send_help};
use super::forwarder::forward_to_task;
use super::trigger::TaskTrigger;

/// Message handler configuration
#[derive(Debug, Clone)]
pub struct MessageHandlerConfig {
    /// Command prefix (default: "/")
    pub command_prefix: String,
    /// Whether to auto-acknowledge unknown messages
    pub auto_acknowledge: bool,
}

impl Default for MessageHandlerConfig {
    fn default() -> Self {
        Self {
            command_prefix: "/".to_string(),
            auto_acknowledge: true,
        }
    }
}

/// Start the message handler loop
///
/// This spawns background tasks to listen for messages on all interactive channels
/// and routes them appropriately.
pub fn start_message_handler<T: TaskTrigger + 'static>(
    router: Arc<ChannelRouter>,
    task_trigger: Arc<T>,
    config: MessageHandlerConfig,
) {
    info!("Starting channel message handler");

    // Get all interactive channels
    let interactive_channels = router.list_interactive();

    if interactive_channels.is_empty() {
        info!("No interactive channels configured, message handler idle");
        return;
    }

    for channel_type in interactive_channels {
        if let Some(channel) = router.get(channel_type) {
            if let Some(stream) = channel.start_receiving() {
                let router = router.clone();
                let trigger = task_trigger.clone();
                let config = config.clone();

                tokio::spawn(async move {
                    info!("Listening for messages on {:?}", channel_type);

                    let mut stream = stream;
                    while let Some(message) = stream.next().await {
                        if let Err(e) =
                            handle_message(&router, trigger.as_ref(), &message, &config).await
                        {
                            error!("Error handling message: {}", e);
                        }
                    }

                    warn!("Message stream ended for {:?}", channel_type);
                });
            }
        }
    }
}

/// Process a single inbound message
async fn handle_message(
    router: &ChannelRouter,
    trigger: &dyn TaskTrigger,
    message: &InboundMessage,
    config: &MessageHandlerConfig,
) -> Result<()> {
    debug!(
        "Received: {:?} from {} in {}",
        message.channel_type, message.sender_id, message.conversation_id
    );

    // Record conversation context
    router.record_conversation(message, None).await;

    // Check if this conversation is linked to an active task
    if let Some(context) = router.get_conversation(&message.conversation_id).await {
        if let Some(task_id) = &context.task_id {
            return forward_to_task(router, trigger, task_id, message).await;
        }
    }

    // Check for commands
    if message.content.starts_with(&config.command_prefix) {
        return handle_command(router, trigger, message).await;
    }

    // Default: acknowledge if enabled
    if config.auto_acknowledge {
        send_help(router, message).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::trigger::mock::MockTaskTrigger;
    use restflow_core::channel::ChannelType;

    fn create_message(content: &str) -> InboundMessage {
        InboundMessage::new("msg-1", ChannelType::Telegram, "user-1", "chat-1", content)
    }

    #[test]
    fn test_config_defaults() {
        let config = MessageHandlerConfig::default();
        assert_eq!(config.command_prefix, "/");
        assert!(config.auto_acknowledge);
    }

    #[test]
    fn test_command_detection() {
        let config = MessageHandlerConfig::default();
        let message = create_message("/help");
        assert!(message.content.starts_with(&config.command_prefix));

        let non_command = create_message("hello");
        assert!(!non_command.content.starts_with(&config.command_prefix));
    }

    #[tokio::test]
    async fn test_conversation_context_with_router() {
        let router = Arc::new(ChannelRouter::new());
        let message = create_message("Hello!");

        // Record conversation
        router.record_conversation(&message, None).await;

        // Verify conversation was recorded
        let context = router.get_conversation("chat-1").await;
        assert!(context.is_some());
        let ctx = context.unwrap();
        assert_eq!(ctx.user_id, "user-1");
        assert_eq!(ctx.channel_type, ChannelType::Telegram);
    }

    #[tokio::test]
    async fn test_conversation_with_task_link() {
        let router = Arc::new(ChannelRouter::new());
        let message = create_message("/run test");

        // Record conversation with task
        router
            .record_conversation(&message, Some("task-1".to_string()))
            .await;

        // Verify task is linked
        let context = router.get_conversation("chat-1").await;
        assert!(context.is_some());
        let ctx = context.unwrap();
        assert_eq!(ctx.task_id, Some("task-1".to_string()));
    }

    #[tokio::test]
    async fn test_mock_trigger_forwarding() {
        let trigger = MockTaskTrigger::new();

        // Simulate forwarding a message
        trigger.send_input_to_task("task-1", "continue").await.unwrap();

        let last_input = trigger.last_input.lock().await;
        assert!(last_input.is_some());
        let (task_id, _) = last_input.as_ref().unwrap();
        assert_eq!(task_id, "task-1");
    }
}
