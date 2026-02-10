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
/// Well-known secret names for default Telegram destination.
const TELEGRAM_CHAT_ID_SECRET: &str = "TELEGRAM_CHAT_ID";
const TELEGRAM_DEFAULT_CHAT_ID_SECRET: &str = "TELEGRAM_DEFAULT_CHAT_ID";

/// Telegram notification sender for agent task results.
///
/// This sender:
/// - Resolves bot token from system secrets
/// - Resolves destination chat from system secrets
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

    /// Resolve bot token from system-level secret.
    fn resolve_bot_token(&self) -> Result<String> {
        if let Some(token) = self.secrets.get_secret(TELEGRAM_BOT_TOKEN_SECRET)? {
            return Ok(token);
        }

        Err(anyhow!(
            "No Telegram bot token configured. Please add '{}' in Settings.",
            TELEGRAM_BOT_TOKEN_SECRET
        ))
    }

    fn resolve_chat_id(&self) -> Result<String> {
        let primary = self
            .secrets
            .get_secret(TELEGRAM_CHAT_ID_SECRET)?
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        if let Some(chat_id) = primary {
            return Ok(chat_id);
        }

        let legacy = self
            .secrets
            .get_secret(TELEGRAM_DEFAULT_CHAT_ID_SECRET)?
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        if let Some(chat_id) = legacy {
            return Ok(chat_id);
        }

        Err(anyhow!(
            "No default Telegram chat id configured. Please set '{}' or '{}'.",
            TELEGRAM_CHAT_ID_SECRET,
            TELEGRAM_DEFAULT_CHAT_ID_SECRET
        ))
    }

    /// Format the notification message for Telegram.
    fn format_message(&self, task: &BackgroundAgent, success: bool, message: &str) -> String {
        if success {
            if message.is_empty() {
                return "Task completed successfully.".to_string();
            }

            // Keep success notification text as close as possible to the agent output.
            return if message.len() > 3500 {
                format!("{}...\n\n(truncated)", &message[..3500])
            } else {
                message.to_string()
            };
        }

        let mut formatted = format!("❌ *Task Failed*: {}\n\n", task.name);
        formatted.push_str(&format!("🤖 Agent: `{}`\n", task.agent_id));

        if let Some(ref input) = task.input {
            let input_preview = if input.len() > 100 {
                format!("{}...", &input[..100])
            } else {
                input.clone()
            };
            formatted.push_str(&format!("📥 Input: {}\n", input_preview));
        }

        formatted.push('\n');

        if message.is_empty() {
            formatted.push_str("Task failed with unknown error.");
        } else {
            let message_preview = if message.len() > 2000 {
                format!("{}...\n\n_(truncated)_", &message[..2000])
            } else {
                message.to_string()
            };
            formatted.push_str(&format!("⚠️ *Error:*\n```\n{}\n```", message_preview));
        }

        formatted
    }
}

#[async_trait]
impl NotificationSender for TelegramNotifier {
    /// Send a notification for task completion/failure.
    async fn send(
        &self,
        _config: &NotificationConfig,
        task: &BackgroundAgent,
        success: bool,
        message: &str,
    ) -> Result<()> {
        // Resolve bot token
        let bot_token = self.resolve_bot_token()?;
        let chat_id = self.resolve_chat_id()?;

        // Format the message
        let formatted_message = self.format_message(task, success, message);

        // Success payloads are plain agent output; avoid Markdown parse issues.
        let parse_mode = if success { None } else { Some("Markdown") };

        // Send via Telegram API
        send_telegram_notification(&bot_token, &chat_id, &formatted_message, parse_mode)
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
    fn test_resolve_bot_token_from_secrets() {
        let (secrets, _temp_dir) = create_test_secrets();

        // Store a system-level token
        secrets
            .set_secret(TELEGRAM_BOT_TOKEN_SECRET, "secret-token-456", None)
            .unwrap();

        let notifier = TelegramNotifier::new(secrets);

        let token = notifier.resolve_bot_token().unwrap();
        assert_eq!(token, "secret-token-456");
    }

    #[test]
    fn test_resolve_bot_token_missing() {
        let (secrets, _temp_dir) = create_test_secrets();
        let notifier = TelegramNotifier::new(secrets);

        let result = notifier.resolve_bot_token();

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No Telegram bot token")
        );
    }

