//! Telegram notification sender implementation for the task runner.
//!
//! This module provides `TelegramNotifier`, which implements the
//! `NotificationSender` trait to send task completion/failure
//! notifications via Telegram Bot API.

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::{future::Future, sync::Arc};
use tracing::warn;

use crate::{
    models::{BackgroundAgent, NotificationConfig},
    storage::SecretStorage,
};
use restflow_ai::tools::send_telegram_notification;

use super::runner::NotificationSender;

/// Find the largest byte index <= `index` that is a valid char boundary.
fn floor_char_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        return s.len();
    }
    let mut i = index;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

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
    fn format_message(&self, _task: &BackgroundAgent, success: bool, message: &str) -> String {
        let content = if message.trim().is_empty() {
            if success {
                "Task completed successfully."
            } else {
                "Task failed without additional details."
            }
        } else {
            message.trim()
        };

        if content.len() > 3500 {
            let end = floor_char_boundary(content, 3500);
            format!("{}...\n\n(truncated)", &content[..end])
        } else {
            content.to_string()
        }
    }

    pub async fn send_raw(&self, message: &str) -> Result<()> {
        let bot_token = self.resolve_bot_token()?;
        let chat_id = self.resolve_chat_id()?;

        send_telegram_message_with_retry(&bot_token, &chat_id, message, 2).await
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
        let formatted_message = self.format_message(task, success, message);
        self.send_raw(&formatted_message).await
    }

    async fn send_formatted(&self, message: &str) -> Result<()> {
        self.send_raw(message).await
    }
}

fn retry_delay_ms(attempt: u32) -> u64 {
    match attempt {
        1 => 1_000,
        2 => 3_000,
        _ => 5_000,
    }
}

async fn send_with_retry<F, Fut>(max_retries: u32, mut send_attempt: F) -> Result<()>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = std::result::Result<(), String>>,
{
    send_with_retry_with_sleep(max_retries, &mut send_attempt, |attempt| async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(retry_delay_ms(attempt))).await;
    })
    .await
}

async fn send_with_retry_with_sleep<F, Fut, S, SFut>(
    max_retries: u32,
    send_attempt: &mut F,
    mut sleep_fn: S,
) -> Result<()>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = std::result::Result<(), String>>,
    S: FnMut(u32) -> SFut,
    SFut: Future<Output = ()>,
{
    let mut last_error = String::new();

    for attempt in 0..=max_retries {
        if attempt > 0 {
            sleep_fn(attempt).await;
        }

        match send_attempt().await {
            Ok(()) => return Ok(()),
            Err(err) => {
                warn!(
                    attempt = attempt + 1,
                    max_retries = max_retries,
                    error = %err,
                    "Telegram notification delivery failed, will retry"
                );
                last_error = err;
            }
        }
    }

    Err(anyhow!(
        "Telegram notification failed after {} attempts: {}",
        max_retries + 1,
        if last_error.is_empty() {
            "unknown".to_string()
        } else {
            last_error
        }
    ))
}

async fn send_telegram_message_with_retry(
    bot_token: &str,
    chat_id: &str,
    message: &str,
    max_retries: u32,
) -> Result<()> {
    send_with_retry(max_retries, || async {
        send_telegram_notification(bot_token, chat_id, message, None).await
    })
    .await
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

        assert_eq!(message, "Connection timeout");
    }

    #[test]
    fn test_format_message_with_input() {
        let (secrets, _temp_dir) = create_test_secrets();
        let notifier = TelegramNotifier::new(secrets);

        let mut task = create_test_task();
        task.input = Some("Process this data".to_string());

        let message = notifier.format_message(&task, false, "Done");

        assert_eq!(message, "Done");
    }

    #[test]
    fn test_format_message_truncates_long_input() {
        let (secrets, _temp_dir) = create_test_secrets();
        let notifier = TelegramNotifier::new(secrets);

        let mut task = create_test_task();
        task.input = Some("x".repeat(200));

        let message = notifier.format_message(&task, false, "Done");

        assert_eq!(message, "Done");
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
        assert!(failure_msg.contains("without additional details"));
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

    #[test]
    fn test_truncate_notification_multibyte() {
        let (secrets, _temp_dir) = create_test_secrets();
        let notifier = TelegramNotifier::new(secrets);
        let task = create_test_task();

        // Create content where byte 3500 falls mid-CJK character
        // Each CJK char is 3 bytes, so 1167 chars = 3501 bytes
        let emoji_content = "你".repeat(1167);
        assert!(emoji_content.len() > 3500);

        // Should not panic
        let message = notifier.format_message(&task, true, &emoji_content);
        assert!(message.contains("truncated"));
    }

    #[tokio::test]
    async fn test_send_with_retry_succeeds_on_second_attempt() {
        let mut attempts = 0_u32;
        let result = send_with_retry_with_sleep(
            2,
            &mut || {
                attempts += 1;
                async move {
                    if attempts == 1 {
                        Err("transient".to_string())
                    } else {
                        Ok(())
                    }
                }
            },
            |_| async {},
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(attempts, 2);
    }

    #[tokio::test]
    async fn test_send_with_retry_exhausted_returns_error() {
        let mut attempts = 0_u32;
        let result = send_with_retry_with_sleep(
            2,
            &mut || {
                attempts += 1;
                async { Err("always fails".to_string()) }
            },
            |_| async {},
        )
        .await;

        assert!(result.is_err());
        assert_eq!(attempts, 3);
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("failed after 3 attempts")
        );
    }
}
