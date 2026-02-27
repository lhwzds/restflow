//! Typed tool trace storage wrapper.
//!
//! Persists append-only execution timeline events for executions.

use crate::models::ToolTrace;
use anyhow::Result;
use redb::Database;
use restflow_storage::SimpleStorage;
use std::sync::Arc;

/// Typed storage wrapper for tool traces.
#[derive(Debug, Clone)]
pub struct ToolTraceStorage {
    inner: restflow_storage::ToolTraceStorage,
}

impl ToolTraceStorage {
    /// Create storage wrapper.
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Ok(Self {
            inner: restflow_storage::ToolTraceStorage::new(db)?,
        })
    }

    /// Append an execution event.
    pub fn append(&self, event: &ToolTrace) -> Result<()> {
        let key = format!("{}:{:020}:{}", event.session_id, event.created_at, event.id);
        let bytes = serde_json::to_vec(event)?;
        self.inner.put_raw(&key, &bytes)
    }

    /// List events for a session, ordered by timestamp ascending.
    pub fn list_by_session(
        &self,
        session_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<ToolTrace>> {
        let prefix = format!("{session_id}:");
        let mut events = self
            .inner
            .list_raw()?
            .into_iter()
            .filter(|(key, _)| key.starts_with(&prefix))
            .filter_map(|(_, value)| serde_json::from_slice::<ToolTrace>(&value).ok())
            .collect::<Vec<_>>();
        events.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        if let Some(max) = limit
            && events.len() > max
        {
            let keep_from = events.len().saturating_sub(max);
            events = events[keep_from..].to_vec();
        }
        Ok(events)
    }

    /// List events for a specific session turn, ordered by timestamp ascending.
    pub fn list_by_session_turn(
        &self,
        session_id: &str,
        turn_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<ToolTrace>> {
        let mut events = self
            .list_by_session(session_id, None)?
            .into_iter()
            .filter(|event| event.turn_id == turn_id)
            .collect::<Vec<_>>();
        if let Some(max) = limit
            && events.len() > max
        {
            let keep_from = events.len().saturating_sub(max);
            events = events[keep_from..].to_vec();
        }
        Ok(events)
    }

    /// Delete all events for a session.
    pub fn delete_by_session(&self, session_id: &str) -> Result<usize> {
        let prefix = format!("{session_id}:");
        let keys = self
            .inner
            .list_raw()?
            .into_iter()
            .map(|(key, _)| key)
            .filter(|key| key.starts_with(&prefix))
            .collect::<Vec<_>>();
        let mut deleted = 0usize;
        for key in keys {
            if self.inner.delete(&key)? {
                deleted += 1;
            }
        }
        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ToolTrace;
    use redb::Database;
    use tempfile::tempdir;

    fn setup_storage() -> ToolTraceStorage {
        let temp_dir = tempdir().expect("tempdir");
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).expect("db"));
        ToolTraceStorage::new(db).expect("storage")
    }

    #[test]
    fn test_append_and_list_by_session_turn() {
        let storage = setup_storage();
        let e1 = ToolTrace::turn_started("session-1", "turn-1");
        let e2 = ToolTrace::turn_completed("session-1", "turn-1");
        storage.append(&e1).expect("append e1");
        storage.append(&e2).expect("append e2");

        let events = storage
            .list_by_session_turn("session-1", "turn-1", None)
            .expect("list");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].turn_id, "turn-1");
        assert_eq!(events[1].turn_id, "turn-1");
    }

    #[test]
    fn test_delete_by_session() {
        let storage = setup_storage();
        storage
            .append(&ToolTrace::turn_started("session-1", "turn-1"))
            .expect("append session-1");
        storage
            .append(&ToolTrace::turn_started("session-2", "turn-2"))
            .expect("append session-2");

        let deleted = storage.delete_by_session("session-1").expect("delete");
        assert_eq!(deleted, 1);
        let remaining = storage
            .list_by_session("session-2", None)
            .expect("remaining");
        assert_eq!(remaining.len(), 1);
    }
}
