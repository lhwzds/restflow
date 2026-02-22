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
