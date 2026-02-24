use anyhow::Result;
use restflow_core::channel::TelegramChannel;
use restflow_core::storage::{DaemonStateStorage, SecretStorage};
use std::sync::Arc;
use tracing::warn;

const TELEGRAM_LAST_UPDATE_KEY: &str = "telegram_last_update_id";

/// Set up the Telegram channel, returning the channel and optional default chat ID.
pub fn setup_telegram_channel(
    secrets: &SecretStorage,
    daemon_state: &DaemonStateStorage,
) -> Result<Option<(TelegramChannel, Option<String>)>> {
    let token = secrets.get_non_empty("TELEGRAM_BOT_TOKEN")?;

    let Some(token) = token else {
        return Ok(None);
    };

    let default_chat_id = secrets
        .get_non_empty("TELEGRAM_CHAT_ID")?
        .or(secrets.get_non_empty("TELEGRAM_DEFAULT_CHAT_ID")?);

    let initial_offset = daemon_state.get_i64(TELEGRAM_LAST_UPDATE_KEY)?;
    let daemon_state = daemon_state.clone();
    let persister: Arc<dyn Fn(i64) + Send + Sync> = Arc::new(move |update_id| {
        if let Err(error) = daemon_state.set_i64(TELEGRAM_LAST_UPDATE_KEY, update_id) {
            warn!(
                "Failed to persist Telegram last_update_id {}: {}",
                update_id, error
            );
        }
    });

    let channel = TelegramChannel::with_token(&token)
        .with_last_update_id(initial_offset)
        .with_offset_persister(persister);

    Ok(Some((channel, default_chat_id)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::Database;
    use tempfile::tempdir;

    #[test]
    fn test_setup_telegram_channel_without_token() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let secrets = SecretStorage::new(db.clone()).unwrap();
        let daemon_state = DaemonStateStorage::new(db).unwrap();

        let result = setup_telegram_channel(&secrets, &daemon_state).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_setup_telegram_channel_with_default_chat_id() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let secrets = SecretStorage::new(db.clone()).unwrap();
        let daemon_state = DaemonStateStorage::new(db).unwrap();

        secrets
            .set_secret("TELEGRAM_BOT_TOKEN", "bot-token", None)
            .unwrap();
        secrets
            .set_secret("TELEGRAM_CHAT_ID", "12345678", None)
            .unwrap();

        let (_, default_chat_id) = setup_telegram_channel(&secrets, &daemon_state)
            .unwrap()
            .unwrap();
        assert_eq!(default_chat_id, Some("12345678".to_string()));
    }

    #[test]
    fn test_setup_telegram_channel_with_legacy_default_chat_id() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let secrets = SecretStorage::new(db.clone()).unwrap();
        let daemon_state = DaemonStateStorage::new(db).unwrap();

        secrets
            .set_secret("TELEGRAM_BOT_TOKEN", "bot-token", None)
            .unwrap();
        secrets
            .set_secret("TELEGRAM_DEFAULT_CHAT_ID", "87654321", None)
            .unwrap();

        let (_, default_chat_id) = setup_telegram_channel(&secrets, &daemon_state)
            .unwrap()
            .unwrap();
        assert_eq!(default_chat_id, Some("87654321".to_string()));
    }
}
