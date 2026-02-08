//! Channel Message Handler
//!
//! Processes inbound messages from channels and routes them to appropriate
//! handlers (commands, task forwarder, chat dispatcher, or help).

use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tokio_stream::StreamExt;
use tracing::{debug, error, info, warn};

use crate::channel::{ChannelRouter, InboundMessage};

use super::chat_dispatcher::ChatDispatcher;
use super::commands::{handle_command, send_help};
use super::forwarder::forward_to_task;
use super::router::{MessageRouter, RouteDecision};
use super::trigger::TaskTrigger;

#[cfg(test)]
const STREAM_RECONNECT_DELAY: Duration = Duration::from_millis(20);
#[cfg(not(test))]
const STREAM_RECONNECT_DELAY: Duration = Duration::from_secs(2);

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
        let Some(channel) = router.get(channel_type).cloned() else {
            continue;
        };
        let router = router.clone();
        let trigger = task_trigger.clone();
        let msg_router = msg_router.clone();
        let chat_dispatcher = chat_dispatcher.clone();
        let config = config.clone();

        tokio::spawn(async move {
            info!("Listening for messages on {:?}", channel_type);

            loop {
                let Some(mut stream) = channel.start_receiving() else {
                    warn!(
                        "Failed to start message stream for {:?}, retrying in {:?}",
                        channel_type, STREAM_RECONNECT_DELAY
                    );
                    sleep(STREAM_RECONNECT_DELAY).await;
                    continue;
                };

                loop {
                    let message = match stream.next().await {
                        Some(msg) => msg,
                        None => {
                            warn!(
                                "Message stream ended for {:?}, restarting in {:?}",
                                channel_type, STREAM_RECONNECT_DELAY
                            );
                            break;
                        }
                    };

                    debug!(
                        "Handler received message {} from {}",
                        message.id, message.conversation_id
                    );

                    // Process message without timeout - AI reasoning can take
                    // variable amounts of time depending on complexity
                    let result = handle_message_routed(
                        &router,
                        &msg_router,
                        trigger.as_ref(),
                        chat_dispatcher.as_ref().map(|d| d.as_ref()),
                        &message,
                        &config,
                    )
                    .await;

                    match result {
                        Ok(()) => {
                            debug!("Message {} handled successfully", message.id);
                        }
                        Err(e) => {
                            error!(
                                "Error handling message {} from {}: {}",
                                message.id, message.conversation_id, e
                            );
                        }
                    }

                    // Continue processing next message regardless of error
                }

                sleep(STREAM_RECONNECT_DELAY).await;
            }
        });
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
    use crate::channel::{Channel, ChannelType, OutboundMessage};
    use crate::runtime::channel::trigger::mock::MockTaskTrigger;
    use anyhow::Result as AnyhowResult;
    use async_trait::async_trait;
    use std::collections::VecDeque;
    use std::pin::Pin;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::Mutex as AsyncMutex;
    use tokio::time::timeout;
    use tokio_stream::iter;

    fn create_message(content: &str) -> InboundMessage {
        InboundMessage::new("msg-1", ChannelType::Telegram, "user-1", "chat-1", content)
    }

    struct ReconnectTestChannel {
        streams: Mutex<VecDeque<Vec<InboundMessage>>>,
        sent_messages: Arc<AsyncMutex<Vec<OutboundMessage>>>,
        start_calls: Arc<AtomicUsize>,
    }

    impl ReconnectTestChannel {
        fn new(batches: Vec<Vec<InboundMessage>>) -> Self {
            Self {
                streams: Mutex::new(VecDeque::from(batches)),
                sent_messages: Arc::new(AsyncMutex::new(Vec::new())),
                start_calls: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    #[async_trait]
    impl Channel for ReconnectTestChannel {
        fn channel_type(&self) -> ChannelType {
            ChannelType::Telegram
        }

        fn is_configured(&self) -> bool {
            true
        }

        async fn send(&self, message: OutboundMessage) -> AnyhowResult<()> {
            self.sent_messages.lock().await.push(message);
            Ok(())
        }

        fn start_receiving(
            &self,
        ) -> Option<Pin<Box<dyn tokio_stream::Stream<Item = InboundMessage> + Send>>> {
            self.start_calls.fetch_add(1, Ordering::SeqCst);
            let mut streams = self.streams.lock().expect("lock reconnect test streams");
            let batch = streams.pop_front()?;
            Some(Box::pin(iter(batch)))
        }
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

    #[tokio::test]
    async fn test_handler_recovers_after_stream_ends() {
        let first =
            InboundMessage::new("msg-1", ChannelType::Telegram, "user-1", "chat-1", "first");
        let second =
            InboundMessage::new("msg-2", ChannelType::Telegram, "user-1", "chat-1", "second");

        let test_channel = ReconnectTestChannel::new(vec![vec![first], vec![second]]);
        let sent_messages = test_channel.sent_messages.clone();
        let start_calls = test_channel.start_calls.clone();

        let mut router = ChannelRouter::new();
        router.register(test_channel);
        let router = Arc::new(router);
        let trigger = Arc::new(MockTaskTrigger::new());

        start_message_handler(router, trigger, MessageHandlerConfig::default());

        timeout(Duration::from_secs(2), async {
            loop {
                let send_count = sent_messages.lock().await.len();
                let stream_start_count = start_calls.load(Ordering::SeqCst);
                if send_count >= 2 && stream_start_count >= 2 {
                    break;
                }
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("message handler should reconnect after stream end");
    }
}
