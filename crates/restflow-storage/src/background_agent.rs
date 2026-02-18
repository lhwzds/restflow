//! Agent Task storage - byte-level API for agent task persistence.
//!
//! Provides low-level storage operations for scheduled agent tasks and their
//! execution events using the redb embedded database.

use anyhow::Result;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::sync::Arc;

use crate::range_utils::prefix_range;

const BACKGROUND_AGENT_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("background_agents");
const BACKGROUND_AGENT_EVENT_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("background_agent_events");
/// Index table: task_id -> event_id (for listing events by task)
const BACKGROUND_AGENT_EVENT_INDEX_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("background_agent_event_index");
/// Index table: status:task_id -> task_id (for listing tasks by status)
const BACKGROUND_AGENT_STATUS_INDEX_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("background_agent_status_index");
/// Background message payload table
const BACKGROUND_MESSAGE_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("background_messages");
/// Index table: task_id:message_id -> message_id
const BACKGROUND_MESSAGE_TASK_INDEX_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("background_message_task_index");
/// Index table: status:task_id:message_id -> message_id
const BACKGROUND_MESSAGE_STATUS_INDEX_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("background_message_status_index");

/// Low-level agent task storage with byte-level API
#[derive(Clone)]
pub struct BackgroundAgentStorage {
    db: Arc<Database>,
}

impl BackgroundAgentStorage {
    /// Create a new BackgroundAgentStorage instance
    pub fn new(db: Arc<Database>) -> Result<Self> {
        // Initialize all tables
        let write_txn = db.begin_write()?;
        write_txn.open_table(BACKGROUND_AGENT_TABLE)?;
        write_txn.open_table(BACKGROUND_AGENT_EVENT_TABLE)?;
        write_txn.open_table(BACKGROUND_AGENT_EVENT_INDEX_TABLE)?;
        write_txn.open_table(BACKGROUND_AGENT_STATUS_INDEX_TABLE)?;
        write_txn.open_table(BACKGROUND_MESSAGE_TABLE)?;
        write_txn.open_table(BACKGROUND_MESSAGE_TASK_INDEX_TABLE)?;
        write_txn.open_table(BACKGROUND_MESSAGE_STATUS_INDEX_TABLE)?;
        write_txn.commit()?;

        Ok(Self { db })
    }

    // ============== Agent Task Operations ==============

