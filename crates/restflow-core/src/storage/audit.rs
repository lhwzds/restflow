//! Audit storage module for persisting audit events.

use std::sync::Arc;

use anyhow::{Context, Result};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};

use crate::models::audit::{AuditEvent, AuditQuery, AuditStats, AuditTimeRange};

const AUDIT_TABLE: TableDefinition<&str, &str> = TableDefinition::new("audit_events");

/// Audit storage for persisting and querying audit events.
pub struct AuditStorage {
    db: Arc<Database>,
}

impl AuditStorage {
    /// Create an audit storage with an existing database.
    pub fn new(db: Arc<Database>) -> Result<Self> {
        // Initialize schema if needed
        {
            let write_txn = db.begin_write().context("Failed to begin write transaction")?;
            write_txn
                .open_table(AUDIT_TABLE)
                .context("Failed to open audit table")?;
            write_txn.commit().context("Failed to commit schema initialization")?;
        }
        
        Ok(Self { db })
    }

    /// Create an in-memory audit storage (for testing).
    #[allow(dead_code)]
    pub fn in_memory() -> Result<Self> {
        let db = Arc::new(
            Database::builder()
                .create_with_backend(redb::backends::InMemoryBackend::new())
                .context("Failed to create in-memory database")?,
        );
        
        // Initialize schema
        {
            let write_txn = db.begin_write().context("Failed to begin write transaction")?;
            write_txn
                .open_table(AUDIT_TABLE)
                .context("Failed to open audit table")?;
            write_txn.commit().context("Failed to commit schema initialization")?;
        }
        
        Ok(Self { db })
    }

    /// Store an audit event.
    pub fn store(&self, event: &AuditEvent) -> Result<()> {
        let write_txn = self.db.begin_write().context("Failed to begin write transaction")?;
        {
            let mut table = write_txn
                .open_table(AUDIT_TABLE)
                .context("Failed to open audit table")?;
            
            let key = format!("{}:{}", event.task_id, event.id);
            let value = serde_json::to_string(event).context("Failed to serialize audit event")?;
            table.insert(key.as_str(), value.as_str()).context("Failed to insert audit event")?;
        }
        write_txn.commit().context("Failed to commit audit event")?;
        
        Ok(())
    }

    /// Query audit events with filters.
    pub fn query(&self, query: &AuditQuery) -> Result<Vec<AuditEvent>> {
        let read_txn = self.db.begin_read().context("Failed to begin read transaction")?;
        let table = read_txn
            .open_table(AUDIT_TABLE)
            .context("Failed to open audit table")?;
        
        let mut events = Vec::new();
        let offset = query.offset.unwrap_or(0);
        let limit = query.limit.unwrap_or(100);
        
        for entry in table.iter()? {
            let (_, value) = entry.context("Failed to read entry")?;
            let value_str = value.value();
            
            if let Ok(event) = serde_json::from_str::<AuditEvent>(value_str) {
                // Apply filters
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
        
        // Sort by timestamp descending
        events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        
        // Apply pagination
        let events: Vec<_> = events.into_iter().skip(offset).take(limit).collect();
        
        Ok(events)
    }

    /// Get statistics about audit events.
    pub fn stats(&self, task_id: Option<&str>) -> Result<AuditStats> {
        let read_txn = self.db.begin_read().context("Failed to begin read transaction")?;
        let table = read_txn
            .open_table(AUDIT_TABLE)
            .context("Failed to open audit table")?;
        
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
        
        for entry in table.iter()? {
            let (_, value) = entry.context("Failed to read entry")?;
            let value_str = value.value();
            
            if let Ok(event) = serde_json::from_str::<AuditEvent>(value_str) {
                // Filter by task_id if specified
                if let Some(tid) = task_id
                    && event.task_id != tid
                {
                    continue;
                }
                
                total_events += 1;
                
                match event.category {
                    crate::models::audit::AuditEventCategory::LlmCall => {
                        llm_call_count += 1;
                        if let Some(ref llm) = event.llm_call {
                            total_tokens += llm.total_tokens.unwrap_or(0) as u64;
                            total_cost_usd += llm.cost_usd.unwrap_or(0.0);
                        }
                    }
                    crate::models::audit::AuditEventCategory::ToolCall => {
                        tool_call_count += 1;
                    }
                    crate::models::audit::AuditEventCategory::ModelSwitch => {
                        model_switch_count += 1;
                    }
                    crate::models::audit::AuditEventCategory::Lifecycle => {
                        lifecycle_count += 1;
                    }
                    crate::models::audit::AuditEventCategory::Message => {
                        message_count += 1;
                    }
                }
                
                // Update time range
                earliest = Some(earliest.map_or(event.timestamp, |e| e.min(event.timestamp)));
                latest = Some(latest.map_or(event.timestamp, |l| l.max(event.timestamp)));
            }
        }
        
        let time_range = match (earliest, latest) {
            (Some(e), Some(l)) => Some(AuditTimeRange { earliest: e, latest: l }),
            _ => None,
        };
        
        Ok(AuditStats {
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
    use crate::models::audit::LlmCallAudit;

    #[test]
    fn test_audit_storage() {
        let storage = AuditStorage::in_memory().unwrap();
        
        // Create and store an event
        let event = AuditEvent::llm_call(
            "task-123",
            "agent-456",
            LlmCallAudit {
                model: "claude-3-5-sonnet".to_string(),
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
        
        // Query events
        let query = AuditQuery {
            task_id: Some("task-123".to_string()),
            ..Default::default()
        };
        
        let results = storage.query(&query).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].task_id, "task-123");
        
        // Get stats
        let stats = storage.stats(Some("task-123")).unwrap();
        assert_eq!(stats.total_events, 1);
        assert_eq!(stats.llm_call_count, 1);
    }
}
