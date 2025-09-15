use anyhow::Result;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::sync::Arc;
use tokio::sync::Notify;

// KISS: Three-table design achieves O(1) pop vs single table's O(n) scan - simpler and faster
const PENDING: TableDefinition<u64, &[u8]> = TableDefinition::new("pending");
const PROCESSING: TableDefinition<&str, &[u8]> = TableDefinition::new("processing");
const COMPLETED: TableDefinition<&str, &[u8]> = TableDefinition::new("completed");

/// Pure storage layer for task queue - only handles data persistence
#[derive(Clone)]
pub struct TaskQueue {
    db: Arc<Database>,
    notify: Arc<Notify>,
}

impl TaskQueue {
    /// Create a new task queue instance
    pub fn new(db: Arc<Database>) -> Result<Self> {
        // Ensure tables exist
        let write_txn = db.begin_write()?;
        write_txn.open_table(PENDING)?;
        write_txn.open_table(PROCESSING)?;
        write_txn.open_table(COMPLETED)?;
        write_txn.commit()?;

        Ok(Self {
            db,
            notify: Arc::new(Notify::new()),
        })
    }

    /// Insert a task into the pending queue
    pub fn insert_pending(&self, priority: u64, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(PENDING)?;
            table.insert(priority, data)?;
        }
        write_txn.commit()?;
        self.notify.notify_one();
        Ok(())
    }

    /// Get the first pending task without removing it
    pub fn get_first_pending(&self) -> Result<Option<(u64, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let pending = read_txn.open_table(PENDING)?;
        
        if let Some((key, value)) = pending.first()? {
            Ok(Some((key.value(), value.value().to_vec())))
        } else {
            Ok(None)
        }
    }

    /// Move a task from pending to processing
    pub fn move_to_processing(&self, priority: u64, task_id: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        
        // Remove from pending
        {
            let mut pending = write_txn.open_table(PENDING)?;
            pending.remove(&priority)?;
        }
        
        // Add to processing
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
        
        // Remove from processing
        {
            let mut processing = write_txn.open_table(PROCESSING)?;
            processing.remove(task_id)?;
        }
        
        // Add to completed
        {
            let mut completed = write_txn.open_table(COMPLETED)?;
            completed.insert(task_id, data)?;
        }
        
        write_txn.commit()?;
        Ok(())
    }

    /// Get a task from processing table
    pub fn get_from_processing(&self, task_id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let processing = read_txn.open_table(PROCESSING)?;
        
        if let Some(data) = processing.get(task_id)? {
            Ok(Some(data.value().to_vec()))
        } else {
            Ok(None)
        }
    }

    /// Remove a task from processing table
    pub fn remove_from_processing(&self, task_id: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut processing = write_txn.open_table(PROCESSING)?;
            processing.remove(task_id)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get a task from any table
    pub fn get_from_any_table(&self, task_id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        
        // Check processing table
        let processing = read_txn.open_table(PROCESSING)?;
        if let Some(data) = processing.get(task_id)? {
            return Ok(Some(data.value().to_vec()));
        }
        
        // Check completed table
        let completed = read_txn.open_table(COMPLETED)?;
        if let Some(data) = completed.get(task_id)? {
            return Ok(Some(data.value().to_vec()));
        }
        
        // Check pending table (requires iteration)
        let pending = read_txn.open_table(PENDING)?;
        for entry in pending.iter()? {
            let (_, _value) = entry?;
            // Note: We'd need to deserialize to check ID, but for pure storage we return None
            // The scheduler should handle this logic
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
    pub async fn wait_for_task(&self) {
        self.notify.notified().await;
    }

    /// Notify that a task is available
    pub fn notify_task_available(&self) {
        self.notify.notify_one();
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
        queue.insert_pending(100, task_data).unwrap();

        let pending = queue.get_first_pending().unwrap();
        assert!(pending.is_some());

        let (priority, data) = pending.unwrap();
        assert_eq!(priority, 100);
        assert_eq!(data, task_data);
    }

    #[test]
    fn test_priority_order() {
        let (queue, _temp_dir) = setup_test_queue();

        // Insert tasks with different priorities
        queue.insert_pending(300, b"low priority").unwrap();
        queue.insert_pending(100, b"high priority").unwrap();
        queue.insert_pending(200, b"medium priority").unwrap();

        // Should get highest priority (lowest number) first
        let first = queue.get_first_pending().unwrap().unwrap();
        assert_eq!(first.0, 100);
        assert_eq!(first.1, b"high priority");
    }

    #[test]
    fn test_move_to_processing() {
        let (queue, _temp_dir) = setup_test_queue();

        let task_data = b"task to process";
        queue.insert_pending(100, task_data).unwrap();

        // Move to processing
        queue.move_to_processing(100, "task-001", task_data).unwrap();

        // Should no longer be in pending
        let pending = queue.get_first_pending().unwrap();
        assert!(pending.is_none());

        // Should be in processing
        let processing = queue.get_from_processing("task-001").unwrap();
        assert!(processing.is_some());
        assert_eq!(processing.unwrap(), task_data);
    }

    #[test]
    fn test_move_to_completed() {
        let (queue, _temp_dir) = setup_test_queue();

        let task_data = b"task to complete";

        // First move to processing
        queue.insert_pending(100, task_data).unwrap();
        queue.move_to_processing(100, "task-001", task_data).unwrap();

        // Then move to completed
        queue.move_to_completed("task-001", task_data).unwrap();

        // Should no longer be in processing
        let processing = queue.get_from_processing("task-001").unwrap();
        assert!(processing.is_none());

        // Should be in completed (check via get_all_completed)
        let completed = queue.get_all_completed().unwrap();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0], task_data);
    }

    #[test]
    fn test_remove_from_processing() {
        let (queue, _temp_dir) = setup_test_queue();

        let task_data = b"task to remove";
        queue.insert_pending(100, task_data).unwrap();
        queue.move_to_processing(100, "task-001", task_data).unwrap();

        // Remove from processing
        queue.remove_from_processing("task-001").unwrap();

        // Should no longer be in processing
        let processing = queue.get_from_processing("task-001").unwrap();
        assert!(processing.is_none());
    }

    #[test]
    fn test_get_all_pending() {
        let (queue, _temp_dir) = setup_test_queue();

        queue.insert_pending(100, b"task1").unwrap();
        queue.insert_pending(200, b"task2").unwrap();
        queue.insert_pending(300, b"task3").unwrap();

        let pending = queue.get_all_pending().unwrap();
        assert_eq!(pending.len(), 3);
    }

    #[test]
    fn test_get_all_processing() {
        let (queue, _temp_dir) = setup_test_queue();

        queue.insert_pending(100, b"task1").unwrap();
        queue.move_to_processing(100, "task-001", b"task1").unwrap();

        queue.insert_pending(200, b"task2").unwrap();
        queue.move_to_processing(200, "task-002", b"task2").unwrap();

        let processing = queue.get_all_processing().unwrap();
        assert_eq!(processing.len(), 2);
    }

    #[test]
    fn test_get_all_completed() {
        let (queue, _temp_dir) = setup_test_queue();

        // Create and complete multiple tasks
        for i in 1..=3 {
            let task_id = format!("task-{:03}", i);
            let task_data = format!("task{}", i).into_bytes();

            queue.insert_pending(i as u64 * 100, &task_data).unwrap();
            queue.move_to_processing(i as u64 * 100, &task_id, &task_data).unwrap();
            queue.move_to_completed(&task_id, &task_data).unwrap();
        }

        let completed = queue.get_all_completed().unwrap();
        assert_eq!(completed.len(), 3);
    }

    #[tokio::test]
    async fn test_wait_for_task() {
        let (queue, _temp_dir) = setup_test_queue();

        // Spawn a task that waits for notification
        let queue_clone = queue.clone();
        let wait_handle = tokio::spawn(async move {
            tokio::select! {
                _ = queue_clone.wait_for_task() => true,
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => false,
            }
        });

        // Give the task time to start waiting
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Insert a task which should trigger notification
        queue.insert_pending(100, b"new task").unwrap();

        // Wait should complete quickly
        let was_notified = wait_handle.await.unwrap();
        assert!(was_notified);
    }

    #[test]
    fn test_get_from_any_table() {
        let (queue, _temp_dir) = setup_test_queue();

        // Test task in processing
        queue.insert_pending(100, b"processing task").unwrap();
        queue.move_to_processing(100, "task-001", b"processing task").unwrap();

        let result = queue.get_from_any_table("task-001").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), b"processing task");

        // Test task in completed
        queue.move_to_completed("task-001", b"completed task").unwrap();

        let result = queue.get_from_any_table("task-001").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), b"completed task");

        // Test non-existent task
        let result = queue.get_from_any_table("nonexistent").unwrap();
        assert!(result.is_none());
    }
}