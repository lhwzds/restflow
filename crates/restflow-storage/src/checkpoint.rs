//! Checkpoint storage - byte-level API for agent checkpoint persistence.
//!
//! Provides low-level storage operations for agent checkpoints using the
//! redb embedded database.

use anyhow::Result;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::sync::Arc;

use crate::range_utils::prefix_range;

/// Primary table: checkpoint_id -> serialized AgentCheckpoint JSON
const CHECKPOINT_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("agent_checkpoints");

/// Index: execution_id:checkpoint_id -> checkpoint_id
const CHECKPOINT_EXECUTION_INDEX: TableDefinition<&str, &str> =
    TableDefinition::new("agent_checkpoint_execution_idx");

/// Index: task_id:checkpoint_id -> checkpoint_id
const CHECKPOINT_TASK_INDEX: TableDefinition<&str, &str> =
    TableDefinition::new("agent_checkpoint_task_idx");

/// Low-level checkpoint storage with byte-level API.
#[derive(Clone)]
pub struct CheckpointStorage {
    db: Arc<Database>,
}

impl CheckpointStorage {
    /// Create a new CheckpointStorage instance and initialize tables.
    pub fn new(db: Arc<Database>) -> Result<Self> {
        let write_txn = db.begin_write()?;
        write_txn.open_table(CHECKPOINT_TABLE)?;
        write_txn.open_table(CHECKPOINT_EXECUTION_INDEX)?;
        write_txn.open_table(CHECKPOINT_TASK_INDEX)?;
        write_txn.commit()?;

        Ok(Self { db })
    }

    /// Store a checkpoint with its index entries.
    pub fn save(
        &self,
        id: &str,
        execution_id: &str,
        task_id: Option<&str>,
        data: &[u8],
    ) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(CHECKPOINT_TABLE)?;
            table.insert(id, data)?;

            let mut exec_idx = write_txn.open_table(CHECKPOINT_EXECUTION_INDEX)?;
            let exec_key = format!("{}:{}", execution_id, id);
            exec_idx.insert(exec_key.as_str(), id)?;

            if let Some(tid) = task_id {
                let mut task_idx = write_txn.open_table(CHECKPOINT_TASK_INDEX)?;
                let task_key = format!("{}:{}", tid, id);
                task_idx.insert(task_key.as_str(), id)?;
            }
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Load a checkpoint by ID.
    pub fn load(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(CHECKPOINT_TABLE)?;
        Ok(table.get(id)?.map(|v| v.value().to_vec()))
    }

    /// Load the most recent checkpoint for an execution_id.
    pub fn load_by_execution_id(&self, execution_id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let exec_idx = read_txn.open_table(CHECKPOINT_EXECUTION_INDEX)?;

        let prefix = format!("{}:", execution_id);
        let (start, end) = prefix_range(&prefix);

        // Get the last entry (most recently inserted) by iterating to the end
        let mut last_cp_id: Option<String> = None;
        for entry in exec_idx.range(start.as_str()..end.as_str())? {
            let entry = entry?;
            last_cp_id = Some(entry.1.value().to_string());
        }

        if let Some(cp_id) = last_cp_id {
            let table = read_txn.open_table(CHECKPOINT_TABLE)?;
            Ok(table.get(cp_id.as_str())?.map(|v| v.value().to_vec()))
        } else {
            Ok(None)
        }
    }

    /// Load the most recent checkpoint for a task_id.
    pub fn load_by_task_id(&self, task_id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let task_idx = read_txn.open_table(CHECKPOINT_TASK_INDEX)?;

        let prefix = format!("{}:", task_id);
        let (start, end) = prefix_range(&prefix);

        let mut last_cp_id: Option<String> = None;
        for entry in task_idx.range(start.as_str()..end.as_str())? {
            let entry = entry?;
            last_cp_id = Some(entry.1.value().to_string());
        }

        if let Some(cp_id) = last_cp_id {
            let table = read_txn.open_table(CHECKPOINT_TABLE)?;
            Ok(table.get(cp_id.as_str())?.map(|v| v.value().to_vec()))
        } else {
            Ok(None)
        }
    }

    /// Delete a checkpoint and its index entries.
    pub fn delete(&self, id: &str, execution_id: &str, task_id: Option<&str>) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(CHECKPOINT_TABLE)?;
            table.remove(id)?;

            let mut exec_idx = write_txn.open_table(CHECKPOINT_EXECUTION_INDEX)?;
            let exec_key = format!("{}:{}", execution_id, id);
            exec_idx.remove(exec_key.as_str())?;

            if let Some(tid) = task_id {
                let mut task_idx = write_txn.open_table(CHECKPOINT_TASK_INDEX)?;
                let task_key = format!("{}:{}", tid, id);
                task_idx.remove(task_key.as_str())?;
            }
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Delete all checkpoints with expired_at <= now_ms.
    /// Returns the number of deleted checkpoints.
    pub fn cleanup_expired(&self, now_ms: i64) -> Result<usize> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(CHECKPOINT_TABLE)?;

