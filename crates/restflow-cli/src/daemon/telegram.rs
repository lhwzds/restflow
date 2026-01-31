use anyhow::Result;
use restflow_core::channel::{ChannelRouter, TelegramChannel};
use restflow_core::storage::SecretStorage;
use std::sync::Arc;

pub fn setup_telegram_channel(secrets: &SecretStorage) -> Result<Option<Arc<ChannelRouter>>> {
    let token = secrets
        .get_secret("TELEGRAM_BOT_TOKEN")?
        .filter(|value| !value.trim().is_empty());

    let Some(token) = token else {
        return Ok(None);
    };

    let mut router = ChannelRouter::new();
    let channel = TelegramChannel::with_token(&token);
    router.register(channel);

    Ok(Some(Arc::new(router)))
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
        let secrets = SecretStorage::new(db).unwrap();

        let router = setup_telegram_channel(&secrets).unwrap();
        assert!(router.is_none());
    }
}
