//! Execution history storage - tracks workflow execution history.

use anyhow::{Result, anyhow};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use ts_rs::TS;

const EXECUTION_DATA: TableDefinition<&str, &[u8]> = TableDefinition::new("execution_history:data");
const EXECUTION_INDEX: TableDefinition<&str, &str> =
    TableDefinition::new("execution_history:index");

/// Execution status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub enum ExecutionStatus {
    Running,
    Completed,
    Failed,
}

/// Execution summary stored in the database
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExecutionSummary {
    pub execution_id: String,
    pub workflow_id: String,
    pub status: ExecutionStatus,
    #[ts(type = "number")]
    pub started_at: i64,
    #[ts(type = "number | null")]
    pub completed_at: Option<i64>,
    pub total_tasks: usize,
    pub completed_tasks: usize,
    pub failed_tasks: usize,
}

/// Paginated execution history response
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExecutionHistoryPage {
    pub items: Vec<ExecutionSummary>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

/// Execution history storage
pub struct ExecutionHistoryStorage {
    db: Arc<Database>,
}

impl ExecutionHistoryStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        let write_txn = db.begin_write()?;
        write_txn.open_table(EXECUTION_DATA)?;
        write_txn.open_table(EXECUTION_INDEX)?;
        write_txn.commit()?;
        Ok(Self { db })
    }

    /// Record a new task created
    pub fn record_task_created(
        &self,
        workflow_id: &str,
        execution_id: &str,
        created_at_nano: i64,
    ) -> Result<()> {
        let txn = self.db.begin_write()?;
        {
            let mut data_table = txn.open_table(EXECUTION_DATA)?;

            let mut summary = if let Some(existing) = data_table.get(execution_id)? {
                serde_json::from_slice::<ExecutionSummary>(existing.value())?
            } else {
                ExecutionSummary {
                    execution_id: execution_id.to_string(),
                    workflow_id: workflow_id.to_string(),
                    status: ExecutionStatus::Running,
                    started_at: nanos_to_millis(created_at_nano),
                    completed_at: None,
                    total_tasks: 0,
                    completed_tasks: 0,
                    failed_tasks: 0,
                }
            };

            summary.total_tasks = summary.total_tasks.saturating_add(1);

            let serialized = serde_json::to_vec(&summary)?;
            data_table.insert(summary.execution_id.as_str(), serialized.as_slice())?;
            drop(data_table);

            let mut index_table = txn.open_table(EXECUTION_INDEX)?;
            let key = Self::index_key(
                &summary.workflow_id,
                summary.started_at,
                &summary.execution_id,
            );
            index_table.insert(key.as_str(), summary.execution_id.as_str())?;
        }
        txn.commit()?;
        Ok(())
    }

    /// Record a task completed
    pub fn record_task_completed(
        &self,
        workflow_id: &str,
        execution_id: &str,
        timestamp_ms: i64,
    ) -> Result<()> {
        let txn = self.db.begin_write()?;
        {
            let mut data_table = txn.open_table(EXECUTION_DATA)?;

            let mut summary = if let Some(existing) = data_table.get(execution_id)? {
                serde_json::from_slice::<ExecutionSummary>(existing.value())?
            } else {
                return Err(anyhow!("Execution summary not found for {execution_id}"));
            };

            debug_assert_eq!(summary.workflow_id, workflow_id);

            summary.completed_tasks = summary.completed_tasks.saturating_add(1);

            if summary.failed_tasks == 0
                && summary.completed_tasks + summary.failed_tasks == summary.total_tasks
            {
                summary.status = ExecutionStatus::Completed;
                summary.completed_at = Some(timestamp_ms);
            }

            let serialized = serde_json::to_vec(&summary)?;
            data_table.insert(summary.execution_id.as_str(), serialized.as_slice())?;
            drop(data_table);

            let mut index_table = txn.open_table(EXECUTION_INDEX)?;
            let key = Self::index_key(
                &summary.workflow_id,
                summary.started_at,
                &summary.execution_id,
            );
            index_table.insert(key.as_str(), summary.execution_id.as_str())?;
        }
        txn.commit()?;
        Ok(())
    }

    /// Record a task failed
    pub fn record_task_failed(
        &self,
        workflow_id: &str,
        execution_id: &str,
        timestamp_ms: i64,
    ) -> Result<()> {
        let txn = self.db.begin_write()?;
        {
            let mut data_table = txn.open_table(EXECUTION_DATA)?;

            let mut summary = if let Some(existing) = data_table.get(execution_id)? {
                serde_json::from_slice::<ExecutionSummary>(existing.value())?
            } else {
                return Err(anyhow!("Execution summary not found for {execution_id}"));
            };

            debug_assert_eq!(summary.workflow_id, workflow_id);

            summary.failed_tasks = summary.failed_tasks.saturating_add(1);
            summary.status = ExecutionStatus::Failed;
            summary.completed_at.get_or_insert(timestamp_ms);

            let serialized = serde_json::to_vec(&summary)?;
            data_table.insert(summary.execution_id.as_str(), serialized.as_slice())?;
            drop(data_table);

            let mut index_table = txn.open_table(EXECUTION_INDEX)?;
            let key = Self::index_key(
                &summary.workflow_id,
                summary.started_at,
                &summary.execution_id,
            );
            index_table.insert(key.as_str(), summary.execution_id.as_str())?;
        }
        txn.commit()?;
        Ok(())
    }

    /// List executions with pagination
    pub fn list_paginated(
        &self,
        workflow_id: &str,
        page: usize,
        page_size: usize,
    ) -> Result<ExecutionHistoryPage> {
        let page = if page == 0 { 1 } else { page };
        let page_size = page_size.clamp(1, 100);
        let read_txn = self.db.begin_read()?;
        let index = read_txn.open_table(EXECUTION_INDEX)?;
        let data = read_txn.open_table(EXECUTION_DATA)?;

        let prefix = format!("{workflow_id}:");

        let mut exec_ids: Vec<String> = Vec::new();
        let mut iter = index.range(prefix.as_str()..)?;
        while let Some(Ok((key, value))) = iter.next() {
            let key_str = key.value();
            if !key_str.starts_with(&prefix) {
                break;
            }

            let exec_id = value.value();
            exec_ids.push(exec_id.to_string());
        }

        let total = exec_ids.len();

        let total_pages = if total == 0 {
            0
        } else {
            ((total - 1) / page_size) + 1
        };

        let current_page = if total_pages == 0 {
            1
        } else {
            page.min(total_pages)
        };

        let start_index = (current_page - 1).saturating_mul(page_size);
        let end_index = (start_index + page_size).min(total);
        let mut items = Vec::new();
        if start_index < end_index {
            for exec_id in &exec_ids[start_index..end_index] {
                if let Some(summary_bytes) = data.get(exec_id.as_str())? {
                    let summary: ExecutionSummary =
                        serde_json::from_slice(summary_bytes.value())?;
                    items.push(summary);
                }
            }
        }

        Ok(ExecutionHistoryPage {
            items,
            total,
            page: current_page,
            page_size,
            total_pages,
        })
    }

    fn index_key(workflow_id: &str, started_at_ms: i64, execution_id: &str) -> String {
        let started_at_ms = started_at_ms.max(0) as u64;
        let reverse_ts = u64::MAX - started_at_ms;
        format!("{workflow_id}:{reverse_ts:020}:{execution_id}")
    }
}

