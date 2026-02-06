//! Channel Message Handler
//!
//! Processes inbound messages from channels and routes them to appropriate
//! handlers (commands, task forwarder, chat dispatcher, or help).

use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::StreamExt;
use tracing::{debug, error, info, warn};

/// Timeout for handling a single message (seconds)
const MESSAGE_HANDLER_TIMEOUT_SECS: u64 = 120;

use restflow_core::channel::{ChannelRouter, InboundMessage};

use super::chat_dispatcher::ChatDispatcher;
use super::commands::{handle_command, send_help};
use super::forwarder::forward_to_task;
use super::router::{MessageRouter, RouteDecision};
use super::trigger::TaskTrigger;

/// Message handler configuration
#[derive(Debug, Clone)]
pub struct MessageHandlerConfig {
    /// Command prefix (default: "/")
    pub command_prefix: String,
    /// Whether to auto-acknowledge unknown messages (when chat is disabled)
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

/// Start the message handler loop (without AI chat support)
///
/// This spawns background tasks to listen for messages on all interactive channels
/// and routes them appropriately. Natural language messages will show a help message.
pub fn start_message_handler<T: TaskTrigger + 'static>(
    router: Arc<ChannelRouter>,
    task_trigger: Arc<T>,
    config: MessageHandlerConfig,
) {
    start_message_handler_internal(router, task_trigger, None, config);
}

/// Start the message handler loop with AI chat support
///
/// This spawns background tasks to listen for messages on all interactive channels
/// and routes them appropriately. Natural language messages are dispatched to
/// the AI chat dispatcher.
pub fn start_message_handler_with_chat<T: TaskTrigger + 'static>(
    router: Arc<ChannelRouter>,
    task_trigger: Arc<T>,
    chat_dispatcher: Arc<ChatDispatcher>,
    config: MessageHandlerConfig,
) {
    start_message_handler_internal(router, task_trigger, Some(chat_dispatcher), config);
}

/// Internal implementation of message handler startup
fn start_message_handler_internal<T: TaskTrigger + 'static>(
    router: Arc<ChannelRouter>,
    task_trigger: Arc<T>,
    chat_dispatcher: Option<Arc<ChatDispatcher>>,
    config: MessageHandlerConfig,
) {
    let chat_enabled = chat_dispatcher.is_some();
    info!(
        "Starting channel message handler (chat_enabled={})",
        chat_enabled
    );

    // Get all interactive channels
    let interactive_channels = router.list_interactive();

    if interactive_channels.is_empty() {
        info!("No interactive channels configured, message handler idle");
        return;
    }

    // Create the message router
    let msg_router = Arc::new(MessageRouter::new(router.clone(), &config.command_prefix));

    for channel_type in interactive_channels {
        if let Some(channel) = router.get(channel_type)
            && let Some(stream) = channel.start_receiving()
        {
            let router = router.clone();
            let trigger = task_trigger.clone();
            let msg_router = msg_router.clone();
            let chat_dispatcher = chat_dispatcher.clone();
            let config = config.clone();

            tokio::spawn(async move {
                info!("Listening for messages on {:?}", channel_type);

                let mut stream = stream;
                loop {
                    let message = match stream.next().await {
                        Some(msg) => msg,
                        None => {
                            warn!("Message stream ended for {:?}", channel_type);
                            break;
                        }
                    };

                    debug!(
                        "Handler received message {} from {}",
                        message.id, message.conversation_id
                    );

                    // Wrap message handling with timeout to prevent hanging
                    let handler_future = handle_message_routed(
                        &router,
                        &msg_router,
                        trigger.as_ref(),
                        chat_dispatcher.as_ref().map(|d| d.as_ref()),
                        &message,
                        &config,
                    );

                    let result = tokio::time::timeout(
                        Duration::from_secs(MESSAGE_HANDLER_TIMEOUT_SECS),
                        handler_future,
                    )
                    .await;

                    match result {
                        Ok(Ok(())) => {
                            debug!("Message {} handled successfully", message.id);
                        }
                        Ok(Err(e)) => {
                            error!(
                                "Error handling message {} from {}: {}",
                                message.id, message.conversation_id, e
                            );
                        }
                        Err(_) => {
                            error!(
                                "TIMEOUT handling message {} from {} ({}s exceeded)",
                                message.id, message.conversation_id, MESSAGE_HANDLER_TIMEOUT_SECS
                            );
                        }
                    }

                    // Continue processing next message regardless of error
                }
            });
        }
    }
}

