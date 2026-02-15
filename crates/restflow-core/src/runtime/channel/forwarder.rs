//! Forward messages to running tasks
//!
//! This module handles forwarding user messages to running tasks.

use crate::channel::{ChannelRouter, InboundMessage, OutboundMessage};
use anyhow::Result;
use tracing::debug;

use super::trigger::BackgroundAgentTrigger;

/// Forward a user message to a running task
pub async fn forward_to_background_agent(
    router: &ChannelRouter,
    trigger: &dyn BackgroundAgentTrigger,
    task_id: &str,
    message: &InboundMessage,
) -> Result<()> {
    debug!(
        "Forwarding message to task {}: {}",
        task_id, message.content
    );

    // Forward message to task input
    match trigger
        .send_message_to_background_agent(task_id, &message.content)
        .await
    {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::channel::trigger::mock::MockBackgroundAgentTrigger;

    #[tokio::test]
    async fn test_mock_trigger_input_tracking() {
        let trigger = MockBackgroundAgentTrigger::new();
        trigger
            .send_message_to_background_agent("task-1", "hello")
            .await
            .unwrap();

        let last_input = trigger.last_input.lock().await;
        assert!(last_input.is_some());
        let (task_id, input) = last_input.as_ref().unwrap();
        assert_eq!(task_id, "task-1");
        assert_eq!(input, "hello");
    }
}
