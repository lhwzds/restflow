//! Typed execution history storage wrapper.
//!
//! Re-exports types from crate::models and provides storage access.

use anyhow::Result;
use redb::Database;
use std::sync::Arc;

// Re-export types from models (which have the from_tasks method)
pub use crate::models::{ExecutionHistoryPage, ExecutionStatus, ExecutionSummary};

/// Typed execution history storage wrapper.
///
/// This is a thin wrapper that re-exports the restflow-storage implementation
/// since ExecutionSummary and related types are self-contained.
pub struct ExecutionHistoryStorage {
    inner: restflow_storage::ExecutionHistoryStorage,
}

impl ExecutionHistoryStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Ok(Self {
            inner: restflow_storage::ExecutionHistoryStorage::new(db)?,
        })
    }

    /// Record a new task created
    pub fn record_task_created(
        &self,
        workflow_id: &str,
        execution_id: &str,
        created_at_nano: i64,
    ) -> Result<()> {
        self.inner
            .record_task_created(workflow_id, execution_id, created_at_nano)
    }

    /// Record a task completed
    pub fn record_task_completed(
        &self,
        workflow_id: &str,
        execution_id: &str,
        timestamp_ms: i64,
    ) -> Result<()> {
        self.inner
            .record_task_completed(workflow_id, execution_id, timestamp_ms)
    }

    /// Record a task failed
    pub fn record_task_failed(
        &self,
        workflow_id: &str,
        execution_id: &str,
        timestamp_ms: i64,
    ) -> Result<()> {
        self.inner
            .record_task_failed(workflow_id, execution_id, timestamp_ms)
    }

    /// List executions with pagination
    pub fn list_paginated(
        &self,
        workflow_id: &str,
        page: usize,
        page_size: usize,
    ) -> Result<ExecutionHistoryPage> {
        let inner_page = self.inner.list_paginated(workflow_id, page, page_size)?;

        // Convert from restflow_storage types to models types
        let items: Vec<ExecutionSummary> = inner_page
            .items
            .into_iter()
            .map(|s| ExecutionSummary {
                execution_id: s.execution_id,
                workflow_id: s.workflow_id,
                status: match s.status {
                    restflow_storage::ExecutionStatus::Running => ExecutionStatus::Running,
                    restflow_storage::ExecutionStatus::Completed => ExecutionStatus::Completed,
                    restflow_storage::ExecutionStatus::Failed => ExecutionStatus::Failed,
                },
                started_at: s.started_at,
                completed_at: s.completed_at,
                total_tasks: s.total_tasks,
                completed_tasks: s.completed_tasks,
                failed_tasks: s.failed_tasks,
            })
            .collect();

        Ok(ExecutionHistoryPage {
            items,
            total: inner_page.total,
            page: inner_page.page,
            page_size: inner_page.page_size,
            total_pages: inner_page.total_pages,
        })
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
}
