//! Agent Task storage - byte-level API for agent task persistence.
//!
//! Provides low-level storage operations for scheduled agent tasks and their
//! execution events using the redb embedded database.

use anyhow::Result;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::sync::Arc;

const AGENT_TASK_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("agent_tasks");
const TASK_EVENT_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("task_events");
/// Index table: task_id -> event_id (for listing events by task)
const TASK_EVENT_INDEX_TABLE: TableDefinition<&str, &str> = TableDefinition::new("task_event_index");

/// Low-level agent task storage with byte-level API
#[derive(Clone)]
pub struct AgentTaskStorage {
    db: Arc<Database>,
}

impl AgentTaskStorage {
    /// Create a new AgentTaskStorage instance
    pub fn new(db: Arc<Database>) -> Result<Self> {
        // Initialize all tables
        let write_txn = db.begin_write()?;
        write_txn.open_table(AGENT_TASK_TABLE)?;
        write_txn.open_table(TASK_EVENT_TABLE)?;
        write_txn.open_table(TASK_EVENT_INDEX_TABLE)?;
        write_txn.commit()?;

        Ok(Self { db })
    }

    // ============== Agent Task Operations ==============

    /// Store raw agent task data
    pub fn put_task_raw(&self, id: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(AGENT_TASK_TABLE)?;
            table.insert(id, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get raw agent task data by ID
    pub fn get_task_raw(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(AGENT_TASK_TABLE)?;

        if let Some(value) = table.get(id)? {
            Ok(Some(value.value().to_vec()))
        } else {
            Ok(None)
        }
    }

    /// List all raw agent task data
    pub fn list_tasks_raw(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(AGENT_TASK_TABLE)?;

        let mut tasks = Vec::new();
        for item in table.iter()? {
            let (key, value) = item?;
            tasks.push((key.value().to_string(), value.value().to_vec()));
        }

        Ok(tasks)
    }

    /// Delete agent task by ID
    pub fn delete_task(&self, id: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let existed = {
            let mut table = write_txn.open_table(AGENT_TASK_TABLE)?;
            table.remove(id)?.is_some()
        };
        write_txn.commit()?;
        Ok(existed)
    }

    // ============== Task Event Operations ==============

    /// Store raw task event data with index
    pub fn put_event_raw(&self, event_id: &str, task_id: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut event_table = write_txn.open_table(TASK_EVENT_TABLE)?;
            event_table.insert(event_id, data)?;

            // Create composite index key: task_id:timestamp:event_id for ordered retrieval
            let mut index_table = write_txn.open_table(TASK_EVENT_INDEX_TABLE)?;
            let index_key = format!("{}:{}", task_id, event_id);
            index_table.insert(index_key.as_str(), event_id)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get raw task event data by ID
    pub fn get_event_raw(&self, event_id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(TASK_EVENT_TABLE)?;

        if let Some(value) = table.get(event_id)? {
            Ok(Some(value.value().to_vec()))
        } else {
            Ok(None)
        }
    }

    /// List all events for a specific task
    pub fn list_events_for_task_raw(&self, task_id: &str) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let index_table = read_txn.open_table(TASK_EVENT_INDEX_TABLE)?;
        let event_table = read_txn.open_table(TASK_EVENT_TABLE)?;

        let prefix = format!("{}:", task_id);
        let mut events = Vec::new();

        for item in index_table.iter()? {
            let (key, value) = item?;
            let key_str = key.value();

            if key_str.starts_with(&prefix) {
                let event_id = value.value();
                if let Some(event_data) = event_table.get(event_id)? {
                    events.push((event_id.to_string(), event_data.value().to_vec()));
                }
            }
        }

        Ok(events)
    }

    /// Delete a task event by ID
    pub fn delete_event(&self, event_id: &str, task_id: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let existed = {
            let mut event_table = write_txn.open_table(TASK_EVENT_TABLE)?;
            let existed = event_table.remove(event_id)?.is_some();

            // Remove from index
            let mut index_table = write_txn.open_table(TASK_EVENT_INDEX_TABLE)?;
            let index_key = format!("{}:{}", task_id, event_id);
            index_table.remove(index_key.as_str())?;

            existed
        };
        write_txn.commit()?;
        Ok(existed)
    }

    /// Delete all events for a specific task
    pub fn delete_events_for_task(&self, task_id: &str) -> Result<u32> {
        // First, collect all event IDs for this task
        let events = self.list_events_for_task_raw(task_id)?;
        let count = events.len() as u32;

        if count == 0 {
            return Ok(0);
        }

        let write_txn = self.db.begin_write()?;
        {
            let mut event_table = write_txn.open_table(TASK_EVENT_TABLE)?;
            let mut index_table = write_txn.open_table(TASK_EVENT_INDEX_TABLE)?;

            for (event_id, _) in &events {
                event_table.remove(event_id.as_str())?;
                let index_key = format!("{}:{}", task_id, event_id);
                index_table.remove(index_key.as_str())?;
            }
        }
        write_txn.commit()?;

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_storage() -> AgentTaskStorage {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        AgentTaskStorage::new(db).unwrap()
    }

    #[test]
    fn test_put_and_get_task_raw() {
        let storage = create_test_storage();

        let data = b"test task data";
        storage.put_task_raw("task-001", data).unwrap();

        let retrieved = storage.get_task_raw("task-001").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), data);
    }

    #[test]
    fn test_get_nonexistent_task() {
        let storage = create_test_storage();

        let result = storage.get_task_raw("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_list_tasks_raw() {
        let storage = create_test_storage();

        storage.put_task_raw("task-001", b"data1").unwrap();
        storage.put_task_raw("task-002", b"data2").unwrap();
        storage.put_task_raw("task-003", b"data3").unwrap();

        let tasks = storage.list_tasks_raw().unwrap();
        assert_eq!(tasks.len(), 3);
    }

    #[test]
    fn test_delete_task() {
        let storage = create_test_storage();

        storage.put_task_raw("task-001", b"data").unwrap();

        let deleted = storage.delete_task("task-001").unwrap();
        assert!(deleted);

        let retrieved = storage.get_task_raw("task-001").unwrap();
        assert!(retrieved.is_none());

        // Deleting again should return false
        let deleted_again = storage.delete_task("task-001").unwrap();
        assert!(!deleted_again);
    }

    #[test]
    fn test_put_and_get_event_raw() {
        let storage = create_test_storage();

        let data = b"test event data";
        storage
            .put_event_raw("event-001", "task-001", data)
            .unwrap();

        let retrieved = storage.get_event_raw("event-001").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), data);
    }