        // Collect IDs of expired checkpoints
        let mut expired: Vec<(String, String, Option<String>)> = Vec::new();
        for entry in table.iter()? {
            let entry = entry?;
            let data = entry.1.value();
            // Parse just enough to check expired_at, execution_id, task_id
            if let Ok(val) = serde_json::from_slice::<serde_json::Value>(data)
                && let Some(exp) = val.get("expired_at").and_then(|v| v.as_i64())
                && exp <= now_ms
            {
                let id = entry.0.value().to_string();
                let exec_id = val
                    .get("execution_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let task_id = val
                    .get("task_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                expired.push((id, exec_id, task_id));
            }
        }
        drop(table);
        drop(read_txn);

        let count = expired.len();
        for (id, exec_id, task_id) in expired {
            self.delete(&id, &exec_id, task_id.as_deref())?;
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> Arc<Database> {
        Arc::new(Database::builder().create_with_backend(redb::backends::InMemoryBackend::new()).unwrap())
    }

    #[test]
    fn test_save_and_load_checkpoint() {
        let db = setup_db();
        let storage = CheckpointStorage::new(db).unwrap();

        let data = br#"{"id":"cp-1","execution_id":"exec-1","version":5}"#;
        storage.save("cp-1", "exec-1", Some("task-1"), data).unwrap();

        let loaded = storage.load("cp-1").unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap(), data.to_vec());
    }

    #[test]
    fn test_load_missing_checkpoint() {
        let db = setup_db();
        let storage = CheckpointStorage::new(db).unwrap();

        let loaded = storage.load("nonexistent").unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_load_by_execution_id() {
        let db = setup_db();
        let storage = CheckpointStorage::new(db).unwrap();

        storage.save("cp-1", "exec-1", None, b"data1").unwrap();
        storage.save("cp-2", "exec-1", None, b"data2").unwrap();
        storage.save("cp-3", "exec-2", None, b"data3").unwrap();

        let loaded = storage.load_by_execution_id("exec-1").unwrap();
        assert!(loaded.is_some());
        // Should get the last one for exec-1 (cp-2)
        assert_eq!(loaded.unwrap(), b"data2".to_vec());

        let loaded2 = storage.load_by_execution_id("exec-2").unwrap();
        assert_eq!(loaded2.unwrap(), b"data3".to_vec());

        let loaded3 = storage.load_by_execution_id("exec-999").unwrap();
        assert!(loaded3.is_none());
    }

    #[test]
    fn test_load_by_task_id() {
        let db = setup_db();
        let storage = CheckpointStorage::new(db).unwrap();

        storage.save("cp-1", "exec-1", Some("task-1"), b"d1").unwrap();
        storage.save("cp-2", "exec-2", Some("task-1"), b"d2").unwrap();

        let loaded = storage.load_by_task_id("task-1").unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap(), b"d2".to_vec());

        let loaded2 = storage.load_by_task_id("task-999").unwrap();
        assert!(loaded2.is_none());
    }

    #[test]
    fn test_delete_checkpoint() {
        let db = setup_db();
        let storage = CheckpointStorage::new(db).unwrap();

        storage.save("cp-1", "exec-1", Some("task-1"), b"data").unwrap();
        assert!(storage.load("cp-1").unwrap().is_some());

        storage.delete("cp-1", "exec-1", Some("task-1")).unwrap();
        assert!(storage.load("cp-1").unwrap().is_none());
        assert!(storage.load_by_execution_id("exec-1").unwrap().is_none());
        assert!(storage.load_by_task_id("task-1").unwrap().is_none());
    }

    #[test]
    fn test_cleanup_expired() {
        let db = setup_db();
        let storage = CheckpointStorage::new(db).unwrap();

        let now = chrono::Utc::now().timestamp_millis();

        // Expired checkpoint
        let expired_data = serde_json::json!({
            "id": "cp-expired",
            "execution_id": "exec-1",
            "expired_at": now - 1000
        });
        storage
            .save(
                "cp-expired",
                "exec-1",
                None,
                serde_json::to_vec(&expired_data).unwrap().as_slice(),
            )
            .unwrap();

        // Valid checkpoint
        let valid_data = serde_json::json!({
            "id": "cp-valid",
            "execution_id": "exec-2",
            "expired_at": now + 100_000
        });
        storage
            .save(
                "cp-valid",
                "exec-2",
                None,
                serde_json::to_vec(&valid_data).unwrap().as_slice(),
            )
            .unwrap();

        let cleaned = storage.cleanup_expired(now).unwrap();
        assert_eq!(cleaned, 1);
        assert!(storage.load("cp-expired").unwrap().is_none());
        assert!(storage.load("cp-valid").unwrap().is_some());
    }
}
