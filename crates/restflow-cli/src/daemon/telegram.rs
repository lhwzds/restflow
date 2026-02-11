use anyhow::Result;
use restflow_core::channel::{ChannelRouter, TelegramChannel};
use restflow_core::storage::{DaemonStateStorage, SecretStorage};
use std::sync::Arc;
use tracing::warn;

fn non_empty_secret(secrets: &SecretStorage, key: &str) -> Result<Option<String>> {
    Ok(secrets
        .get_secret(key)?
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty()))
}

const TELEGRAM_LAST_UPDATE_KEY: &str = "telegram_last_update_id";

pub fn setup_telegram_channel(
    secrets: &SecretStorage,
    daemon_state: &DaemonStateStorage,
) -> Result<Option<Arc<ChannelRouter>>> {
    let token = non_empty_secret(secrets, "TELEGRAM_BOT_TOKEN")?;

    let Some(token) = token else {
        return Ok(None);
    };

    let default_chat_id = non_empty_secret(secrets, "TELEGRAM_CHAT_ID")?
        .or(non_empty_secret(secrets, "TELEGRAM_DEFAULT_CHAT_ID")?);

    let mut router = ChannelRouter::new();
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
    if let Some(chat_id) = default_chat_id {
        router.register_with_default(channel, chat_id);
    } else {
        warn!(
            "Telegram channel registered without default chat ID. Notifications will only work \
if a user interacts with the bot first. Set TELEGRAM_CHAT_ID for reliable notifications."
        );
        router.register(channel);
    }

    Ok(Some(Arc::new(router)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::Database;
    use restflow_core::channel::ChannelType;
    use tempfile::tempdir;

    #[test]
    fn test_setup_telegram_channel_without_token() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let secrets = SecretStorage::new(db.clone()).unwrap();
        let daemon_state = DaemonStateStorage::new(db).unwrap();

        let router = setup_telegram_channel(&secrets, &daemon_state).unwrap();
        assert!(router.is_none());
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

        let router = setup_telegram_channel(&secrets, &daemon_state)
            .unwrap()
            .unwrap();
        assert!(router.has_default_conversation(ChannelType::Telegram));
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

        let router = setup_telegram_channel(&secrets, &daemon_state)
            .unwrap()
            .unwrap();
        assert!(router.has_default_conversation(ChannelType::Telegram));
    }
}
