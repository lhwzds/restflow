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