fn nanos_to_millis(timestamp: i64) -> i64 {
    if timestamp == 0 {
        0
    } else {
        timestamp / 1_000_000
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Returns both the store and the TempDir to ensure the directory
    /// is not deleted while the store is in use.
    fn test_store() -> (ExecutionHistoryStorage, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("execution_history.redb");
        let db = Arc::new(Database::create(db_path).unwrap());
        (ExecutionHistoryStorage::new(db).unwrap(), dir)
    }

    #[test]
    fn test_create_and_list_execution_history() {
        let (store, _temp_dir) = test_store();

        store
            .record_task_created("wf1", "exec-1", 1_000_000_000)
            .unwrap();
        store
            .record_task_created("wf1", "exec-1", 1_000_000_001)
            .unwrap();
        store.record_task_completed("wf1", "exec-1", 1_000).unwrap();
        store
            .record_task_created("wf1", "exec-2", 2_000_000_000)
            .unwrap();
        store.record_task_failed("wf1", "exec-2", 2_000).unwrap();

        let page = store.list_paginated("wf1", 1, 10).unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.page, 1);
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].execution_id, "exec-2");
        assert_eq!(page.items[1].execution_id, "exec-1");
    }

    #[test]
    fn test_pagination_bounds() {
        let (store, _temp_dir) = test_store();
        for i in 0..5 {
            let exec_id = format!("exec-{i}");
            store
                .record_task_created("wf1", &exec_id, 1_000_000_000 - (i as i64) * 1_000_000)
                .unwrap();
        }

        let page = store.list_paginated("wf1", 2, 2).unwrap();
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].execution_id, "exec-2");
        assert_eq!(page.items[1].execution_id, "exec-3");

        let page = store.list_paginated("wf1", 10, 2).unwrap();
        assert_eq!(page.page, 3);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].execution_id, "exec-4");
    }
}
