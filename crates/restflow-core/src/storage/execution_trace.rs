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

    /// Access the underlying database for related projection stores.
    pub fn db(&self) -> Arc<Database> {
        self.inner.db().clone()
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
                if let Some(ref run_id) = query.run_id
                    && event.run_id.as_deref() != Some(run_id.as_str())
                {
                    continue;
                }
                if let Some(ref session_id) = query.session_id
                    && event.session_id.as_deref() != Some(session_id.as_str())
                {
                    continue;
                }
                if let Some(ref turn_id) = query.turn_id
                    && event.turn_id.as_deref() != Some(turn_id.as_str())
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
        let mut metric_sample_count = 0u64;
        let mut provider_health_count = 0u64;
        let mut log_record_count = 0u64;
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
                    ExecutionTraceCategory::MetricSample => {
                        metric_sample_count += 1;
                    }
                    ExecutionTraceCategory::ProviderHealth => {
                        provider_health_count += 1;
                    }
                    ExecutionTraceCategory::LogRecord => {
                        log_record_count += 1;
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
            metric_sample_count,
            provider_health_count,
            log_record_count,
            total_tokens,
            total_cost_usd,
            time_range,
        })
    }

    /// Delete all execution trace events associated with a session.
    pub fn delete_by_session(&self, session_id: &str) -> Result<usize> {
        let entries = self
            .inner
            .list_raw()
            .context("Failed to list execution trace events for delete")?;

        let matching_keys = entries
            .into_iter()
            .filter_map(|(key, bytes)| {
                serde_json::from_slice::<ExecutionTraceEvent>(&bytes)
                    .ok()
                    .filter(|event| event.session_id.as_deref() == Some(session_id))
                    .map(|_| key)
            })
            .collect::<Vec<_>>();

        let mut deleted = 0usize;
        for key in matching_keys {
            if self
                .inner
                .delete(&key)
                .context("Failed to delete execution trace event")?
            {
                deleted += 1;
            }
        }

        Ok(deleted)
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

    #[test]
    fn test_query_filters_and_stats_include_telemetry_categories() {
        let storage = ExecutionTraceStorage::in_memory().unwrap();
        let base_trace =
            restflow_telemetry::RestflowTrace::new("run-1", "session-1", "task-1", "agent-1");

        let llm = ExecutionTraceEvent::llm_call(
            "task-1",
            "agent-1",
            LlmCallTrace {
                model: "minimax-coding-plan-m2-5".to_string(),
                input_tokens: Some(10),
                output_tokens: Some(5),
                total_tokens: Some(15),
                cost_usd: Some(0.1),
                duration_ms: Some(100),
                is_reasoning: Some(false),
                message_count: Some(2),
            },
        )
        .with_trace_context(&base_trace);
        let metric = ExecutionTraceEvent::metric_sample(
            "task-1",
            "agent-1",
            crate::models::MetricSampleTrace {
                name: "llm_total_tokens".to_string(),
                value: 15.0,
                unit: Some("tokens".to_string()),
                dimensions: Vec::new(),
            },
        )
        .with_trace_context(&base_trace);
        let health = ExecutionTraceEvent::provider_health(
            "task-1",
            "agent-1",
            crate::models::ProviderHealthTrace {
                provider: "minimax-coding-plan".to_string(),
                model: Some("minimax-coding-plan-m2-5-highspeed".to_string()),
                status: "degraded".to_string(),
                reason: Some("failover".to_string()),
                error_kind: None,
            },
        )
        .with_trace_context(&base_trace);
        let log = ExecutionTraceEvent::log_record(
            "task-1",
            "agent-1",
            crate::models::LogRecordTrace {
                level: "warn".to_string(),
                message: "failover".to_string(),
                fields: Vec::new(),
            },
        )
        .with_trace_context(&base_trace);

        for event in [&llm, &metric, &health, &log] {
            storage.store(event).unwrap();
        }

        let results = storage
            .query(&ExecutionTraceQuery {
                task_id: Some("task-1".to_string()),
                run_id: Some("run-1".to_string()),
                session_id: Some("session-1".to_string()),
                turn_id: Some("run-run-1".to_string()),
                ..ExecutionTraceQuery::default()
            })
            .unwrap();
        assert_eq!(results.len(), 4);

        let stats = storage.stats(Some("task-1")).unwrap();
        assert_eq!(stats.metric_sample_count, 1);
        assert_eq!(stats.provider_health_count, 1);
        assert_eq!(stats.log_record_count, 1);
        assert_eq!(stats.llm_call_count, 1);
    }
}
