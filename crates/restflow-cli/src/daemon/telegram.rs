use anyhow::Result;
use restflow_core::channel::TelegramChannel;
use restflow_core::storage::{DaemonStateStorage, SecretStorage};
use restflow_storage::ChannelSettings;
use std::sync::Arc;
use tracing::warn;

const TELEGRAM_LAST_UPDATE_KEY: &str = "telegram_last_update_id";

/// Set up the Telegram channel, returning the channel and optional default chat ID.
pub fn setup_telegram_channel(
    secrets: &SecretStorage,
    daemon_state: &DaemonStateStorage,
    channel_defaults: &ChannelSettings,
) -> Result<Option<(TelegramChannel, Option<String>)>> {
    let token = secrets.get_non_empty("TELEGRAM_BOT_TOKEN")?;

    let Some(token) = token else {
        return Ok(None);
    };

    let default_chat_id = secrets
        .get_non_empty("TELEGRAM_CHAT_ID")?
        .or(secrets.get_non_empty("TELEGRAM_DEFAULT_CHAT_ID")?);

    let initial_offset = match daemon_state.get_i64(TELEGRAM_LAST_UPDATE_KEY) {
        Ok(offset) => offset,
        Err(error) => {
            warn!(
                "Failed to read Telegram last_update_id from daemon state: {}. Falling back to 0.",
                error
            );
            0
        }
    };
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
        .with_api_timeout(channel_defaults.telegram_api_timeout_secs)
        .with_polling_timeout(channel_defaults.telegram_polling_timeout_secs)
        .with_last_update_id(initial_offset)
        .with_offset_persister(persister);

    Ok(Some((channel, default_chat_id)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::Database;
    use restflow_storage::SimpleStorage;
    use std::env;
    use std::path::Path;
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    struct EnvGuard {
        key: &'static str,
        original: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn set_path(key: &'static str, path: &Path) -> Self {
            let original = env::var_os(key);
            unsafe {
                env::set_var(key, path);
            }
            Self { key, original }
        }

        fn clear(key: &'static str) -> Self {
            let original = env::var_os(key);
            unsafe {
                env::remove_var(key);
            }
            Self { key, original }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(value) = &self.original {
                unsafe {
                    env::set_var(self.key, value);
                }
            } else {
                unsafe {
                    env::remove_var(self.key);
                }
            }
        }
    }

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn setup_channel_state(
    ) -> (
        SecretStorage,
        DaemonStateStorage,
        tempfile::TempDir,
        EnvGuard,
        EnvGuard,
    ) {
        let temp_dir = tempdir().unwrap();
        let restflow_dir = temp_dir.path().join("state");
        std::fs::create_dir_all(&restflow_dir).unwrap();
        let restflow_dir_guard = EnvGuard::set_path("RESTFLOW_DIR", &restflow_dir);
        let master_key_guard = EnvGuard::clear("RESTFLOW_MASTER_KEY");
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let secrets = SecretStorage::new(db.clone()).unwrap();
        let daemon_state = DaemonStateStorage::new(db).unwrap();
        (
            secrets,
            daemon_state,
            temp_dir,
            restflow_dir_guard,
            master_key_guard,
        )
    }

    #[test]
    fn test_setup_telegram_channel_without_token() {
        let _lock = env_lock();
        let (secrets, daemon_state, _temp_dir, _restflow_dir_guard, _master_key_guard) =
            setup_channel_state();

        let result =
            setup_telegram_channel(&secrets, &daemon_state, &ChannelSettings::default()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_setup_telegram_channel_with_default_chat_id() {
        let _lock = env_lock();
        let (secrets, daemon_state, _temp_dir, _restflow_dir_guard, _master_key_guard) =
            setup_channel_state();

        secrets
            .set_secret("TELEGRAM_BOT_TOKEN", "bot-token", None)
            .unwrap();
        secrets
            .set_secret("TELEGRAM_CHAT_ID", "12345678", None)
            .unwrap();

        let (_, default_chat_id) =
            setup_telegram_channel(&secrets, &daemon_state, &ChannelSettings::default())
                .unwrap()
                .unwrap();
        assert_eq!(default_chat_id, Some("12345678".to_string()));
    }

    #[test]
    fn test_setup_telegram_channel_with_legacy_default_chat_id() {
        let _lock = env_lock();
        let (secrets, daemon_state, _temp_dir, _restflow_dir_guard, _master_key_guard) =
            setup_channel_state();

        secrets
            .set_secret("TELEGRAM_BOT_TOKEN", "bot-token", None)
            .unwrap();
        secrets
            .set_secret("TELEGRAM_DEFAULT_CHAT_ID", "87654321", None)
            .unwrap();

        let (_, default_chat_id) =
            setup_telegram_channel(&secrets, &daemon_state, &ChannelSettings::default())
                .unwrap()
                .unwrap();
        assert_eq!(default_chat_id, Some("87654321".to_string()));
    }

    #[test]
    fn test_setup_telegram_channel_falls_back_when_offset_is_malformed() {
        let _lock = env_lock();
        let (secrets, daemon_state, _temp_dir, _restflow_dir_guard, _master_key_guard) =
            setup_channel_state();

        secrets
            .set_secret("TELEGRAM_BOT_TOKEN", "bot-token", None)
            .unwrap();
        daemon_state
            .put_raw("telegram_last_update_id", &[1, 2, 3])
            .unwrap();

        let result = setup_telegram_channel(&secrets, &daemon_state, &ChannelSettings::default());
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn test_setup_telegram_channel_applies_channel_defaults() {
        let _lock = env_lock();
        let (secrets, daemon_state, _temp_dir, _restflow_dir_guard, _master_key_guard) =
            setup_channel_state();

        secrets
            .set_secret("TELEGRAM_BOT_TOKEN", "bot-token", None)
            .unwrap();

        let defaults = ChannelSettings {
            telegram_api_timeout_secs: 45,
            telegram_polling_timeout_secs: 60,
        };

        let (channel, _) = setup_telegram_channel(&secrets, &daemon_state, &defaults)
            .unwrap()
            .unwrap();
        assert_eq!(channel.last_update_id(), 0);
        let config = channel.config();
        assert_eq!(config.api_timeout_secs, 45);
        assert_eq!(config.polling_timeout, 60);
    }
}
