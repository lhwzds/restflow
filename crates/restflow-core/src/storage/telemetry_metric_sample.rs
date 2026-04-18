//! Typed telemetry metric sample storage wrapper.

use std::sync::Arc;

use anyhow::{Context, Result};
use redb::Database;
use restflow_storage::SimpleStorage;

use crate::models::ExecutionTraceEvent;

/// Typed storage wrapper for metric sample projection events.
#[derive(Clone)]
pub struct TelemetryMetricSampleStorage {
    inner: restflow_storage::TelemetryMetricSampleStorage,
}

impl TelemetryMetricSampleStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Ok(Self {
            inner: restflow_storage::TelemetryMetricSampleStorage::new(db)?,
        })
    }

    pub fn store(&self, event: &ExecutionTraceEvent) -> Result<()> {
        let key = format!("{}:{:020}:{}", event.task_id, event.timestamp, event.id);
        let bytes = serde_json::to_vec(event).context("Failed to serialize metric sample event")?;
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

    pub fn cleanup_older_than(&self, cutoff_ms: i64) -> Result<usize> {
        let entries = self
            .inner
            .list_raw()
            .context("Failed to list telemetry metric samples for cleanup")?;

        let matching_keys = entries
            .into_iter()
            .filter_map(|(key, bytes)| {
                serde_json::from_slice::<ExecutionTraceEvent>(&bytes)
                    .ok()
                    .filter(|event| event.timestamp < cutoff_ms)
                    .map(|_| key)
            })
            .collect::<Vec<_>>();

        self.inner
            .delete_many(&matching_keys)
            .context("Failed to delete old telemetry metric samples")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::execution_trace_builders;

    fn storage() -> TelemetryMetricSampleStorage {
        let db = Arc::new(
            Database::builder()
                .create_with_backend(redb::backends::InMemoryBackend::new())
                .unwrap(),
        );
        TelemetryMetricSampleStorage::new(db).unwrap()
    }

    #[test]
    fn cleanup_older_than_deletes_only_old_samples() {
        let storage = storage();
        let mut old = execution_trace_builders::metric_sample(
            "task-1",
            "agent-1",
            crate::models::MetricSampleTrace {
                name: "tokens".to_string(),
                value: 1.0,
                unit: None,
                dimensions: Vec::new(),
            },
        );
        old.timestamp = 100;
        let mut recent = old.clone();
        recent.id = "recent-sample".to_string();
        recent.timestamp = 300;

        storage.store(&old).unwrap();
        storage.store(&recent).unwrap();

        let deleted = storage.cleanup_older_than(200).unwrap();

        assert_eq!(deleted, 1);
        let remaining = storage.list_all().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, recent.id);
    }
}
