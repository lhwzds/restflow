//! Telegram notification sender implementation for the task runner.
//!
//! This module provides `TelegramNotifier`, which implements the
//! `NotificationSender` trait to send task completion/failure
//! notifications via Telegram Bot API.

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::sync::Arc;

use crate::{
    models::{BackgroundAgent, NotificationConfig},
    storage::SecretStorage,
};
use restflow_ai::tools::send_telegram_notification;

use super::runner::NotificationSender;

/// Well-known secret name for system-level Telegram bot token.
const TELEGRAM_BOT_TOKEN_SECRET: &str = "TELEGRAM_BOT_TOKEN";

/// Telegram notification sender for agent task results.
///
/// This sender:
/// - Resolves bot token from task config or system secrets
/// - Formats task result as a Telegram message
/// - Sends notification via Telegram Bot API
pub struct TelegramNotifier {
    secrets: Arc<SecretStorage>,
}

impl TelegramNotifier {
    /// Create a new TelegramNotifier with access to secrets storage.
    pub fn new(secrets: Arc<SecretStorage>) -> Self {
        Self { secrets }
    }

    /// Resolve the bot token from config or secrets.
    ///
    /// Priority:
    /// 1. Task-level `telegram_bot_token` in NotificationConfig
    /// 2. System-level secret `TELEGRAM_BOT_TOKEN`
    fn resolve_bot_token(&self, config: &NotificationConfig) -> Result<String> {
        // First, check task-level bot token
        if let Some(ref token) = config.telegram_bot_token
            && !token.is_empty()
        {
            return Ok(token.clone());
        }

        // Fall back to system-level secret
        if let Some(token) = self.secrets.get_secret(TELEGRAM_BOT_TOKEN_SECRET)? {
            return Ok(token);
        }

        Err(anyhow!(
            "No Telegram bot token configured. Please add '{}' in Settings or configure it per-task.",
            TELEGRAM_BOT_TOKEN_SECRET
        ))
    }

    /// Format the notification message for Telegram.
    ///
    /// Uses Markdown formatting for better readability.
    fn format_message(&self, task: &BackgroundAgent, success: bool, message: &str) -> String {
        let status_emoji = if success { "✅" } else { "❌" };
        let status_text = if success { "Completed" } else { "Failed" };

        let mut formatted = format!("{} *Task {}*: {}\n\n", status_emoji, status_text, task.name);

        // Add task details
        formatted.push_str(&format!("🤖 Agent: `{}`\n", task.agent_id));

        if let Some(ref input) = task.input {
            // Truncate long inputs
            let input_preview = if input.len() > 100 {
                format!("{}...", &input[..100])
            } else {
                input.clone()
            };
            formatted.push_str(&format!("📥 Input: {}\n", input_preview));
        }

        formatted.push('\n');

        // Add result/error message
        if message.is_empty() {
            if success {
                formatted.push_str("Task completed successfully.");
            } else {
                formatted.push_str("Task failed with unknown error.");
            }
        } else {
            // Truncate very long messages
            let message_preview = if message.len() > 2000 {
                format!("{}...\n\n_(truncated)_", &message[..2000])
            } else {
                message.to_string()
            };

            if success {
                formatted.push_str(&format!("📤 *Result:*\n```\n{}\n```", message_preview));
            } else {
                formatted.push_str(&format!("⚠️ *Error:*\n```\n{}\n```", message_preview));
            }
        }

        formatted
    }
}

