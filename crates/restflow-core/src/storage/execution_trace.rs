//! Typed execution trace storage wrapper.

use std::sync::Arc;

use anyhow::{Context, Result};
use redb::Database;
use restflow_storage::SimpleStorage;

use crate::models::execution_trace::{
    ExecutionTraceCategory, ExecutionTraceEvent, ExecutionTraceQuery, ExecutionTraceStats,
    ExecutionTraceTimeRange,
};

/// Typed execution trace storage wrapper around `restflow-storage`.
#[derive(Clone)]
pub struct ExecutionTraceStorage {
    inner: restflow_storage::ExecutionTraceStorageBackend,
}

impl ExecutionTraceStorage {
    /// Create an execution trace storage with an existing database.
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Ok(Self {
            inner: restflow_storage::ExecutionTraceStorageBackend::new(db)?,
        })
    }

    /// Create an in-memory execution trace storage (for testing).
    #[allow(dead_code)]
    pub fn in_memory() -> Result<Self> {
        let db = Arc::new(
            Database::builder()
                .create_with_backend(redb::backends::InMemoryBackend::new())
                .context("Failed to create in-memory database")?,
        );
        Ok(Self {
            inner: restflow_storage::ExecutionTraceStorageBackend::new(db)?,
        })
    }

    /// Store an execution trace event.
    pub fn store(&self, event: &ExecutionTraceEvent) -> Result<()> {
        let key = format!("{}:{}", event.task_id, event.id);
        let bytes =
            serde_json::to_vec(event).context("Failed to serialize execution trace event")?;
        self.inner
            .put_raw(&key, &bytes)
            .context("Failed to store execution trace event")?;
        Ok(())
    }

    /// Query execution trace events with filters.
    pub fn query(&self, query: &ExecutionTraceQuery) -> Result<Vec<ExecutionTraceEvent>> {
        let raw_entries = self
            .inner
            .list_raw()
            .context("Failed to list execution trace events")?;

        let mut events = Vec::new();
        let offset = query.offset.unwrap_or(0);
        let limit = query.limit.unwrap_or(100);

        for (_, bytes) in raw_entries {
            if let Ok(event) = serde_json::from_slice::<ExecutionTraceEvent>(&bytes) {
                if let Some(ref task_id) = query.task_id
                    && event.task_id != *task_id
                {
                    continue;
                }
                if let Some(ref agent_id) = query.agent_id
                    && event.agent_id != *agent_id
                {
                    continue;
                }
                if let Some(ref category) = query.category
                    && event.category != *category
                {
                    continue;
                }
                if let Some(ref source) = query.source
                    && event.source != *source
                {
                    continue;
                }
                if let Some(from) = query.from_timestamp
                    && event.timestamp < from
                {
                    continue;
                }
                if let Some(to) = query.to_timestamp
                    && event.timestamp > to
                {
                    continue;
                }

                events.push(event);
            }
        }

        events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        let events: Vec<_> = events.into_iter().skip(offset).take(limit).collect();
        Ok(events)
    }

    /// Get a single execution trace event by event ID.
    pub fn get_by_id(&self, event_id: &str) -> Result<Option<ExecutionTraceEvent>> {
        let raw_entries = self
            .inner
            .list_raw()
            .context("Failed to list execution trace events")?;

        for (_, bytes) in raw_entries {
            if let Ok(event) = serde_json::from_slice::<ExecutionTraceEvent>(&bytes)
                && event.id == event_id
            {
                return Ok(Some(event));
            }
        }

        Ok(None)
    }

    /// Get statistics about execution trace events.
    pub fn stats(&self, task_id: Option<&str>) -> Result<ExecutionTraceStats> {
        let raw_entries = self
            .inner
            .list_raw()
            .context("Failed to list execution trace events")?;

        let mut total_events = 0u64;
        let mut llm_call_count = 0u64;
        let mut tool_call_count = 0u64;
        let mut model_switch_count = 0u64;
        let mut lifecycle_count = 0u64;
        let mut message_count = 0u64;
        let mut total_tokens = 0u64;
        let mut total_cost_usd = 0.0f64;
        let mut earliest: Option<i64> = None;
        let mut latest: Option<i64> = None;

        for (_, bytes) in raw_entries {
            if let Ok(event) = serde_json::from_slice::<ExecutionTraceEvent>(&bytes) {
                if let Some(tid) = task_id
                    && event.task_id != tid
                {
                    continue;
                }

                total_events += 1;

                match event.category {
                    ExecutionTraceCategory::LlmCall => {
                        llm_call_count += 1;
                        if let Some(ref llm) = event.llm_call {
                            total_tokens += llm.total_tokens.unwrap_or(0) as u64;
                            total_cost_usd += llm.cost_usd.unwrap_or(0.0);
                        }
                    }
                    ExecutionTraceCategory::ToolCall => {
                        tool_call_count += 1;
                    }
                    ExecutionTraceCategory::ModelSwitch => {
                        model_switch_count += 1;
                    }
                    ExecutionTraceCategory::Lifecycle => {
                        lifecycle_count += 1;
                    }
                    ExecutionTraceCategory::Message => {
                        message_count += 1;
                    }
                }

                earliest = Some(earliest.map_or(event.timestamp, |e| e.min(event.timestamp)));
                latest = Some(latest.map_or(event.timestamp, |l| l.max(event.timestamp)));
            }
        }

        let time_range = match (earliest, latest) {
            (Some(e), Some(l)) => Some(ExecutionTraceTimeRange {
                earliest: e,
                latest: l,
            }),
            _ => None,
        };

        Ok(ExecutionTraceStats {
            total_events,
            llm_call_count,
            tool_call_count,
            model_switch_count,
            lifecycle_count,
            message_count,
            total_tokens,
            total_cost_usd,
            time_range,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::execution_trace::LlmCallTrace;

    #[test]
    fn test_execution_trace_storage() {
        let storage = ExecutionTraceStorage::in_memory().unwrap();

        let event = ExecutionTraceEvent::llm_call(
            "task-123",
            "agent-456",
            LlmCallTrace {
                model: "claude-sonnet-4-20250514".to_string(),
                input_tokens: Some(1000),
                output_tokens: Some(500),
                total_tokens: Some(1500),
                cost_usd: Some(0.01),
                duration_ms: Some(1500),
                is_reasoning: Some(false),
                message_count: Some(10),
            },
        );

        storage.store(&event).unwrap();

        let query = ExecutionTraceQuery {
            task_id: Some("task-123".to_string()),
            ..Default::default()
        };

        let results = storage.query(&query).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].task_id, "task-123");

        let stats = storage.stats(Some("task-123")).unwrap();
        assert_eq!(stats.total_events, 1);
        assert_eq!(stats.llm_call_count, 1);

        let fetched = storage.get_by_id(&event.id).unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().id, event.id);
    }
}
