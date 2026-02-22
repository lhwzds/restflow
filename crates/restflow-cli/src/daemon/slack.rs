use anyhow::Result;
use restflow_core::channel::SlackChannel;
use restflow_core::storage::SecretStorage;

fn non_empty_secret(secrets: &SecretStorage, key: &str) -> Result<Option<String>> {
    Ok(secrets
        .get_secret(key)?
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty()))
}

/// Set up the Slack channel, returning the channel and optional default channel ID.
///
/// Requires both `SLACK_BOT_TOKEN` and `SLACK_APP_TOKEN` to be configured.
pub fn setup_slack_channel(
    secrets: &SecretStorage,
) -> Result<Option<(SlackChannel, Option<String>)>> {
    let bot_token = non_empty_secret(secrets, "SLACK_BOT_TOKEN")?;
    let app_token = non_empty_secret(secrets, "SLACK_APP_TOKEN")?;

    let (Some(bot_token), Some(app_token)) = (bot_token, app_token) else {
        return Ok(None);
    };

    let default_channel_id = non_empty_secret(secrets, "SLACK_CHANNEL_ID")?;

    let channel = SlackChannel::with_tokens(&bot_token, &app_token);

    Ok(Some((channel, default_channel_id)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::Database;
    use std::sync::Arc;
    use tempfile::tempdir;

    #[test]
    fn test_setup_slack_without_tokens() {
        let temp_dir = tempdir().unwrap();
        let db = Arc::new(Database::create(temp_dir.path().join("test.db")).unwrap());
        let secrets = SecretStorage::new(db).unwrap();

        let result = setup_slack_channel(&secrets).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_setup_slack_with_both_tokens() {
        let temp_dir = tempdir().unwrap();
        let db = Arc::new(Database::create(temp_dir.path().join("test.db")).unwrap());
        let secrets = SecretStorage::new(db).unwrap();

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
        let temp_dir = tempdir().unwrap();
        let db = Arc::new(Database::create(temp_dir.path().join("test.db")).unwrap());
        let secrets = SecretStorage::new(db).unwrap();

        secrets
            .set_secret("SLACK_BOT_TOKEN", "xoxb-bot", None)
            .unwrap();

        let result = setup_slack_channel(&secrets).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_setup_slack_without_bot_token() {
        let temp_dir = tempdir().unwrap();
        let db = Arc::new(Database::create(temp_dir.path().join("test.db")).unwrap());
        let secrets = SecretStorage::new(db).unwrap();

        secrets
            .set_secret("SLACK_APP_TOKEN", "xapp-app", None)
            .unwrap();

        let result = setup_slack_channel(&secrets).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_setup_slack_with_default_channel() {
        let temp_dir = tempdir().unwrap();
        let db = Arc::new(Database::create(temp_dir.path().join("test.db")).unwrap());
        let secrets = SecretStorage::new(db).unwrap();

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
