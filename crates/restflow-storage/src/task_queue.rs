//! Task queue storage - three-table priority queue design.
//!
//! Uses separate tables for pending/processing/completed for O(1) pop performance.
//! Pending uses composite key "{priority:020}:{task_id}" for uniqueness and correct ordering.

use anyhow::{anyhow, Result};
use redb::{Database, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::Notify;

const PENDING: TableDefinition<&str, &[u8]> = TableDefinition::new("pending");
const PROCESSING: TableDefinition<&str, &[u8]> = TableDefinition::new("processing");
const COMPLETED: TableDefinition<&str, &[u8]> = TableDefinition::new("completed");

/// Pure storage layer for task queue - only handles data persistence
#[derive(Clone)]
pub struct TaskQueue {
    db: Arc<Database>,
    notify: Arc<Notify>,
    /// Counter to track pending tasks, used for reliable notification
    pending_count: Arc<AtomicUsize>,
}

impl TaskQueue {
    /// Create a new task queue instance
    pub fn new(db: Arc<Database>) -> Result<Self> {
        let write_txn = db.begin_write()?;
        write_txn.open_table(PENDING)?;
        write_txn.open_table(PROCESSING)?;
        write_txn.open_table(COMPLETED)?;
        write_txn.commit()?;

        // Count existing pending tasks for reliable notification
        let pending_count = {
            let read_txn = db.begin_read()?;
            let pending = read_txn.open_table(PENDING)?;
            pending.len()? as usize
        };

        Ok(Self {
            db,
            notify: Arc::new(Notify::new()),
            pending_count: Arc::new(AtomicUsize::new(pending_count)),
        })
    }

    /// Insert a task into the pending queue with composite key for uniqueness
    pub fn insert_pending(&self, priority: u64, task_id: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(PENDING)?;
            // Composite key: "{priority:020}:{task_id}" ensures uniqueness and correct ordering
            let key = format!("{:020}:{}", priority, task_id);
            table.insert(key.as_str(), data)?;
        }
        write_txn.commit()?;
        self.pending_count.fetch_add(1, Ordering::SeqCst);
        // Use notify_waiters() to ensure notification is not lost
        self.notify.notify_waiters();
        Ok(())
    }

    /// Atomically pop the first pending task and move it to processing
    /// Accepts a fallible callback to update task data within the same transaction
    ///
    /// The callback receives the raw data and must return `Ok(updated_data)` or an error.
    /// If the callback returns an error, the transaction is aborted and the error is propagated.
    pub fn atomic_pop_pending<F>(&self, on_data: F) -> Result<Option<Vec<u8>>>
    where
        F: FnOnce(&[u8]) -> Result<Vec<u8>>,
    {
        let write_txn = self.db.begin_write()?;

        let result = {
            let mut pending = write_txn.open_table(PENDING)?;

            // Extract first entry into owned values
            let first_entry = if let Some(first) = pending.first()? {
                let key_str = first.0.value().to_string();
                let data = first.1.value().to_vec();
                // Extract task_id from composite key with proper error handling
                let task_id = key_str
                    .split(':')
                    .nth(1)
                    .ok_or_else(|| anyhow!("Invalid composite key format: {}", key_str))?
                    .to_string();
                Some((key_str, task_id, data))
            } else {
                None
            };

            if let Some((key, task_id, data)) = first_entry {
                // Remove from pending
                pending.remove(key.as_str())?;

                // Update data via callback (fallible)
                let updated_data = match on_data(&data) {
                    Ok(data) => data,
                    Err(e) => {
                        // Callback failed, abort transaction
                        drop(pending);
                        write_txn.abort()?;
                        return Err(e);
                    }
                };

                // Write to processing table
                let mut processing = write_txn.open_table(PROCESSING)?;
                processing.insert(task_id.as_str(), updated_data.as_slice())?;

                Some(updated_data)
            } else {
                None
            }
        };

        if result.is_some() {
            write_txn.commit()?;
            self.pending_count.fetch_sub(1, Ordering::SeqCst);
        } else {
            write_txn.abort()?;
        }

        Ok(result)
    }

    /// Get the first pending task without removing it
    pub fn get_first_pending(&self) -> Result<Option<(u64, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let pending = read_txn.open_table(PENDING)?;

        if let Some((key, value)) = pending.first()? {
            // Extract priority from composite key
            let key_str = key.value();
            let priority = key_str
                .split(':')
                .next()
                .and_then(|s| s.parse::<u64>().ok())
                .ok_or_else(|| anyhow::anyhow!("Invalid pending key format: {}", key_str))?;

            Ok(Some((priority, value.value().to_vec())))
        } else {
            Ok(None)
        }
    }

    /// Move a task from pending to processing (legacy method)
    pub fn move_to_processing(&self, priority: u64, task_id: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;

        {
            let mut pending = write_txn.open_table(PENDING)?;
            let key = format!("{:020}:{}", priority, task_id);
            pending.remove(key.as_str())?;
        }

        {
            let mut processing = write_txn.open_table(PROCESSING)?;
            processing.insert(task_id, data)?;
        }

        write_txn.commit()?;
        Ok(())
    }

    /// Move a task from processing to completed
    pub fn move_to_completed(&self, task_id: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;

        {
            let mut processing = write_txn.open_table(PROCESSING)?;
            processing.remove(task_id)?;
        }

        {
            let mut completed = write_txn.open_table(COMPLETED)?;
            completed.insert(task_id, data)?;
        }

        write_txn.commit()?;
        Ok(())
    }

    /// Get task from processing table
    pub fn get_from_processing(&self, task_id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let processing = read_txn.open_table(PROCESSING)?;

        if let Some(data) = processing.get(task_id)? {
            Ok(Some(data.value().to_vec()))
        } else {
            Ok(None)
        }
    }

    /// Remove task from processing table
    pub fn remove_from_processing(&self, task_id: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut processing = write_txn.open_table(PROCESSING)?;
            processing.remove(task_id)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get task from any table
    pub fn get_from_any_table(&self, task_id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;

        // Check processing
        let processing = read_txn.open_table(PROCESSING)?;
        if let Some(data) = processing.get(task_id)? {
            return Ok(Some(data.value().to_vec()));
        }

        // Check completed
        let completed = read_txn.open_table(COMPLETED)?;
        if let Some(data) = completed.get(task_id)? {
            return Ok(Some(data.value().to_vec()));
        }

        // Check pending (requires iteration)
        let pending = read_txn.open_table(PENDING)?;
        for entry in pending.iter()? {
            let (key, value) = entry?;
            let key_str = key.value();
            // Extract task_id from composite key
            if let Some(id) = key_str.split(':').nth(1)
                && id == task_id
            {
                return Ok(Some(value.value().to_vec()));
            }
        }

        Ok(None)
    }

    /// Get all tasks from pending table
    pub fn get_all_pending(&self) -> Result<Vec<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let pending = read_txn.open_table(PENDING)?;
        let mut tasks = Vec::new();

        for entry in pending.iter()? {
            let (_, value) = entry?;
            tasks.push(value.value().to_vec());
        }

        Ok(tasks)
    }

    /// Get all tasks from processing table
    pub fn get_all_processing(&self) -> Result<Vec<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let processing = read_txn.open_table(PROCESSING)?;
        let mut tasks = Vec::new();

        for entry in processing.iter()? {
            let (_, value) = entry?;
            tasks.push(value.value().to_vec());
        }

        Ok(tasks)
    }

    /// Get all tasks from completed table
    pub fn get_all_completed(&self) -> Result<Vec<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let completed = read_txn.open_table(COMPLETED)?;
        let mut tasks = Vec::new();

        for entry in completed.iter()? {
            let (_, value) = entry?;
            tasks.push(value.value().to_vec());
        }

        Ok(tasks)
    }

    /// Wait for a task to be available
    ///
    /// This method checks the pending count first before waiting to avoid
    /// missing notifications that occurred before the wait started.
    pub async fn wait_for_task(&self) {
        // If there are already pending tasks, return immediately
        if self.pending_count.load(Ordering::SeqCst) > 0 {
            return;
        }
        self.notify.notified().await;
    }

    /// Check if there are pending tasks
    pub fn has_pending_tasks(&self) -> bool {
        self.pending_count.load(Ordering::SeqCst) > 0
    }

    /// Clear all tasks from all queues
    pub fn clear_all(&self) -> Result<(usize, usize, usize)> {
        let (pending_keys, processing_keys, completed_keys) = {
            let read_txn = self.db.begin_read()?;

            let pending = read_txn.open_table(PENDING)?;
            let pending_keys: Vec<String> = pending
                .iter()?
                .filter_map(|e| e.ok().map(|(k, _)| k.value().to_string()))
                .collect();

            let processing = read_txn.open_table(PROCESSING)?;
            let processing_keys: Vec<String> = processing
                .iter()?
                .filter_map(|e| e.ok().map(|(k, _)| k.value().to_string()))
                .collect();

            let completed = read_txn.open_table(COMPLETED)?;
            let completed_keys: Vec<String> = completed
                .iter()?
                .filter_map(|e| e.ok().map(|(k, _)| k.value().to_string()))
                .collect();

            (pending_keys, processing_keys, completed_keys)
        };

        let pending_count = pending_keys.len();
        let processing_count = processing_keys.len();
        let completed_count = completed_keys.len();

        let write_txn = self.db.begin_write()?;
        {
            let mut pending = write_txn.open_table(PENDING)?;
            for key in &pending_keys {
                pending.remove(key.as_str())?;
            }

            let mut processing = write_txn.open_table(PROCESSING)?;
            for key in &processing_keys {
                processing.remove(key.as_str())?;
            }

            let mut completed = write_txn.open_table(COMPLETED)?;
            for key in &completed_keys {
                completed.remove(key.as_str())?;
            }
        }
        write_txn.commit()?;

        Ok((pending_count, processing_count, completed_count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn setup_test_queue() -> (TaskQueue, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let queue = TaskQueue::new(db).unwrap();
        (queue, temp_dir)
    }

    #[test]
    fn test_insert_and_get_pending() {
        let (queue, _temp_dir) = setup_test_queue();

        let task_data = b"test task data";
        queue.insert_pending(100, "task-001", task_data).unwrap();

        let pending = queue.get_first_pending().unwrap();
        assert!(pending.is_some());

        let (priority, data) = pending.unwrap();
        assert_eq!(priority, 100);
        assert_eq!(data, task_data);
    }

    #[test]
    fn test_priority_order() {
        let (queue, _temp_dir) = setup_test_queue();

        queue
            .insert_pending(300, "task-low", b"low priority")
            .unwrap();
        queue
            .insert_pending(100, "task-high", b"high priority")
            .unwrap();
        queue
            .insert_pending(200, "task-med", b"medium priority")
            .unwrap();

        let first = queue.get_first_pending().unwrap().unwrap();
        assert_eq!(first.0, 100);
        assert_eq!(first.1, b"high priority");
    }

    #[test]
    fn test_move_to_processing() {
        let (queue, _temp_dir) = setup_test_queue();

        let task_data = b"task to process";
        queue.insert_pending(100, "task-001", task_data).unwrap();

        queue
            .move_to_processing(100, "task-001", task_data)
            .unwrap();

        let pending = queue.get_first_pending().unwrap();
        assert!(pending.is_none());

        let processing = queue.get_from_processing("task-001").unwrap();
        assert!(processing.is_some());
        assert_eq!(processing.unwrap(), task_data);
    }

    #[test]
    fn test_move_to_completed() {
        let (queue, _temp_dir) = setup_test_queue();

        let task_data = b"task to complete";

        queue.insert_pending(100, "task-001", task_data).unwrap();
        queue
            .move_to_processing(100, "task-001", task_data)
            .unwrap();

        queue.move_to_completed("task-001", task_data).unwrap();

        let processing = queue.get_from_processing("task-001").unwrap();
        assert!(processing.is_none());

        let completed = queue.get_all_completed().unwrap();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0], task_data);
    }

    #[test]
    fn test_get_from_any_table() {
        let (queue, _temp_dir) = setup_test_queue();

        queue
            .insert_pending(100, "task-001", b"processing task")
            .unwrap();
        queue
            .move_to_processing(100, "task-001", b"processing task")
            .unwrap();

        let result = queue.get_from_any_table("task-001").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), b"processing task");

        queue
            .move_to_completed("task-001", b"completed task")
            .unwrap();

        let result = queue.get_from_any_table("task-001").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), b"completed task");

        let result = queue.get_from_any_table("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_wait_for_task() {
        let (queue, _temp_dir) = setup_test_queue();

        let queue_clone = queue.clone();
        let wait_handle = tokio::spawn(async move {
            tokio::select! {
                _ = queue_clone.wait_for_task() => true,
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => false,
            }
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        queue.insert_pending(100, "task-001", b"new task").unwrap();

        let was_notified = wait_handle.await.unwrap();
        assert!(was_notified);
    }
}
