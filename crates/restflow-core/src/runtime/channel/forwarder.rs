//! Forward messages to running tasks
//!
//! This module handles forwarding user messages to running tasks and
//! processing approval responses.

use crate::channel::{ChannelRouter, InboundMessage, MessageLevel, OutboundMessage};
use anyhow::Result;
use tracing::debug;

use super::trigger::TaskTrigger;

/// Forward a user message to a running task
pub async fn forward_to_task(
    router: &ChannelRouter,
    trigger: &dyn TaskTrigger,
    task_id: &str,
    message: &InboundMessage,
) -> Result<()> {
    debug!(
        "Forwarding message to task {}: {}",
        task_id, message.content
    );

    // Check for approval responses first
    let content_lower = message.content.to_lowercase();
    let content_trimmed = content_lower.trim();

    if matches!(content_trimmed, "approve" | "yes" | "y" | "✅") {
        return handle_approval(router, trigger, task_id, message, true).await;
    }

    if matches!(content_trimmed, "reject" | "no" | "n" | "❌") {
        return handle_approval(router, trigger, task_id, message, false).await;
    }

    // Forward message to task input
    match trigger.send_input_to_task(task_id, &message.content).await {
        Ok(()) => {
            // Acknowledge receipt
            let response =
                OutboundMessage::new(&message.conversation_id, "📨 Message forwarded to agent.");
            router.send_to(message.channel_type, response).await
        }
        Err(e) => {
            let response = OutboundMessage::error(
                &message.conversation_id,
                format!("Failed to forward message: {}", e),
            );
            router.send_to(message.channel_type, response).await
        }
    }
}

/// Handle approval response for command execution
async fn handle_approval(
    router: &ChannelRouter,
    trigger: &dyn TaskTrigger,
    task_id: &str,
    message: &InboundMessage,
    approved: bool,
) -> Result<()> {
    debug!(
        "Handling approval for task {}: approved={}",
        task_id, approved
    );

    match trigger.handle_approval(task_id, approved).await {
        Ok(true) => {
            let response = if approved {
                OutboundMessage::success(
                    &message.conversation_id,
                    "✅ Approved. Executing command...",
                )
            } else {
                OutboundMessage::new(&message.conversation_id, "❌ Rejected. Command cancelled.")
                    .with_level(MessageLevel::Warning)
            };
            router.send_to(message.channel_type, response).await
        }
        Ok(false) => {
            let response =
                OutboundMessage::new(&message.conversation_id, "No pending approval found.")
                    .with_level(MessageLevel::Warning);
            router.send_to(message.channel_type, response).await
        }
        Err(e) => {
            let response =
                OutboundMessage::error(&message.conversation_id, format!("Approval error: {}", e));
            router.send_to(message.channel_type, response).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::channel::trigger::mock::MockTaskTrigger;

    #[tokio::test]
    async fn test_approval_detection_approve() {
        let content_lower = "approve".to_lowercase();
        let content_trimmed = content_lower.trim();
        assert!(matches!(content_trimmed, "approve" | "yes" | "y" | "✅"));
    }

    #[tokio::test]
    async fn test_approval_detection_reject() {
        let content_lower = "reject".to_lowercase();
        let content_trimmed = content_lower.trim();
        assert!(matches!(content_trimmed, "reject" | "no" | "n" | "❌"));
    }

    #[tokio::test]
    async fn test_approval_detection_emoji() {
        assert!(matches!("✅", "approve" | "yes" | "y" | "✅"));
        assert!(matches!("❌", "reject" | "no" | "n" | "❌"));
    }

    #[tokio::test]
    async fn test_mock_trigger_input_tracking() {
        let trigger = MockTaskTrigger::new();
        trigger.send_input_to_task("task-1", "hello").await.unwrap();

        let last_input = trigger.last_input.lock().await;
        assert!(last_input.is_some());
        let (task_id, input) = last_input.as_ref().unwrap();
        assert_eq!(task_id, "task-1");
        assert_eq!(input, "hello");
    }

    #[tokio::test]
    async fn test_mock_trigger_approval_tracking() {
        let trigger = MockTaskTrigger::new();
        trigger.handle_approval("task-1", true).await.unwrap();

        let last_approval = trigger.last_approval.lock().await;
        assert!(last_approval.is_some());
        let (task_id, approved) = last_approval.as_ref().unwrap();
        assert_eq!(task_id, "task-1");
        assert!(*approved);
    }
}
