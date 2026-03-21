//! Typed provider health snapshot storage wrapper.

use std::sync::Arc;

use anyhow::{Context, Result};
use redb::Database;
use restflow_storage::SimpleStorage;

use crate::models::ExecutionTraceEvent;

/// Typed storage wrapper for provider health projection events.
#[derive(Clone)]
pub struct ProviderHealthSnapshotStorage {
    inner: restflow_storage::ProviderHealthSnapshotStorage,
}

impl ProviderHealthSnapshotStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Ok(Self {
            inner: restflow_storage::ProviderHealthSnapshotStorage::new(db)?,
        })
    }

    pub fn store(&self, event: &ExecutionTraceEvent) -> Result<()> {
        let key = format!("{}:{:020}:{}", event.task_id, event.timestamp, event.id);
        let bytes =
            serde_json::to_vec(event).context("Failed to serialize provider health event")?;
        self.inner.put_raw(&key, &bytes)?;
        Ok(())
    }

    pub fn list_all(&self) -> Result<Vec<ExecutionTraceEvent>> {
        let mut events = self
            .inner
            .list_raw()?
            .into_iter()
            .filter_map(|(_, bytes)| serde_json::from_slice::<ExecutionTraceEvent>(&bytes).ok())
            .collect::<Vec<_>>();
        events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp).then_with(|| a.id.cmp(&b.id)));
        Ok(events)
    }
}
