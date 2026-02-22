use anyhow::Result;
use restflow_core::channel::DiscordChannel;
use restflow_core::storage::SecretStorage;

fn non_empty_secret(secrets: &SecretStorage, key: &str) -> Result<Option<String>> {
    Ok(secrets
        .get_secret(key)?
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty()))
}

/// Set up the Discord channel, returning the channel and optional default channel ID.
pub fn setup_discord_channel(
    secrets: &SecretStorage,
) -> Result<Option<(DiscordChannel, Option<String>)>> {
    let token = non_empty_secret(secrets, "DISCORD_BOT_TOKEN")?;

    let Some(token) = token else {
        return Ok(None);
    };

    let default_channel_id = non_empty_secret(secrets, "DISCORD_CHANNEL_ID")?;

    let channel = DiscordChannel::with_token(&token);

    Ok(Some((channel, default_channel_id)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::Database;
    use std::sync::Arc;
    use tempfile::tempdir;

    #[test]
    fn test_setup_discord_without_token() {
        let temp_dir = tempdir().unwrap();
        let db = Arc::new(Database::create(temp_dir.path().join("test.db")).unwrap());
        let secrets = SecretStorage::new(db).unwrap();

        let result = setup_discord_channel(&secrets).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_setup_discord_with_token() {
        let temp_dir = tempdir().unwrap();
        let db = Arc::new(Database::create(temp_dir.path().join("test.db")).unwrap());
        let secrets = SecretStorage::new(db).unwrap();

        secrets
            .set_secret("DISCORD_BOT_TOKEN", "bot-token", None)
            .unwrap();

        let result = setup_discord_channel(&secrets).unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_setup_discord_with_default_channel() {
        let temp_dir = tempdir().unwrap();
        let db = Arc::new(Database::create(temp_dir.path().join("test.db")).unwrap());
        let secrets = SecretStorage::new(db).unwrap();

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
        let temp_dir = tempdir().unwrap();
        let db = Arc::new(Database::create(temp_dir.path().join("test.db")).unwrap());
        let secrets = SecretStorage::new(db).unwrap();

        secrets
            .set_secret("DISCORD_BOT_TOKEN", "  ", None)
            .unwrap();

        let result = setup_discord_channel(&secrets).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_setup_discord_ignores_whitespace_channel() {
        let temp_dir = tempdir().unwrap();
        let db = Arc::new(Database::create(temp_dir.path().join("test.db")).unwrap());
        let secrets = SecretStorage::new(db).unwrap();

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