    /// Store raw agent task data
    pub fn put_task_raw(&self, id: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(BACKGROUND_AGENT_TABLE)?;
            table.insert(id, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Store raw agent task data with status index
    pub fn put_task_raw_with_status(&self, id: &str, status: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(BACKGROUND_AGENT_TABLE)?;
            table.insert(id, data)?;

            let mut status_index = write_txn.open_table(BACKGROUND_AGENT_STATUS_INDEX_TABLE)?;
            let status_key = format!("{}:{}", status, id);
            status_index.insert(status_key.as_str(), id)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Update raw agent task data while keeping the status index consistent
    pub fn update_task_raw_with_status(
        &self,
        id: &str,
        old_status: &str,
        new_status: &str,
        data: &[u8],
    ) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(BACKGROUND_AGENT_TABLE)?;
            table.insert(id, data)?;

            let mut status_index = write_txn.open_table(BACKGROUND_AGENT_STATUS_INDEX_TABLE)?;
            if old_status != new_status {
                let old_key = format!("{}:{}", old_status, id);
                status_index.remove(old_key.as_str())?;
            }

            let new_key = format!("{}:{}", new_status, id);
            status_index.insert(new_key.as_str(), id)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get raw agent task data by ID
    pub fn get_task_raw(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(BACKGROUND_AGENT_TABLE)?;

        if let Some(value) = table.get(id)? {
            Ok(Some(value.value().to_vec()))
        } else {
            Ok(None)
        }
    }

    /// List all raw agent task data
    pub fn list_tasks_raw(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(BACKGROUND_AGENT_TABLE)?;

        let mut tasks = Vec::new();
        for item in table.iter()? {
            let (key, value) = item?;
            tasks.push((key.value().to_string(), value.value().to_vec()));
        }

        Ok(tasks)
    }

    /// List tasks by status using the status index
    pub fn list_tasks_by_status_indexed(&self, status: &str) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let status_index = read_txn.open_table(BACKGROUND_AGENT_STATUS_INDEX_TABLE)?;
        let task_table = read_txn.open_table(BACKGROUND_AGENT_TABLE)?;

        let prefix = format!("{}:", status);
        let (start, end) = prefix_range(&prefix);
        let mut tasks = Vec::new();

        for item in status_index.range(start.as_str()..end.as_str())? {
            let (_, value) = item?;
            let task_id = value.value();
            if let Some(data) = task_table.get(task_id)? {
                tasks.push((task_id.to_string(), data.value().to_vec()));
            }
        }

        Ok(tasks)
    }

    /// Delete agent task by ID
    pub fn delete_task(&self, id: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let existed = {
            let mut table = write_txn.open_table(BACKGROUND_AGENT_TABLE)?;
            table.remove(id)?.is_some()
        };
        write_txn.commit()?;
        Ok(existed)
    }

    /// Delete agent task by ID with status index cleanup
    pub fn delete_task_with_status(&self, id: &str, status: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let existed = {
            let mut table = write_txn.open_table(BACKGROUND_AGENT_TABLE)?;
            let existed = table.remove(id)?.is_some();

            let mut status_index = write_txn.open_table(BACKGROUND_AGENT_STATUS_INDEX_TABLE)?;
            let status_key = format!("{}:{}", status, id);
            status_index.remove(status_key.as_str())?;

            existed
        };
        write_txn.commit()?;
        Ok(existed)
    }

    /// Delete a task and all related task/message/event records atomically.
    pub fn delete_task_cascade(&self, id: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let existed = {
            let mut task_table = write_txn.open_table(BACKGROUND_AGENT_TABLE)?;
            let existed = task_table.get(id)?.is_some();

            let prefix = format!("{}:", id);
            let (start, end) = prefix_range(&prefix);

            let mut event_table = write_txn.open_table(BACKGROUND_AGENT_EVENT_TABLE)?;
            let mut event_index = write_txn.open_table(BACKGROUND_AGENT_EVENT_INDEX_TABLE)?;
            let mut event_keys = Vec::new();
            for item in event_index.range(start.as_str()..end.as_str())? {
                let (key, value) = item?;
                event_keys.push((key.value().to_string(), value.value().to_string()));
            }
            for (event_key, event_id) in event_keys {
                event_index.remove(event_key.as_str())?;
                event_table.remove(event_id.as_str())?;
            }

            let mut message_table = write_txn.open_table(BACKGROUND_MESSAGE_TABLE)?;
            let mut message_task_index =
                write_txn.open_table(BACKGROUND_MESSAGE_TASK_INDEX_TABLE)?;
            let mut message_keys = Vec::new();
            for item in message_task_index.range(start.as_str()..end.as_str())? {
                let (key, value) = item?;
                message_keys.push((key.value().to_string(), value.value().to_string()));
            }
            for (message_key, message_id) in message_keys {
                message_task_index.remove(message_key.as_str())?;
                message_table.remove(message_id.as_str())?;
            }

            let mut message_status_index =
                write_txn.open_table(BACKGROUND_MESSAGE_STATUS_INDEX_TABLE)?;
            let task_segment = format!("{}:", id);
            let mut message_status_keys = Vec::new();
            for item in message_status_index.iter()? {
                let (key, _) = item?;
                let key_value = key.value();
                if let Some((_, suffix)) = key_value.split_once(':')
                    && suffix.starts_with(task_segment.as_str())
                {
                    message_status_keys.push(key_value.to_string());
                }
            }
            for status_key in message_status_keys {
                message_status_index.remove(status_key.as_str())?;
            }

            let mut task_status_index =
                write_txn.open_table(BACKGROUND_AGENT_STATUS_INDEX_TABLE)?;
            let task_suffix = format!(":{}", id);
            let mut task_status_keys = Vec::new();
            for item in task_status_index.iter()? {
                let (key, _) = item?;
                let key_value = key.value();
                if key_value.ends_with(task_suffix.as_str()) {
                    task_status_keys.push(key_value.to_string());
                }
            }
            for status_key in task_status_keys {
                task_status_index.remove(status_key.as_str())?;
            }

            if existed {
                task_table.remove(id)?;
            }

            existed
        };
        write_txn.commit()?;
        Ok(existed)
    }

    // ============== Background Message Operations ==============

    /// Store raw background message data with task/status indices.
    pub fn put_background_message_raw_with_status(
        &self,
        message_id: &str,
        task_id: &str,
        status: &str,
        data: &[u8],
    ) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut message_table = write_txn.open_table(BACKGROUND_MESSAGE_TABLE)?;
            message_table.insert(message_id, data)?;

            let mut task_index = write_txn.open_table(BACKGROUND_MESSAGE_TASK_INDEX_TABLE)?;
            let task_key = format!("{}:{}", task_id, message_id);
            task_index.insert(task_key.as_str(), message_id)?;

            let mut status_index = write_txn.open_table(BACKGROUND_MESSAGE_STATUS_INDEX_TABLE)?;
            let status_key = format!("{}:{}:{}", status, task_id, message_id);
            status_index.insert(status_key.as_str(), message_id)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Update raw background message data and keep status index consistent.
    pub fn update_background_message_raw_with_status(
        &self,
        message_id: &str,
        task_id: &str,
        old_status: &str,
        new_status: &str,
        data: &[u8],
    ) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut message_table = write_txn.open_table(BACKGROUND_MESSAGE_TABLE)?;
            message_table.insert(message_id, data)?;

            let mut status_index = write_txn.open_table(BACKGROUND_MESSAGE_STATUS_INDEX_TABLE)?;
            if old_status != new_status {
                let old_key = format!("{}:{}:{}", old_status, task_id, message_id);
                status_index.remove(old_key.as_str())?;
            }

            let new_key = format!("{}:{}:{}", new_status, task_id, message_id);
            status_index.insert(new_key.as_str(), message_id)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get raw background message data by ID.
    pub fn get_background_message_raw(&self, message_id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(BACKGROUND_MESSAGE_TABLE)?;

        if let Some(value) = table.get(message_id)? {
            Ok(Some(value.value().to_vec()))
        } else {
            Ok(None)
        }
    }

    /// List raw background messages for a task.
    pub fn list_background_messages_for_task_raw(
        &self,
        task_id: &str,
    ) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let task_index = read_txn.open_table(BACKGROUND_MESSAGE_TASK_INDEX_TABLE)?;
        let message_table = read_txn.open_table(BACKGROUND_MESSAGE_TABLE)?;

        let prefix = format!("{}:", task_id);
        let (start, end) = prefix_range(&prefix);
        let mut messages = Vec::new();

        for item in task_index.range(start.as_str()..end.as_str())? {
            let (_, value) = item?;
            let message_id = value.value();
            if let Some(data) = message_table.get(message_id)? {
                messages.push((message_id.to_string(), data.value().to_vec()));
            }
        }

        Ok(messages)
    }

    /// List raw background messages for a task by status.
    pub fn list_background_messages_by_status_for_task_raw(
        &self,
        task_id: &str,
        status: &str,
    ) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let status_index = read_txn.open_table(BACKGROUND_MESSAGE_STATUS_INDEX_TABLE)?;
        let message_table = read_txn.open_table(BACKGROUND_MESSAGE_TABLE)?;

        let prefix = format!("{}:{}:", status, task_id);
        let (start, end) = prefix_range(&prefix);
        let mut messages = Vec::new();

        for item in status_index.range(start.as_str()..end.as_str())? {
            let (_, value) = item?;
            let message_id = value.value();
            if let Some(data) = message_table.get(message_id)? {
                messages.push((message_id.to_string(), data.value().to_vec()));
            }
        }

        Ok(messages)
    }

