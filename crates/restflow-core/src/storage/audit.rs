use crate::models::{
    AuditEntry, AuditEntryType, AuditSummary, ModelAuditSummary, ToolAuditSummary,
};
use anyhow::Result;
use chrono::{DateTime, Utc};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

const AUDIT_ENTRIES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("audit_entries_v1");
const TASK_EXECUTION_INDEX_TABLE: TableDefinition<&str, u8> =
    TableDefinition::new("audit_task_execution_index_v1");

#[derive(Clone)]
pub struct AuditStorage {
    db: Arc<Database>,
}

impl AuditStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        {
            let tx = db.begin_write()?;
            tx.open_table(AUDIT_ENTRIES_TABLE)?;
            tx.open_table(TASK_EXECUTION_INDEX_TABLE)?;
            tx.commit()?;
        }
        Ok(Self { db })
    }

    pub fn append(&self, entry: &AuditEntry) -> Result<()> {
        let tx = self.db.begin_write()?;
        {
            let mut table = tx.open_table(AUDIT_ENTRIES_TABLE)?;
            table.insert(
                entry_key(entry).as_str(),
                serde_json::to_vec(entry)?.as_slice(),
            )?;
        }
        {
            let mut index = tx.open_table(TASK_EXECUTION_INDEX_TABLE)?;
            let key = task_execution_key(&entry.task_id, &entry.execution_id);
            index.insert(key.as_str(), 1)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn list_by_execution(&self, execution_id: &str) -> Result<Vec<AuditEntry>> {
        let tx = self.db.begin_read()?;
        let table = tx.open_table(AUDIT_ENTRIES_TABLE)?;
        let mut entries = Vec::new();
        for row in table.iter()? {
            let (_, value) = row?;
            let entry: AuditEntry = serde_json::from_slice(value.value())?;
            if entry.execution_id == execution_id {
                entries.push(entry);
            }
        }
        entries.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(entries)
    }

    pub fn list_by_task(&self, task_id: &str, limit: usize) -> Result<Vec<AuditEntry>> {
        let tx = self.db.begin_read()?;
        let index = tx.open_table(TASK_EXECUTION_INDEX_TABLE)?;
        let mut execution_ids = BTreeSet::new();
        for row in index.iter()? {
            let (key, _) = row?;
            let raw = key.value();
            if let Some((task, execution)) = parse_task_execution_key(raw)
                && task == task_id
            {
                execution_ids.insert(execution.to_string());
            }
        }

        let mut all_entries = Vec::new();
        for execution_id in execution_ids {
            let mut entries = self.list_by_execution(&execution_id)?;
            all_entries.append(&mut entries);
        }
        all_entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        if limit == 0 {
            return Ok(all_entries);
        }
        Ok(all_entries.into_iter().take(limit).collect())
    }

    pub fn summarize_execution(&self, execution_id: &str) -> Result<Option<AuditSummary>> {
        let entries = self.list_by_execution(execution_id)?;
        if entries.is_empty() {
            return Ok(None);
        }

        let task_id = entries[0].task_id.clone();
        let mut total_llm_calls = 0usize;
        let mut total_tool_calls = 0usize;
        let mut total_tokens = 0u32;
        let mut total_cost_usd = 0.0f64;
        let mut total_duration_ms = 0u64;
        let mut success: Option<bool> = None;
        let mut tool_map: BTreeMap<String, (usize, usize, usize, u64)> = BTreeMap::new();
        let mut model_map: BTreeMap<String, (usize, u32, f64)> = BTreeMap::new();

        for entry in entries {
            match entry.entry_type {
                AuditEntryType::LlmCall {
                    model,
                    input_tokens,
                    output_tokens,
                    cost_usd,
                    duration_ms,
                    ..
                } => {
                    total_llm_calls += 1;
                    total_tokens = total_tokens.saturating_add(input_tokens + output_tokens);
                    total_cost_usd += cost_usd;
                    total_duration_ms = total_duration_ms.saturating_add(duration_ms);
                    let item = model_map.entry(model).or_insert((0, 0, 0.0));
                    item.0 += 1;
                    item.1 = item.1.saturating_add(input_tokens + output_tokens);
                    item.2 += cost_usd;
                }
                AuditEntryType::ToolCall {
                    tool_name,
                    success: tool_success,
                    duration_ms,
                    ..
                } => {
                    total_tool_calls += 1;
                    total_duration_ms = total_duration_ms.saturating_add(duration_ms);
                    let item = tool_map.entry(tool_name).or_insert((0, 0, 0, 0));
                    item.0 += 1;
                    if tool_success {
                        item.1 += 1;
                    } else {
                        item.2 += 1;
                    }
                    item.3 = item.3.saturating_add(duration_ms);
                }
                AuditEntryType::ExecutionComplete {
                    total_iterations: _,
                    total_tokens: completion_tokens,
                    total_cost_usd: completion_cost,
                    total_duration_ms: completion_duration,
                    success: completion_success,
                } => {
                    success = Some(completion_success);
                    total_duration_ms = total_duration_ms.max(completion_duration);
                    total_tokens = total_tokens.max(completion_tokens);
                    total_cost_usd = total_cost_usd.max(completion_cost);
                }
                AuditEntryType::ExecutionFailed {
                    total_duration_ms: failed_duration,
                    ..
                } => {
                    success = Some(false);
                    total_duration_ms = total_duration_ms.max(failed_duration);
                }
                _ => {}
            }
        }

        let tool_breakdown = tool_map
            .into_iter()
            .map(
                |(tool_name, (call_count, success_count, failure_count, total_duration_ms))| {
                    ToolAuditSummary {
                        tool_name,
                        call_count,
                        success_count,
                        failure_count,
                        total_duration_ms,
                        avg_duration_ms: if call_count > 0 {
                            total_duration_ms / call_count as u64
                        } else {
                            0
                        },
                    }
                },
            )
            .collect();

        let model_breakdown = model_map
            .into_iter()
            .map(
                |(model, (call_count, model_total_tokens, model_total_cost_usd))| {
                    ModelAuditSummary {
                        model,
                        call_count,
                        total_tokens: model_total_tokens,
                        total_cost_usd: model_total_cost_usd,
                    }
                },
            )
            .collect();

        Ok(Some(AuditSummary {
            task_id,
            execution_id: execution_id.to_string(),
            total_llm_calls,
            total_tool_calls,
            total_tokens,
            total_cost_usd,
            total_duration_ms,
            success,
            tool_breakdown,
            model_breakdown,
        }))
    }

    pub fn cleanup_before(&self, before: DateTime<Utc>) -> Result<usize> {
        let tx = self.db.begin_write()?;
        let mut removed = 0usize;
        {
            let mut table = tx.open_table(AUDIT_ENTRIES_TABLE)?;
            let mut to_delete = Vec::new();
            for row in table.iter()? {
                let (key, value) = row?;
                let entry: AuditEntry = serde_json::from_slice(value.value())?;
                if entry.timestamp < before {
                    to_delete.push(key.value().to_string());
                }
            }
            for key in to_delete {
                table.remove(key.as_str())?;
                removed += 1;
            }
        }
        tx.commit()?;
        Ok(removed)
    }
}

fn entry_key(entry: &AuditEntry) -> String {
    format!(
        "{}:{}:{}:{}",
        entry.task_id,
        entry.execution_id,
        entry.timestamp.timestamp_millis(),
        entry.id
    )
}

fn task_execution_key(task_id: &str, execution_id: &str) -> String {
    format!("{}:{}", task_id, execution_id)
}

fn parse_task_execution_key(value: &str) -> Option<(&str, &str)> {
    value.split_once(':')
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use tempfile::NamedTempFile;

    fn create_storage() -> AuditStorage {
        let file = NamedTempFile::new().expect("temp db file");
        let db = Arc::new(Database::create(file.path()).expect("create db"));
        AuditStorage::new(db).expect("create storage")
    }

    #[test]
    fn append_and_list_execution_entries() {
        let storage = create_storage();
        let execution_id = "exec-1";
        storage
            .append(&AuditEntry::new(
                "task-1",
                execution_id,
                AuditEntryType::ExecutionStart {
                    agent_id: "agent-1".to_string(),
                    model: "gpt-5".to_string(),
                    input_preview: "hello".to_string(),
                },
            ))
            .unwrap();
        storage
            .append(&AuditEntry::new(
                "task-1",
                execution_id,
                AuditEntryType::ToolCall {
                    tool_name: "bash".to_string(),
                    success: true,
                    duration_ms: 120,
                    input_size_bytes: 10,
                    output_size_bytes: 20,
                    error: None,
                    iteration: 1,
                },
            ))
            .unwrap();

        let entries = storage.list_by_execution(execution_id).unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn summarize_execution_aggregates_metrics() {
        let storage = create_storage();
        let execution_id = "exec-summary";
        storage
            .append(&AuditEntry::new(
                "task-1",
                execution_id,
                AuditEntryType::LlmCall {
                    model: "gpt-5".to_string(),
                    input_tokens: 100,
                    output_tokens: 50,
                    cost_usd: 0.01,
                    duration_ms: 200,
                    iteration: 1,
                },
            ))
            .unwrap();
        storage
            .append(&AuditEntry::new(
                "task-1",
                execution_id,
                AuditEntryType::ToolCall {
                    tool_name: "bash".to_string(),
                    success: false,
                    duration_ms: 80,
                    input_size_bytes: 20,
                    output_size_bytes: 30,
                    error: Some("boom".to_string()),
                    iteration: 1,
                },
            ))
            .unwrap();
        storage
            .append(&AuditEntry::new(
                "task-1",
                execution_id,
                AuditEntryType::ExecutionComplete {
                    total_iterations: 1,
                    total_tokens: 150,
                    total_cost_usd: 0.01,
                    total_duration_ms: 280,
                    success: true,
                },
            ))
            .unwrap();

        let summary = storage
            .summarize_execution(execution_id)
            .unwrap()
            .expect("summary");
        assert_eq!(summary.total_llm_calls, 1);
        assert_eq!(summary.total_tool_calls, 1);
        assert_eq!(summary.total_tokens, 150);
        assert_eq!(summary.success, Some(true));
        assert_eq!(summary.tool_breakdown.len(), 1);
        assert_eq!(summary.model_breakdown.len(), 1);
    }

    #[test]
    fn cleanup_before_removes_old_entries() {
        let storage = create_storage();
        let old_time = Utc
            .timestamp_millis_opt(1_700_000_000_000)
            .single()
            .expect("old timestamp");
        let mut entry = AuditEntry::new(
            "task-1",
            "exec-old",
            AuditEntryType::ExecutionFailed {
                error: "x".to_string(),
                total_duration_ms: 1,
            },
        );
        entry.timestamp = old_time;
        storage.append(&entry).unwrap();

        let removed = storage
            .cleanup_before(
                Utc.timestamp_millis_opt(1_800_000_000_000)
                    .single()
                    .expect("new timestamp"),
            )
            .unwrap();
        assert_eq!(removed, 1);
    }
}
