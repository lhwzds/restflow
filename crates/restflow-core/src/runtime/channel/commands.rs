//! Telegram/Channel Command Handler
//!
//! Handles command messages (/help, /tasks, /run, /status, /stop) from channels.

use crate::channel::{ChannelRouter, InboundMessage, MessageLevel, OutboundMessage};
use crate::models::TaskStatus;
use anyhow::Result;
use tracing::debug;

use super::trigger::TaskTrigger;

/// Handle command messages
///
/// Parses the command and executes the appropriate action.
pub async fn handle_command(
    router: &ChannelRouter,
    trigger: &dyn TaskTrigger,
    message: &InboundMessage,
) -> Result<()> {
    let parts: Vec<&str> = message.content.split_whitespace().collect();
    let command = parts.first().map(|s| s.to_lowercase()).unwrap_or_default();

    debug!("Handling command: {} from {}", command, message.sender_id);

    match command.as_str() {
        "/start" | "/help" => cmd_help(router, message).await,
        "/agents" | "/tasks" | "/list" => cmd_list_tasks(router, trigger, message).await,
        "/run" | "/start_task" => {
            let task_name = if parts.len() > 1 {
                Some(parts[1..].join(" "))
            } else {
                None
            };
            cmd_run_task(router, trigger, message, task_name).await
        }
        "/status" => cmd_status(router, trigger, message).await,
        "/stop" => cmd_stop(router, trigger, message).await,
        _ => cmd_unknown(router, message, &command).await,
    }
}

/// Send help message
async fn cmd_help(router: &ChannelRouter, message: &InboundMessage) -> Result<()> {
    let text = r#"🤖 *RestFlow Agent Bot*

*Commands:*
`/tasks` - List all configured tasks
`/run <name>` - Run a task by name or ID
`/status` - Show current status
`/stop` - Stop active task
`/help` - Show this help

*During Task Execution:*
Send messages directly to interact with the task."#;

    let response = OutboundMessage::new(&message.conversation_id, text);
    router.send_to(message.channel_type, response).await
}

/// List all tasks
async fn cmd_list_tasks(
    router: &ChannelRouter,
    trigger: &dyn TaskTrigger,
    message: &InboundMessage,
) -> Result<()> {
    let tasks = trigger.list_tasks().await?;

    let mut text = String::from("📋 *Tasks:*\n\n");

    if tasks.is_empty() {
        text.push_str("_No tasks configured._\n\nCreate one in the RestFlow app.");
    } else {
        for task in tasks.iter().take(10) {
            let status_emoji = match task.status {
                TaskStatus::Running => "🟢",
                TaskStatus::Active => "🟡",
                TaskStatus::Completed => "✅",
                TaskStatus::Failed => "❌",
                TaskStatus::Paused => "⏸️",
                TaskStatus::Interrupted => "⏸️",
            };
            text.push_str(&format!("{} `{}` - {}\n", status_emoji, task.id, task.name));
        }
        if tasks.len() > 10 {
            text.push_str(&format!("\n_...and {} more_", tasks.len() - 10));
        }
    }

    let response = OutboundMessage::new(&message.conversation_id, text);
    router.send_to(message.channel_type, response).await
}

/// Run a task
async fn cmd_run_task(
    router: &ChannelRouter,
    trigger: &dyn TaskTrigger,
    message: &InboundMessage,
    task_name: Option<String>,
) -> Result<()> {
    let task_name = match task_name {
        Some(name) if !name.is_empty() => name,
        _ => {
            let response = OutboundMessage::new(
                &message.conversation_id,
                "⚠️ Usage: `/run <name>`\n\nUse `/tasks` to see available tasks.",
            )
            .with_level(MessageLevel::Warning);
            return router.send_to(message.channel_type, response).await;
        }
    };

    // Notify user before dispatching task execution.
    let pending_response = OutboundMessage::new(
        &message.conversation_id,
        format!("⏳ Received request. Starting task `{}`...", task_name),
    );
    router
        .send_to(message.channel_type, pending_response)
        .await?;

    // Find and run task
    match trigger.find_and_run_task(&task_name).await {
        Ok(task) => {
            // Link conversation to task
            router
                .associate_task(&message.conversation_id, &task.id)
                .await?;

            let response = OutboundMessage::success(
                &message.conversation_id,
                format!(
                    "🚀 Started: *{}*\n\nI'll send updates as the run progresses.",
                    task.name
                ),
            );
            router.send_to(message.channel_type, response).await
        }
        Err(e) => {
            let response = OutboundMessage::error(
                &message.conversation_id,
                format!("Failed to start task: {}", e),
            );
            router.send_to(message.channel_type, response).await
        }
    }
}

/// Show system status
async fn cmd_status(
    router: &ChannelRouter,
    trigger: &dyn TaskTrigger,
    message: &InboundMessage,
) -> Result<()> {
    let status = trigger.get_status().await?;

    let text = format!(
        r#"📊 *System Status*

Runner: {}
Active Tasks: {}
Pending Tasks: {}
Completed Today: {}"#,
        if status.runner_active {
            "✅ Active"
        } else {
            "❌ Stopped"
        },
        status.active_count,
        status.pending_count,
        status.completed_today,
    );

    let response = OutboundMessage::new(&message.conversation_id, text);
    router.send_to(message.channel_type, response).await
}