    /// Delete one background message and related indices.
    pub fn delete_background_message(
        &self,
        message_id: &str,
        task_id: &str,
        status: &str,
    ) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let existed = {
            let mut message_table = write_txn.open_table(BACKGROUND_MESSAGE_TABLE)?;
            let existed = message_table.remove(message_id)?.is_some();

            let mut task_index = write_txn.open_table(BACKGROUND_MESSAGE_TASK_INDEX_TABLE)?;
            let task_key = format!("{}:{}", task_id, message_id);
            task_index.remove(task_key.as_str())?;

            let mut status_index = write_txn.open_table(BACKGROUND_MESSAGE_STATUS_INDEX_TABLE)?;
            let status_key = format!("{}:{}:{}", status, task_id, message_id);
            status_index.remove(status_key.as_str())?;

            existed
        };
        write_txn.commit()?;
        Ok(existed)
    }

    /// Delete all background messages for a task.
    pub fn delete_background_messages_for_task(&self, task_id: &str) -> Result<u32> {
        let messages = self.list_background_messages_for_task_raw(task_id)?;
        let count = messages.len() as u32;

        if count == 0 {
            return Ok(0);
        }

        let write_txn = self.db.begin_write()?;
        {
            let mut message_table = write_txn.open_table(BACKGROUND_MESSAGE_TABLE)?;
            let mut task_index = write_txn.open_table(BACKGROUND_MESSAGE_TASK_INDEX_TABLE)?;
            let mut status_index = write_txn.open_table(BACKGROUND_MESSAGE_STATUS_INDEX_TABLE)?;

            for (message_id, data) in &messages {
                message_table.remove(message_id.as_str())?;

                let task_key = format!("{}:{}", task_id, message_id);
                task_index.remove(task_key.as_str())?;

                if let Ok(value) = serde_json::from_slice::<serde_json::Value>(data)
                    && let Some(status) = value.get("status").and_then(|s| s.as_str())
                {
                    let status_key = format!("{}:{}:{}", status, task_id, message_id);
                    status_index.remove(status_key.as_str())?;
                }
            }
        }
        write_txn.commit()?;
        Ok(count)
    }

    // ============== Task Event Operations ==============

    /// Store raw task event data with index
    pub fn put_event_raw(&self, event_id: &str, task_id: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut event_table = write_txn.open_table(BACKGROUND_AGENT_EVENT_TABLE)?;
            event_table.insert(event_id, data)?;

            // Create composite index key: task_id:timestamp:event_id for ordered retrieval
            let mut index_table = write_txn.open_table(BACKGROUND_AGENT_EVENT_INDEX_TABLE)?;
            let index_key = format!("{}:{}", task_id, event_id);
            index_table.insert(index_key.as_str(), event_id)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get raw task event data by ID
    pub fn get_event_raw(&self, event_id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(BACKGROUND_AGENT_EVENT_TABLE)?;

        if let Some(value) = table.get(event_id)? {
            Ok(Some(value.value().to_vec()))
        } else {
            Ok(None)
        }
    }

    /// List all events for a specific task
    pub fn list_events_for_task_raw(&self, task_id: &str) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let index_table = read_txn.open_table(BACKGROUND_AGENT_EVENT_INDEX_TABLE)?;
        let event_table = read_txn.open_table(BACKGROUND_AGENT_EVENT_TABLE)?;

        let prefix = format!("{}:", task_id);
        let (start, end) = prefix_range(&prefix);
        let mut events = Vec::new();

        for item in index_table.range(start.as_str()..end.as_str())? {
            let (_, value) = item?;
            let event_id = value.value();
            if let Some(event_data) = event_table.get(event_id)? {
                events.push((event_id.to_string(), event_data.value().to_vec()));
            }
        }

        Ok(events)
    }

    /// Delete a task event by ID
    pub fn delete_event(&self, event_id: &str, task_id: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let existed = {
            let mut event_table = write_txn.open_table(BACKGROUND_AGENT_EVENT_TABLE)?;
            let existed = event_table.remove(event_id)?.is_some();

            // Remove from index
            let mut index_table = write_txn.open_table(BACKGROUND_AGENT_EVENT_INDEX_TABLE)?;
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
            let mut event_table = write_txn.open_table(BACKGROUND_AGENT_EVENT_TABLE)?;
            let mut index_table = write_txn.open_table(BACKGROUND_AGENT_EVENT_INDEX_TABLE)?;

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

    fn create_test_storage() -> BackgroundAgentStorage {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        BackgroundAgentStorage::new(db).unwrap()
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
    fn test_list_tasks_by_status_indexed() {
        let storage = create_test_storage();

        storage
            .put_task_raw_with_status("task-001", "active", b"data1")
            .unwrap();
        storage
            .put_task_raw_with_status("task-002", "paused", b"data2")
            .unwrap();
        storage
            .put_task_raw_with_status("task-003", "active", b"data3")
            .unwrap();

        let active_tasks = storage.list_tasks_by_status_indexed("active").unwrap();
        let paused_tasks = storage.list_tasks_by_status_indexed("paused").unwrap();

        assert_eq!(active_tasks.len(), 2);
        assert_eq!(paused_tasks.len(), 1);
    }

    #[test]
    fn test_update_task_raw_with_status() {
        let storage = create_test_storage();

        storage
            .put_task_raw_with_status("task-001", "active", b"data1")
            .unwrap();
        storage
            .update_task_raw_with_status("task-001", "active", "paused", b"data2")
            .unwrap();

        let active_tasks = storage.list_tasks_by_status_indexed("active").unwrap();
        let paused_tasks = storage.list_tasks_by_status_indexed("paused").unwrap();

        assert!(active_tasks.is_empty());
        assert_eq!(paused_tasks.len(), 1);
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
    fn test_delete_task_with_status() {
        let storage = create_test_storage();

        storage
            .put_task_raw_with_status("task-001", "active", b"data")
            .unwrap();

        let deleted = storage
            .delete_task_with_status("task-001", "active")
            .unwrap();
        assert!(deleted);

        let retrieved = storage.get_task_raw("task-001").unwrap();
        assert!(retrieved.is_none());

        let active_tasks = storage.list_tasks_by_status_indexed("active").unwrap();
        assert!(active_tasks.is_empty());
    }

    #[test]
    fn test_put_and_get_background_message_raw() {
        let storage = create_test_storage();
        let data = br#"{"id":"msg-1","status":"queued"}"#;

        storage
            .put_background_message_raw_with_status("msg-1", "task-1", "queued", data)
            .unwrap();

        let raw = storage.get_background_message_raw("msg-1").unwrap();
        assert!(raw.is_some());
        assert_eq!(raw.unwrap(), data);
    }

    #[test]
    fn test_list_background_messages_for_task_raw() {
        let storage = create_test_storage();
        storage
            .put_background_message_raw_with_status(
                "msg-1",
                "task-1",
                "queued",
                br#"{"id":"msg-1","status":"queued"}"#,
            )
            .unwrap();
        storage
            .put_background_message_raw_with_status(
                "msg-2",
                "task-1",
                "delivered",
                br#"{"id":"msg-2","status":"delivered"}"#,
            )
            .unwrap();
        storage
            .put_background_message_raw_with_status(
                "msg-3",
                "task-2",
                "queued",
                br#"{"id":"msg-3","status":"queued"}"#,
            )
            .unwrap();

        let task1 = storage
            .list_background_messages_for_task_raw("task-1")
            .unwrap();
        let queued_task1 = storage
            .list_background_messages_by_status_for_task_raw("task-1", "queued")
            .unwrap();

        assert_eq!(task1.len(), 2);
        assert_eq!(queued_task1.len(), 1);
    }

    #[test]
    fn test_update_background_message_raw_with_status() {
        let storage = create_test_storage();
        storage
            .put_background_message_raw_with_status(
                "msg-1",
                "task-1",
                "queued",
                br#"{"id":"msg-1","status":"queued"}"#,
            )
            .unwrap();
        storage
            .update_background_message_raw_with_status(
                "msg-1",
                "task-1",
                "queued",
                "delivered",
                br#"{"id":"msg-1","status":"delivered"}"#,
            )
            .unwrap();

        let queued = storage
            .list_background_messages_by_status_for_task_raw("task-1", "queued")
            .unwrap();
        let delivered = storage
            .list_background_messages_by_status_for_task_raw("task-1", "delivered")
            .unwrap();
        assert!(queued.is_empty());
        assert_eq!(delivered.len(), 1);
    }

    #[test]
    fn test_delete_background_messages_for_task() {
        let storage = create_test_storage();
        storage
            .put_background_message_raw_with_status(
                "msg-1",
                "task-1",
                "queued",
                br#"{"id":"msg-1","status":"queued"}"#,
            )
            .unwrap();
        storage
            .put_background_message_raw_with_status(
                "msg-2",
                "task-1",
                "delivered",
                br#"{"id":"msg-2","status":"delivered"}"#,
            )
            .unwrap();
        storage
            .put_background_message_raw_with_status(
                "msg-3",
                "task-2",
                "queued",
                br#"{"id":"msg-3","status":"queued"}"#,
            )
            .unwrap();

        let deleted = storage
            .delete_background_messages_for_task("task-1")
            .unwrap();
        assert_eq!(deleted, 2);

        let remaining_task1 = storage
            .list_background_messages_for_task_raw("task-1")
            .unwrap();
        let remaining_task2 = storage
            .list_background_messages_for_task_raw("task-2")
            .unwrap();
        assert!(remaining_task1.is_empty());
        assert_eq!(remaining_task2.len(), 1);
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

    #[test]
    fn test_delete_task_cascade_removes_related_records_atomically() {
        let storage = create_test_storage();

        storage
            .put_task_raw_with_status("task-1", "active", br#"{"id":"task-1"}"#)
            .unwrap();
        storage
            .put_task_raw_with_status("task-2", "active", br#"{"id":"task-2"}"#)
            .unwrap();

        storage
            .put_event_raw("event-1", "task-1", b"event-1")
            .unwrap();
        storage
            .put_event_raw("event-2", "task-1", b"event-2")
            .unwrap();
        storage
            .put_event_raw("event-3", "task-2", b"event-3")
            .unwrap();

        storage
            .put_background_message_raw_with_status(
                "msg-1",
                "task-1",
                "queued",
                br#"{"id":"msg-1","status":"queued"}"#,
            )
            .unwrap();
        storage
            .put_background_message_raw_with_status(
                "msg-2",
                "task-1",
                "delivered",
                br#"{"id":"msg-2","status":"delivered"}"#,
            )
            .unwrap();
        storage
            .put_background_message_raw_with_status(
                "msg-3",
                "task-2",
                "queued",
                br#"{"id":"msg-3","status":"queued"}"#,
            )
            .unwrap();

        let deleted = storage.delete_task_cascade("task-1").unwrap();
        assert!(deleted);

        assert!(storage.get_task_raw("task-1").unwrap().is_none());
        assert_eq!(storage.list_events_for_task_raw("task-1").unwrap().len(), 0);
        assert_eq!(
            storage
                .list_background_messages_for_task_raw("task-1")
                .unwrap()
                .len(),
            0
        );

        assert!(storage.get_task_raw("task-2").unwrap().is_some());
        assert_eq!(storage.list_events_for_task_raw("task-2").unwrap().len(), 1);
        assert_eq!(
            storage
                .list_background_messages_for_task_raw("task-2")
                .unwrap()
                .len(),
            1
        );
    }
}
