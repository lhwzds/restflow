use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Persistent mapping between an external channel route and a chat session.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export)]
pub struct ChannelSessionBinding {
    pub id: String,
    /// Normalized channel identifier (e.g. `telegram`, `discord`, `slack`).
    pub channel: String,
    /// Optional account/bot identifier in this channel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    /// External conversation identifier from channel payload.
    pub conversation_id: String,
    /// Internal chat session ID.
    pub session_id: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl ChannelSessionBinding {
    pub fn new(
        channel: impl Into<String>,
        account_id: Option<String>,
        conversation_id: impl Into<String>,
        session_id: impl Into<String>,
    ) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            channel: normalize_segment(&channel.into()),
            account_id: account_id.map(|value| normalize_segment(&value)),
            conversation_id: conversation_id.into().trim().to_string(),
            session_id: session_id.into().trim().to_string(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Stable lookup key used by storage.
    ///
    /// Format: `{channel}:{account_or_star}:{conversation_id}`.
    pub fn route_key(&self) -> String {
        let account = self.account_id.as_deref().unwrap_or("*");
        format!(
            "{}:{}:{}",
            normalize_segment(&self.channel),
            normalize_segment(account),
            self.conversation_id.trim()
        )
    }

    pub fn touch(&mut self) {
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }
}

fn normalize_segment(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "*".to_string();
    }
    trimmed.to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_key_uses_star_for_missing_account() {
        let binding = ChannelSessionBinding::new("Telegram", None, "chat-1", "sess-1");
        assert_eq!(binding.route_key(), "telegram:*:chat-1");
    }

    #[test]
    fn route_key_normalizes_segments() {
        let binding =
            ChannelSessionBinding::new("DISCORD", Some(" Bot-1 ".to_string()), "conv-42", "sess-2");
        assert_eq!(binding.route_key(), "discord:bot-1:conv-42");
    }

    #[test]
    fn export_bindings_channel_session_binding() {
        ChannelSessionBinding::export_to_string(&ts_rs::Config::default()).unwrap();
    }
}