    #[test]
    fn test_resolve_chat_id_from_primary_secret() {
        let (secrets, _temp_dir) = create_test_secrets();
        secrets
            .set_secret(TELEGRAM_CHAT_ID_SECRET, "12345678", None)
            .unwrap();

        let notifier = TelegramNotifier::new(secrets);
        let chat_id = notifier.resolve_chat_id().unwrap();
        assert_eq!(chat_id, "12345678");
    }

    #[test]
    fn test_resolve_chat_id_from_legacy_secret() {
        let (secrets, _temp_dir) = create_test_secrets();
        secrets
            .set_secret(TELEGRAM_DEFAULT_CHAT_ID_SECRET, "87654321", None)
            .unwrap();

        let notifier = TelegramNotifier::new(secrets);
        let chat_id = notifier.resolve_chat_id().unwrap();
        assert_eq!(chat_id, "87654321");
    }

    #[test]
    fn test_resolve_chat_id_prefers_primary_secret() {
        let (secrets, _temp_dir) = create_test_secrets();
        secrets
            .set_secret(TELEGRAM_CHAT_ID_SECRET, "12345678", None)
            .unwrap();
        secrets
            .set_secret(TELEGRAM_DEFAULT_CHAT_ID_SECRET, "87654321", None)
            .unwrap();

        let notifier = TelegramNotifier::new(secrets);
        let chat_id = notifier.resolve_chat_id().unwrap();
        assert_eq!(chat_id, "12345678");
    }

    #[test]
    fn test_resolve_chat_id_missing() {
        let (secrets, _temp_dir) = create_test_secrets();
        let notifier = TelegramNotifier::new(secrets);

        let result = notifier.resolve_chat_id();

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No default Telegram chat id")
        );
    }

    #[test]
    fn test_format_message_success() {
        let (secrets, _temp_dir) = create_test_secrets();
        let notifier = TelegramNotifier::new(secrets);

        let task = create_test_task();
        let message = notifier.format_message(&task, true, "Task output here");

        assert_eq!(message, "Task output here");
        assert!(!message.contains("✅"));
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

        let message = notifier.format_message(&task, false, "Done");

        assert!(message.contains("Process this data"));
    }

    #[test]
    fn test_format_message_truncates_long_input() {
        let (secrets, _temp_dir) = create_test_secrets();
        let notifier = TelegramNotifier::new(secrets);

        let mut task = create_test_task();
        task.input = Some("x".repeat(200));

        let message = notifier.format_message(&task, false, "Done");

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
        let long_output = "y".repeat(5000);
        let message = notifier.format_message(&task, true, &long_output);

        // Should contain truncation indicator
        assert!(message.contains("truncated"));
        // Should not contain the full 5000 y's
        assert!(!message.contains(&"y".repeat(4000)));
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
        assert!(result.unwrap_err().to_string().contains("chat id"));
    }

    #[tokio::test]
    async fn test_send_empty_chat_id() {
        let (secrets, _temp_dir) = create_test_secrets();
        secrets
            .set_secret(TELEGRAM_BOT_TOKEN_SECRET, "test-token", None)
            .unwrap();
        secrets
            .set_secret(TELEGRAM_CHAT_ID_SECRET, "   ", None)
            .unwrap();

        let notifier = TelegramNotifier::new(secrets);
        let task = create_test_task();

        let config = NotificationConfig::default();

        let result = notifier.send(&config, &task, true, "output").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("chat id"));
    }
}