/// Process a single inbound message using the router
async fn handle_message_routed(
    router: &ChannelRouter,
    msg_router: &MessageRouter,
    trigger: &dyn TaskTrigger,
    chat_dispatcher: Option<&ChatDispatcher>,
    message: &InboundMessage,
    config: &MessageHandlerConfig,
) -> Result<()> {
    debug!(
        "Received: {:?} from {} in {}",
        message.channel_type, message.sender_id, message.conversation_id
    );

    // Record conversation context (preserves existing task link if any)
    router.record_conversation(message, None).await;

    // Route the message
    let decision = msg_router.route(message).await;

    match decision {
        RouteDecision::ForwardToTask { task_id } => {
            debug!("Routing to task: {}", task_id);
            forward_to_task(router, trigger, &task_id, message).await
        }

        RouteDecision::HandleCommand { command, args } => {
            debug!("Routing to command: {} {:?}", command, args);
            handle_command(router, trigger, message).await
        }

        RouteDecision::DispatchToChat => {
            if let Some(dispatcher) = chat_dispatcher {
                debug!("Routing to chat dispatcher");
                dispatcher.dispatch(message).await
            } else if config.auto_acknowledge {
                debug!("Chat disabled, sending help");
                send_help(router, message).await
            } else {
                debug!("Ignoring natural language message (chat disabled)");
                Ok(())
            }
        }

        RouteDecision::Ignore => {
            debug!("Ignoring message");
            Ok(())
        }
    }
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
        trigger
            .send_input_to_task("task-1", "continue")
            .await
            .unwrap();

        let last_input = trigger.last_input.lock().await;
        assert!(last_input.is_some());
        let (task_id, _) = last_input.as_ref().unwrap();
        assert_eq!(task_id, "task-1");
    }

    #[tokio::test]
    async fn test_message_router_integration() {
        let channel_router = Arc::new(ChannelRouter::new());
        let msg_router = MessageRouter::new(channel_router.clone(), "/");

        // Test command routing
        let cmd_msg = create_message("/help");
        let decision = msg_router.route(&cmd_msg).await;
        assert!(matches!(decision, RouteDecision::HandleCommand { .. }));

        // Test natural language routing
        let chat_msg = create_message("Hello, how are you?");
        let decision = msg_router.route(&chat_msg).await;
        assert_eq!(decision, RouteDecision::DispatchToChat);
    }

    #[tokio::test]
    async fn test_task_linked_conversation_routes_to_task() {
        let channel_router = Arc::new(ChannelRouter::new());
        let msg_router = MessageRouter::new(channel_router.clone(), "/");

        // First, record a conversation with a task link
        let initial_msg = create_message("initial");
        channel_router
            .record_conversation(&initial_msg, Some("task-123".to_string()))
            .await;

        // Even a command should be forwarded to task when linked
        let cmd_msg = create_message("/status");
        let decision = msg_router.route(&cmd_msg).await;
        assert!(matches!(
            decision,
            RouteDecision::ForwardToTask { task_id } if task_id == "task-123"
        ));

        // Natural language should also forward to task
        let chat_msg = create_message("continue with the task");
        let decision = msg_router.route(&chat_msg).await;
        assert!(matches!(
            decision,
            RouteDecision::ForwardToTask { task_id } if task_id == "task-123"
        ));
    }
}
