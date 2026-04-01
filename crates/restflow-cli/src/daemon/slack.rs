use anyhow::Result;
use restflow_core::channel::SlackChannel;
use restflow_core::storage::SecretStorage;

/// Set up the Slack channel, returning the channel and optional default channel ID.
///
/// Requires both `SLACK_BOT_TOKEN` and `SLACK_APP_TOKEN` to be configured.
pub fn setup_slack_channel(
    secrets: &SecretStorage,
) -> Result<Option<(SlackChannel, Option<String>)>> {
    let bot_token = secrets.get_non_empty("SLACK_BOT_TOKEN")?;
    let app_token = secrets.get_non_empty("SLACK_APP_TOKEN")?;

    let (Some(bot_token), Some(app_token)) = (bot_token, app_token) else {
        return Ok(None);
    };

    let default_channel_id = secrets.get_non_empty("SLACK_CHANNEL_ID")?;

    let channel = SlackChannel::with_tokens(&bot_token, &app_token);

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
    fn test_setup_slack_without_tokens() {
        let _lock = env_lock();
        let (secrets, _temp_dir, _restflow_dir_guard, _master_key_guard) = setup_secrets();

        let result = setup_slack_channel(&secrets).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_setup_slack_with_both_tokens() {
        let _lock = env_lock();
        let (secrets, _temp_dir, _restflow_dir_guard, _master_key_guard) = setup_secrets();

        secrets
            .set_secret("SLACK_BOT_TOKEN", "xoxb-bot", None)
            .unwrap();
        secrets
            .set_secret("SLACK_APP_TOKEN", "xapp-app", None)
            .unwrap();

        let result = setup_slack_channel(&secrets).unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_setup_slack_without_app_token() {
        let _lock = env_lock();
        let (secrets, _temp_dir, _restflow_dir_guard, _master_key_guard) = setup_secrets();

        secrets
            .set_secret("SLACK_BOT_TOKEN", "xoxb-bot", None)
            .unwrap();

        let result = setup_slack_channel(&secrets).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_setup_slack_without_bot_token() {
        let _lock = env_lock();
        let (secrets, _temp_dir, _restflow_dir_guard, _master_key_guard) = setup_secrets();

        secrets
            .set_secret("SLACK_APP_TOKEN", "xapp-app", None)
            .unwrap();

        let result = setup_slack_channel(&secrets).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_setup_slack_with_default_channel() {
        let _lock = env_lock();
        let (secrets, _temp_dir, _restflow_dir_guard, _master_key_guard) = setup_secrets();

        secrets
            .set_secret("SLACK_BOT_TOKEN", "xoxb-bot", None)
            .unwrap();
        secrets
            .set_secret("SLACK_APP_TOKEN", "xapp-app", None)
            .unwrap();
        secrets
            .set_secret("SLACK_CHANNEL_ID", "C123456", None)
            .unwrap();

        let (_, default_channel_id) = setup_slack_channel(&secrets).unwrap().unwrap();
        assert_eq!(default_channel_id, Some("C123456".to_string()));
    }
}