/// Stop a task
async fn cmd_stop(
    router: &ChannelRouter,
    trigger: &dyn TaskTrigger,
    message: &InboundMessage,
) -> Result<()> {
    // Check if this conversation has an active task
    if let Some(context) = router.get_conversation(&message.conversation_id).await
        && let Some(task_id) = context.task_id
    {
        trigger.stop_task(&task_id).await?;
        router.clear_task(&message.conversation_id).await?;

        let response = OutboundMessage::new(&message.conversation_id, "⏹️ Task stopped.");
        return router.send_to(message.channel_type, response).await;
    }

    let response = OutboundMessage::new(
        &message.conversation_id,
        "No active task in this conversation.",
    )
    .with_level(MessageLevel::Warning);
    router.send_to(message.channel_type, response).await
}

/// Handle unknown command
async fn cmd_unknown(
    router: &ChannelRouter,
    message: &InboundMessage,
    command: &str,
) -> Result<()> {
    let response = OutboundMessage::new(
        &message.conversation_id,
        format!(
            "Unknown command: `{}`\n\nUse `/help` for available commands.",
            command
        ),
    )
    .with_level(MessageLevel::Warning);
    router.send_to(message.channel_type, response).await
}

/// Send help message for unrecognized input
pub async fn send_help(router: &ChannelRouter, message: &InboundMessage) -> Result<()> {
    let text = "👋 Hi! I'm the RestFlow bot.\n\nUse `/help` to see available commands.";
    let response = OutboundMessage::new(&message.conversation_id, text);
    router.send_to(message.channel_type, response).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::{Channel, ChannelType, OutboundMessage};
    use crate::models::{Task, TaskSchedule};
    use crate::runtime::channel::trigger::mock::MockTaskTrigger;
    use async_trait::async_trait;
    use futures::Stream;
    use std::pin::Pin;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    struct CaptureChannel {
        sent: Arc<Mutex<Vec<OutboundMessage>>>,
    }

    #[async_trait]
    impl Channel for CaptureChannel {
        fn channel_type(&self) -> ChannelType {
            ChannelType::Telegram
        }

        fn is_configured(&self) -> bool {
            true
        }

        async fn send(&self, message: OutboundMessage) -> Result<()> {
            self.sent.lock().await.push(message);
            Ok(())
        }

        async fn send_typing(&self, _conversation_id: &str) -> Result<()> {
            Ok(())
        }

        fn start_receiving(&self) -> Option<Pin<Box<dyn Stream<Item = InboundMessage> + Send>>> {
            None
        }
    }

    fn create_message(content: &str) -> InboundMessage {
        InboundMessage::new("msg-1", ChannelType::Telegram, "user-1", "chat-1", content)
    }

    #[tokio::test]
    async fn test_help_command() {
        // Test that help command parsing works (router not needed for parse test)
        let _trigger = MockTaskTrigger::new();
        let message = create_message("/help");

        let parts: Vec<&str> = message.content.split_whitespace().collect();
        let command = parts.first().map(|s| s.to_lowercase()).unwrap_or_default();
        assert_eq!(command, "/help");
    }

    #[tokio::test]
    async fn test_command_parsing() {
        // Test command parsing without needing full router
        let message = create_message("/run my task");
        let parts: Vec<&str> = message.content.split_whitespace().collect();
        let command = parts.first().map(|s| s.to_lowercase()).unwrap_or_default();
        assert_eq!(command, "/run");

        let task_name = if parts.len() > 1 {
            Some(parts[1..].join(" "))
        } else {
            None
        };
        assert_eq!(task_name, Some("my task".to_string()));
    }

    #[tokio::test]
    async fn test_status_uses_trigger() {
        let trigger = MockTaskTrigger::new();
        trigger.set_active_count(2);
        trigger.set_runner_active(true);

        let status = trigger.get_status().await.unwrap();
        assert!(status.runner_active);
        assert_eq!(status.active_count, 2);
    }

    #[tokio::test]
    async fn test_unknown_command_detection() {
        let message = create_message("/foobar");
        let parts: Vec<&str> = message.content.split_whitespace().collect();
        let command = parts.first().map(|s| s.to_lowercase()).unwrap_or_default();
        assert!(!matches!(
            command.as_str(),
            "/start" | "/help" | "/agents" | "/tasks" | "/list" | "/run" | "/status" | "/stop"
        ));
    }

    #[tokio::test]
    async fn test_run_command_sends_pending_then_started_and_associates_task() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let mut router = ChannelRouter::new();
        router.register(CaptureChannel { sent: sent.clone() });

        let trigger = MockTaskTrigger::new();
        let task = Task::new(
            "task-1".to_string(),
            "Build docs".to_string(),
            "agent-1".to_string(),
            TaskSchedule::default(),
        );
        trigger.add_task(task).await;

        let message = create_message("/run task-1");
        router.record_conversation(&message, None).await;

        handle_command(&router, &trigger, &message).await.unwrap();

        let sent_messages = sent.lock().await;
        assert_eq!(sent_messages.len(), 2);
        assert!(sent_messages[0].content.contains("Received request"));
        assert!(sent_messages[1].content.contains("Started"));

        let context = router
            .get_conversation(&message.conversation_id)
            .await
            .unwrap();
        assert_eq!(context.task_id, Some("task-1".to_string()));
    }
}
