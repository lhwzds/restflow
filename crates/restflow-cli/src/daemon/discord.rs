use anyhow::Result;
use restflow_core::channel::DiscordChannel;
use restflow_core::storage::SecretStorage;

/// Set up the Discord channel, returning the channel and optional default channel ID.
pub fn setup_discord_channel(
    secrets: &SecretStorage,
) -> Result<Option<(DiscordChannel, Option<String>)>> {
    let token = secrets.get_non_empty("DISCORD_BOT_TOKEN")?;

    let Some(token) = token else {
        return Ok(None);
    };

    let default_channel_id = secrets.get_non_empty("DISCORD_CHANNEL_ID")?;

    let channel = DiscordChannel::with_token(&token);

    Ok(Some((channel, default_channel_id)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::Database;
    use std::env;
    use std::path::Path;
    use std::sync::Arc;
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
        crate::test_support::env_lock()
    }

    fn setup_secrets() -> (SecretStorage, tempfile::TempDir, EnvGuard, EnvGuard) {
        let temp_dir = tempdir().unwrap();
        let restflow_dir = temp_dir.path().join("state");
        std::fs::create_dir_all(&restflow_dir).unwrap();
        let restflow_dir_guard = EnvGuard::set_path("RESTFLOW_DIR", &restflow_dir);
        let master_key_guard = EnvGuard::clear("RESTFLOW_MASTER_KEY");
        let db = Arc::new(Database::create(temp_dir.path().join("test.db")).unwrap());
        let secrets = SecretStorage::new(db).unwrap();
        (secrets, temp_dir, restflow_dir_guard, master_key_guard)
    }

    #[test]
    fn test_setup_discord_without_token() {
        let _lock = env_lock();
        let (secrets, _temp_dir, _restflow_dir_guard, _master_key_guard) = setup_secrets();

        let result = setup_discord_channel(&secrets).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_setup_discord_with_token() {
        let _lock = env_lock();
        let (secrets, _temp_dir, _restflow_dir_guard, _master_key_guard) = setup_secrets();

        secrets
            .set_secret("DISCORD_BOT_TOKEN", "bot-token", None)
            .unwrap();

        let result = setup_discord_channel(&secrets).unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_setup_discord_with_default_channel() {
        let _lock = env_lock();
        let (secrets, _temp_dir, _restflow_dir_guard, _master_key_guard) = setup_secrets();

        secrets
            .set_secret("DISCORD_BOT_TOKEN", "bot-token", None)
            .unwrap();
        secrets
            .set_secret("DISCORD_CHANNEL_ID", "123456789", None)
            .unwrap();

        let (_, default_channel_id) = setup_discord_channel(&secrets).unwrap().unwrap();
        assert_eq!(default_channel_id, Some("123456789".to_string()));
    }

    #[test]
    fn test_setup_discord_ignores_empty_token() {
        let _lock = env_lock();
        let (secrets, _temp_dir, _restflow_dir_guard, _master_key_guard) = setup_secrets();

        secrets.set_secret("DISCORD_BOT_TOKEN", "  ", None).unwrap();

        let result = setup_discord_channel(&secrets).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_setup_discord_ignores_whitespace_channel() {
        let _lock = env_lock();
        let (secrets, _temp_dir, _restflow_dir_guard, _master_key_guard) = setup_secrets();

        secrets
            .set_secret("DISCORD_BOT_TOKEN", "bot-token", None)
            .unwrap();
        secrets
            .set_secret("DISCORD_CHANNEL_ID", "  ", None)
            .unwrap();

        let (_, default_channel_id) = setup_discord_channel(&secrets).unwrap().unwrap();
        assert!(default_channel_id.is_none());
    }
}
