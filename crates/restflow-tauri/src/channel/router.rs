//! Message Router - Routes inbound messages to appropriate handlers.
//!
//! This module provides the `MessageRouter` which determines how to handle
//! each inbound message based on conversation context and message content.

use restflow_core::channel::{ChannelRouter, InboundMessage};
use std::sync::Arc;

/// Routing decision for an inbound message.
#[derive(Debug, Clone, PartialEq)]
pub enum RouteDecision {
    /// Forward the message to a linked task.
    ForwardToTask { task_id: String },
    /// Handle as a command (e.g., /help, /run).
    HandleCommand { command: String, args: Vec<String> },
    /// Dispatch to AI chat for natural language processing.
    DispatchToChat,
    /// Ignore the message (no action needed).
    Ignore,
}

/// Message router that determines how to handle inbound messages.
///
/// The router checks:
/// 1. Is the conversation linked to an active task? → Forward to task
/// 2. Is the message a command (starts with prefix)? → Handle as command
/// 3. Otherwise → Dispatch to AI chat
pub struct MessageRouter {
    channel_router: Arc<ChannelRouter>,
    command_prefix: String,
}

impl MessageRouter {
    /// Create a new MessageRouter.
    pub fn new(channel_router: Arc<ChannelRouter>, command_prefix: impl Into<String>) -> Self {
        Self {
            channel_router,
            command_prefix: command_prefix.into(),
        }
    }

    /// Route an inbound message to the appropriate handler.
    pub async fn route(&self, message: &InboundMessage) -> RouteDecision {
        // 1. Check if conversation is linked to an active task
        if let Some(ctx) = self.channel_router.get_conversation(&message.conversation_id).await {
            if let Some(task_id) = ctx.task_id {
                return RouteDecision::ForwardToTask { task_id };
            }
        }

        // 2. Check for commands
        if message.content.starts_with(&self.command_prefix) {
            if let Some((command, args)) = self.parse_command(&message.content) {
                return RouteDecision::HandleCommand { command, args };
            }
        }

        // 3. Default: dispatch to AI chat
        RouteDecision::DispatchToChat
    }

    /// Parse a command message into command name and arguments.
    fn parse_command(&self, content: &str) -> Option<(String, Vec<String>)> {
        let trimmed = content.strip_prefix(&self.command_prefix)?;
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        
        if parts.is_empty() {
            return None;
        }

        let command = parts[0].to_lowercase();
        let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

        Some((command, args))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_core::channel::ChannelType;

    fn create_message(content: &str) -> InboundMessage {
        InboundMessage::new("msg-1", ChannelType::Telegram, "user-1", "chat-1", content)
    }

    #[tokio::test]
    async fn test_route_command() {
        let channel_router = Arc::new(ChannelRouter::new());
        let router = MessageRouter::new(channel_router, "/");

        let message = create_message("/help");
        let decision = router.route(&message).await;

        assert!(matches!(
            decision,
            RouteDecision::HandleCommand { command, args } if command == "help" && args.is_empty()
        ));
    }

    #[tokio::test]
    async fn test_route_command_with_args() {
        let channel_router = Arc::new(ChannelRouter::new());
        let router = MessageRouter::new(channel_router, "/");

        let message = create_message("/run my task");
        let decision = router.route(&message).await;

        assert!(matches!(
            decision,
            RouteDecision::HandleCommand { command, args } 
            if command == "run" && args == vec!["my", "task"]
        ));
    }

    #[tokio::test]
    async fn test_route_natural_language_to_chat() {
        let channel_router = Arc::new(ChannelRouter::new());
        let router = MessageRouter::new(channel_router, "/");

        let message = create_message("Hello, can you help me?");
        let decision = router.route(&message).await;

        assert_eq!(decision, RouteDecision::DispatchToChat);
    }

    #[tokio::test]
    async fn test_route_task_linked_conversation() {
        let channel_router = Arc::new(ChannelRouter::new());
        
        // Record a conversation with a task link
        let message = create_message("test");
        channel_router
            .record_conversation(&message, Some("task-1".to_string()))
            .await;

        let router = MessageRouter::new(channel_router, "/");

        // Even a command should be forwarded to task if linked
        let cmd_message = create_message("/help");
        let decision = router.route(&cmd_message).await;

        assert!(matches!(
            decision,
            RouteDecision::ForwardToTask { task_id } if task_id == "task-1"
        ));
    }

    #[tokio::test]
    async fn test_route_empty_command() {
        let channel_router = Arc::new(ChannelRouter::new());
        let router = MessageRouter::new(channel_router, "/");

        let message = create_message("/");
        let decision = router.route(&message).await;

        // Empty command should fall through to chat
        assert_eq!(decision, RouteDecision::DispatchToChat);
    }

    #[test]
    fn test_parse_command() {
        let channel_router = Arc::new(ChannelRouter::new());
        let router = MessageRouter::new(channel_router, "/");

        let result = router.parse_command("/status");
        assert_eq!(result, Some(("status".to_string(), vec![])));

        let result = router.parse_command("/run my task name");
        assert_eq!(
            result,
            Some((
                "run".to_string(),
                vec!["my".to_string(), "task".to_string(), "name".to_string()]
            ))
        );

        let result = router.parse_command("/");
        assert_eq!(result, None);

        let result = router.parse_command("no prefix");
        assert_eq!(result, None);
    }
}
