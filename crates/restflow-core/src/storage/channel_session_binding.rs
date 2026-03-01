//! Typed channel-session binding storage wrapper.
//!
//! Provides type-safe CRUD for persisted channel route -> chat session mappings.

use crate::models::ChannelSessionBinding;
use anyhow::Result;
use redb::Database;
use restflow_storage::SimpleStorage;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ChannelSessionBindingStorage {
    inner: restflow_storage::ChannelSessionBindingStorage,
}

impl ChannelSessionBindingStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Ok(Self {
            inner: restflow_storage::ChannelSessionBindingStorage::new(db)?,
        })
    }

    pub fn upsert(&self, binding: &ChannelSessionBinding) -> Result<()> {
        let mut normalized = binding.clone();
        normalized.touch();
        let key = normalized.route_key();
        let payload = serde_json::to_vec(&normalized)?;
        self.inner.put_raw(&key, &payload)
    }

    pub fn get_by_route(
        &self,
        channel: &str,
        account_id: Option<&str>,
        conversation_id: &str,
    ) -> Result<Option<ChannelSessionBinding>> {
        let key = build_route_key(channel, account_id, conversation_id);
        if let Some(bytes) = self.inner.get_raw(&key)? {
            let binding: ChannelSessionBinding = serde_json::from_slice(&bytes)?;
            return Ok(Some(binding));
        }
        Ok(None)
    }

    pub fn remove_by_route(
        &self,
        channel: &str,
        account_id: Option<&str>,
        conversation_id: &str,
    ) -> Result<bool> {
        let key = build_route_key(channel, account_id, conversation_id);
        self.inner.delete(&key)
    }

    pub fn list(&self) -> Result<Vec<ChannelSessionBinding>> {
        let mut out = Vec::new();
        for (_key, bytes) in self.inner.list_raw()? {
            let binding: ChannelSessionBinding = serde_json::from_slice(&bytes)?;
            out.push(binding);
        }
        out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(out)
    }

    pub fn list_by_session(&self, session_id: &str) -> Result<Vec<ChannelSessionBinding>> {
        let target = session_id.trim();
        let mut out = self
            .list()?
            .into_iter()
            .filter(|binding| binding.session_id == target)
            .collect::<Vec<_>>();
        out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(out)
    }
}

fn build_route_key(channel: &str, account_id: Option<&str>, conversation_id: &str) -> String {
    let normalized_channel = normalize_segment(channel);
    let normalized_account = normalize_segment(account_id.unwrap_or("*"));
    format!(
        "{}:{}:{}",
        normalized_channel,
        normalized_account,
        conversation_id.trim()
    )
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
    use tempfile::tempdir;

    fn setup() -> (ChannelSessionBindingStorage, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Arc::new(redb::Database::create(db_path).unwrap());
        (ChannelSessionBindingStorage::new(db).unwrap(), dir)
    }

    #[test]
    fn upsert_and_get_by_route() {
        let (storage, _dir) = setup();
        let binding = ChannelSessionBinding::new("telegram", None, "chat-1", "session-1");
        storage.upsert(&binding).unwrap();

        let fetched = storage
            .get_by_route("telegram", None, "chat-1")
            .unwrap()
            .unwrap();
        assert_eq!(fetched.session_id, "session-1");
        assert_eq!(fetched.channel, "telegram");
    }

    #[test]
    fn get_by_route_is_case_insensitive_for_route_segments() {
        let (storage, _dir) = setup();
        let binding =
            ChannelSessionBinding::new("Telegram", Some("BotA".to_string()), "chat-2", "sess-2");
        storage.upsert(&binding).unwrap();

        let fetched = storage
            .get_by_route("telegram", Some("bota"), "chat-2")
            .unwrap();
        assert!(fetched.is_some());
    }

    #[test]
    fn list_by_session_filters_bindings() {
        let (storage, _dir) = setup();
        storage
            .upsert(&ChannelSessionBinding::new(
                "telegram",
                None,
                "chat-a",
                "session-x",
            ))
            .unwrap();
        storage
            .upsert(&ChannelSessionBinding::new(
                "discord",
                None,
                "chat-b",
                "session-y",
            ))
            .unwrap();
        storage
            .upsert(&ChannelSessionBinding::new(
                "slack",
                Some("acct-1".to_string()),
                "chat-c",
                "session-x",
            ))
            .unwrap();

        let bound = storage.list_by_session("session-x").unwrap();
        assert_eq!(bound.len(), 2);
        assert!(
            bound
                .iter()
                .all(|binding| binding.session_id == "session-x")
        );
    }

    #[test]
    fn remove_by_route_removes_binding() {
        let (storage, _dir) = setup();
        let binding = ChannelSessionBinding::new("telegram", None, "chat-9", "session-9");
        storage.upsert(&binding).unwrap();

        assert!(storage.remove_by_route("telegram", None, "chat-9").unwrap());
        assert!(
            storage
                .get_by_route("telegram", None, "chat-9")
                .unwrap()
                .is_none()
        );
    }
}