    #[test]
    fn test_list_events_for_task() {
        let storage = create_test_storage();

        // Add events for task-001
        storage
            .put_event_raw("event-001", "task-001", b"data1")
            .unwrap();
        storage
            .put_event_raw("event-002", "task-001", b"data2")
            .unwrap();

        // Add events for task-002
        storage
            .put_event_raw("event-003", "task-002", b"data3")
            .unwrap();

        let events_task1 = storage.list_events_for_task_raw("task-001").unwrap();
        assert_eq!(events_task1.len(), 2);

        let events_task2 = storage.list_events_for_task_raw("task-002").unwrap();
        assert_eq!(events_task2.len(), 1);

        let events_task3 = storage.list_events_for_task_raw("task-003").unwrap();
        assert_eq!(events_task3.len(), 0);
    }

    #[test]
    fn test_delete_event() {
        let storage = create_test_storage();

        storage
            .put_event_raw("event-001", "task-001", b"data")
            .unwrap();

        let deleted = storage.delete_event("event-001", "task-001").unwrap();
        assert!(deleted);

        let retrieved = storage.get_event_raw("event-001").unwrap();
        assert!(retrieved.is_none());

        // Should also be removed from the index
        let events = storage.list_events_for_task_raw("task-001").unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn test_delete_events_for_task() {
        let storage = create_test_storage();

        storage
            .put_event_raw("event-001", "task-001", b"data1")
            .unwrap();
        storage
            .put_event_raw("event-002", "task-001", b"data2")
            .unwrap();
        storage
            .put_event_raw("event-003", "task-002", b"data3")
            .unwrap();

        let count = storage.delete_events_for_task("task-001").unwrap();
        assert_eq!(count, 2);

        let events_task1 = storage.list_events_for_task_raw("task-001").unwrap();
        assert!(events_task1.is_empty());

        // Events for task-002 should still exist
        let events_task2 = storage.list_events_for_task_raw("task-002").unwrap();
        assert_eq!(events_task2.len(), 1);
    }

    #[test]
    fn test_update_task() {
        let storage = create_test_storage();

        storage.put_task_raw("task-001", b"original data").unwrap();
        storage.put_task_raw("task-001", b"updated data").unwrap();

        let retrieved = storage.get_task_raw("task-001").unwrap();
        assert_eq!(retrieved.unwrap(), b"updated data");
    }
}
