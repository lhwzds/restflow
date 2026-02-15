//! Deliverable storage - byte-level API for typed agent outputs.

use anyhow::Result;
use redb::{Database, ReadableDatabase, TableDefinition};
use std::sync::Arc;

use crate::range_utils::prefix_range;

const DELIVERABLE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("deliverables");
/// Index table: task_id:deliverable_id -> deliverable_id
const DELIVERABLE_TASK_INDEX_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("deliverable_task_index");
/// Index table: execution_id:deliverable_id -> deliverable_id
const DELIVERABLE_EXECUTION_INDEX_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("deliverable_execution_index");

/// Low-level deliverable storage with byte-level API.
#[derive(Clone)]
pub struct DeliverableStorage {
    db: Arc<Database>,
}

impl DeliverableStorage {
    /// Create a new DeliverableStorage instance.
    pub fn new(db: Arc<Database>) -> Result<Self> {
        let write_txn = db.begin_write()?;
        write_txn.open_table(DELIVERABLE_TABLE)?;
        write_txn.open_table(DELIVERABLE_TASK_INDEX_TABLE)?;
        write_txn.open_table(DELIVERABLE_EXECUTION_INDEX_TABLE)?;
        write_txn.commit()?;

        Ok(Self { db })
    }

    /// Store raw deliverable data with task/execution indexes.
    pub fn put_raw_with_indexes(
        &self,
        id: &str,
        task_id: &str,
        execution_id: &str,
        data: &[u8],
    ) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(DELIVERABLE_TABLE)?;
            table.insert(id, data)?;

            let mut task_index = write_txn.open_table(DELIVERABLE_TASK_INDEX_TABLE)?;
            let task_key = format!("{}:{}", task_id, id);
            task_index.insert(task_key.as_str(), id)?;

            let mut execution_index = write_txn.open_table(DELIVERABLE_EXECUTION_INDEX_TABLE)?;
            let execution_key = format!("{}:{}", execution_id, id);
            execution_index.insert(execution_key.as_str(), id)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get raw deliverable by ID.
    pub fn get_raw(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(DELIVERABLE_TABLE)?;

        if let Some(value) = table.get(id)? {
            Ok(Some(value.value().to_vec()))
        } else {
            Ok(None)
        }
    }

    /// List raw deliverables for task.
    pub fn list_by_task_raw(&self, task_id: &str) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let task_index = read_txn.open_table(DELIVERABLE_TASK_INDEX_TABLE)?;
        let deliverable_table = read_txn.open_table(DELIVERABLE_TABLE)?;

        let prefix = format!("{}:", task_id);
        let (start, end) = prefix_range(&prefix);
        let mut deliverables = Vec::new();

        for item in task_index.range(start.as_str()..end.as_str())? {
            let (_, value) = item?;
            let deliverable_id = value.value();
            if let Some(data) = deliverable_table.get(deliverable_id)? {
                deliverables.push((deliverable_id.to_string(), data.value().to_vec()));
            }
        }

        Ok(deliverables)
    }

    /// List raw deliverables for execution.
    pub fn list_by_execution_raw(&self, execution_id: &str) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let execution_index = read_txn.open_table(DELIVERABLE_EXECUTION_INDEX_TABLE)?;
        let deliverable_table = read_txn.open_table(DELIVERABLE_TABLE)?;

        let prefix = format!("{}:", execution_id);
        let (start, end) = prefix_range(&prefix);
        let mut deliverables = Vec::new();

        for item in execution_index.range(start.as_str()..end.as_str())? {
            let (_, value) = item?;
            let deliverable_id = value.value();
            if let Some(data) = deliverable_table.get(deliverable_id)? {
                deliverables.push((deliverable_id.to_string(), data.value().to_vec()));
            }
        }

        Ok(deliverables)
    }

    /// Delete deliverable by ID with index cleanup.
    pub fn delete(&self, id: &str, task_id: &str, execution_id: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let existed = {
            let mut table = write_txn.open_table(DELIVERABLE_TABLE)?;
            let existed = table.remove(id)?.is_some();

            let mut task_index = write_txn.open_table(DELIVERABLE_TASK_INDEX_TABLE)?;
            let task_key = format!("{}:{}", task_id, id);
            task_index.remove(task_key.as_str())?;

            let mut execution_index = write_txn.open_table(DELIVERABLE_EXECUTION_INDEX_TABLE)?;
            let execution_key = format!("{}:{}", execution_id, id);
            execution_index.remove(execution_key.as_str())?;

            existed
        };
        write_txn.commit()?;
        Ok(existed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_deliverable_storage_crud_and_indexes() {
        let dir = tempdir().expect("temp dir should be created");
        let db_path = dir.path().join("deliverable-storage.db");
        let db = Arc::new(Database::create(db_path).expect("db should be created"));
        let storage = DeliverableStorage::new(db).expect("storage should be created");

        storage
            .put_raw_with_indexes("d1", "t1", "e1", br#"{"title":"A"}"#)
            .expect("first put should succeed");
        storage
            .put_raw_with_indexes("d2", "t1", "e2", br#"{"title":"B"}"#)
            .expect("second put should succeed");

        let d1 = storage.get_raw("d1").expect("get should succeed");
        assert!(d1.is_some());

        let by_task = storage
            .list_by_task_raw("t1")
            .expect("list by task should succeed");
        assert_eq!(by_task.len(), 2);

        let by_execution = storage
            .list_by_execution_raw("e1")
            .expect("list by execution should succeed");
        assert_eq!(by_execution.len(), 1);

        let deleted = storage
            .delete("d1", "t1", "e1")
            .expect("delete should succeed");
        assert!(deleted);

        let by_execution_after = storage
            .list_by_execution_raw("e1")
            .expect("list by execution should succeed");
        assert!(by_execution_after.is_empty());
    }
}