#[async_trait]
impl NotificationSender for TelegramNotifier {
    /// Send a notification for task completion/failure.
    async fn send(
        &self,
        config: &NotificationConfig,
        task: &BackgroundAgent,
        success: bool,
        message: &str,
    ) -> Result<()> {
        // Resolve bot token
        let bot_token = self.resolve_bot_token(config)?;

        // Get chat ID (required)
        let chat_id = config
            .telegram_chat_id
            .as_ref()
            .ok_or_else(|| anyhow!("Telegram chat ID not configured for task '{}'", task.name))?;

        if chat_id.is_empty() {
            return Err(anyhow!(
                "Telegram chat ID is empty for task '{}'",
                task.name
            ));
        }

        // Format the message
        let formatted_message = self.format_message(task, success, message);

        // Send via Telegram API
        send_telegram_notification(&bot_token, chat_id, &formatted_message, Some("Markdown"))
            .await
            .map_err(|e| anyhow!("Failed to send Telegram notification: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TaskSchedule;
    use tempfile::tempdir;

    fn create_test_secrets() -> (Arc<SecretStorage>, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(redb::Database::create(db_path).unwrap());
        (Arc::new(SecretStorage::new(db).unwrap()), temp_dir)
    }

    fn create_test_task() -> BackgroundAgent {
        let now = chrono::Utc::now().timestamp_millis();
        BackgroundAgent::new(
            "test-task-id".to_string(),
            "Test Task".to_string(),
            "agent-001".to_string(),
            TaskSchedule::Once { run_at: now },
        )
    }

    #[test]
    fn test_notifier_creation() {
        let (secrets, _temp_dir) = create_test_secrets();
        let notifier = TelegramNotifier::new(secrets);
        assert!(Arc::strong_count(&notifier.secrets) >= 1);
    }

    #[test]
    fn test_resolve_bot_token_from_config() {
        let (secrets, _temp_dir) = create_test_secrets();
        let notifier = TelegramNotifier::new(secrets);

        let config = NotificationConfig {
            telegram_bot_token: Some("config-token-123".to_string()),
            ..Default::default()
        };

        let token = notifier.resolve_bot_token(&config).unwrap();
        assert_eq!(token, "config-token-123");
    }

    #[test]
    fn test_resolve_bot_token_from_secrets() {
        let (secrets, _temp_dir) = create_test_secrets();

        // Store a system-level token
        secrets
            .set_secret(TELEGRAM_BOT_TOKEN_SECRET, "secret-token-456", None)
            .unwrap();

        let notifier = TelegramNotifier::new(secrets);

        // Config without token should fall back to secrets
        let config = NotificationConfig::default();
        let token = notifier.resolve_bot_token(&config).unwrap();
        assert_eq!(token, "secret-token-456");
    }

    #[test]
    fn test_resolve_bot_token_config_priority() {
        let (secrets, _temp_dir) = create_test_secrets();

        // Store a system-level token
        secrets
            .set_secret(TELEGRAM_BOT_TOKEN_SECRET, "secret-token-456", None)
            .unwrap();

        let notifier = TelegramNotifier::new(secrets);

        // Config token should take priority
        let config = NotificationConfig {
            telegram_bot_token: Some("config-token-123".to_string()),
            ..Default::default()
        };

        let token = notifier.resolve_bot_token(&config).unwrap();
        assert_eq!(token, "config-token-123");
    }

    #[test]
    fn test_resolve_bot_token_empty_config_falls_back() {
        let (secrets, _temp_dir) = create_test_secrets();

        secrets
            .set_secret(TELEGRAM_BOT_TOKEN_SECRET, "secret-token-456", None)
            .unwrap();

        let notifier = TelegramNotifier::new(secrets);

        // Empty config token should fall back to secrets
        let config = NotificationConfig {
            telegram_bot_token: Some("".to_string()),
            ..Default::default()
        };

        let token = notifier.resolve_bot_token(&config).unwrap();
        assert_eq!(token, "secret-token-456");
    }

    #[test]
    fn test_resolve_bot_token_missing() {
        let (secrets, _temp_dir) = create_test_secrets();
        let notifier = TelegramNotifier::new(secrets);

        let config = NotificationConfig::default();
        let result = notifier.resolve_bot_token(&config);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No Telegram bot token")
        );
    }

    #[test]
    fn test_format_message_success() {
        let (secrets, _temp_dir) = create_test_secrets();
        let notifier = TelegramNotifier::new(secrets);

        let task = create_test_task();
        let message = notifier.format_message(&task, true, "Task output here");

        assert!(message.contains("✅"));
        assert!(message.contains("Completed"));
        assert!(message.contains("Test Task"));
        assert!(message.contains("agent-001"));
        assert!(message.contains("Task output here"));
    }

    #[test]
    fn test_format_message_failure() {
        let (secrets, _temp_dir) = create_test_secrets();
        let notifier = TelegramNotifier::new(secrets);

        let task = create_test_task();
        let message = notifier.format_message(&task, false, "Connection timeout");

        assert!(message.contains("❌"));
        assert!(message.contains("Failed"));
        assert!(message.contains("Test Task"));
        assert!(message.contains("Connection timeout"));
    }

    #[test]
    fn test_format_message_with_input() {
        let (secrets, _temp_dir) = create_test_secrets();
        let notifier = TelegramNotifier::new(secrets);

        let mut task = create_test_task();
        task.input = Some("Process this data".to_string());

        let message = notifier.format_message(&task, true, "Done");

        assert!(message.contains("Process this data"));
    }

    #[test]
    fn test_format_message_truncates_long_input() {
        let (secrets, _temp_dir) = create_test_secrets();
        let notifier = TelegramNotifier::new(secrets);

        let mut task = create_test_task();
        task.input = Some("x".repeat(200));

        let message = notifier.format_message(&task, true, "Done");

        // Should contain truncated input with ellipsis
        assert!(message.contains("..."));
        // Should not contain the full 200 x's
        assert!(!message.contains(&"x".repeat(150)));
    }

    #[test]
    fn test_format_message_truncates_long_output() {
        let (secrets, _temp_dir) = create_test_secrets();
        let notifier = TelegramNotifier::new(secrets);

        let task = create_test_task();
        let long_output = "y".repeat(3000);
        let message = notifier.format_message(&task, true, &long_output);

        // Should contain truncation indicator
        assert!(message.contains("truncated"));
        // Should not contain the full 3000 y's
        assert!(!message.contains(&"y".repeat(2500)));
    }

    #[test]
    fn test_format_message_empty_message() {
        let (secrets, _temp_dir) = create_test_secrets();
        let notifier = TelegramNotifier::new(secrets);

        let task = create_test_task();

        let success_msg = notifier.format_message(&task, true, "");
        assert!(success_msg.contains("completed successfully"));

        let failure_msg = notifier.format_message(&task, false, "");
        assert!(failure_msg.contains("unknown error"));
    }

    #[tokio::test]
    async fn test_send_missing_chat_id() {
        let (secrets, _temp_dir) = create_test_secrets();
        secrets
            .set_secret(TELEGRAM_BOT_TOKEN_SECRET, "test-token", None)
            .unwrap();

        let notifier = TelegramNotifier::new(secrets);
        let task = create_test_task();

        // Config without chat ID
        let config = NotificationConfig::default();

        let result = notifier.send(&config, &task, true, "output").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("chat ID"));
    }

    #[tokio::test]
    async fn test_send_empty_chat_id() {
        let (secrets, _temp_dir) = create_test_secrets();
        secrets
            .set_secret(TELEGRAM_BOT_TOKEN_SECRET, "test-token", None)
            .unwrap();

        let notifier = TelegramNotifier::new(secrets);
        let task = create_test_task();

        let config = NotificationConfig {
            telegram_chat_id: Some("".to_string()),
            ..Default::default()
        };

        let result = notifier.send(&config, &task, true, "output").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }
}
