use anyhow::Result;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::sync::Arc;
use tokio::sync::Notify;

// KISS: Three-table design achieves O(1) pop vs single table's O(n) scan - simpler and faster
// PENDING uses composite key "{priority:020}:{task_id}" for uniqueness and correct ordering
const PENDING: TableDefinition<&str, &[u8]> = TableDefinition::new("pending");
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
        self.notify.notify_one();
        Ok(())
    }

    /// Atomically pop the first pending task and move it to processing
    /// Accepts a callback to update task state within the same transaction
    /// This prevents race conditions and ensures atomicity of pop→update→save
    pub fn atomic_pop_pending<F>(&self, on_task: F) -> Result<Option<crate::models::Task>>
    where
        F: FnOnce(&mut crate::models::Task),
    {
        let write_txn = self.db.begin_write()?;

        // Process the task in a scope to drop table borrows before commit/abort
        let task = {
            let mut pending = write_txn.open_table(PENDING)?;

            // Extract first entry into owned values
            let first_entry = if let Some(first) = pending.first()? {
                let key_str = first.0.value().to_string();
                let data = first.1.value().to_vec();
                Some((key_str, data))
            } else {
                None
            };

            // Process the task if found
            if let Some((key, data)) = first_entry {
                // Remove from pending
                pending.remove(key.as_str())?;

                // Deserialize and update task state via callback
                let mut task: crate::models::Task = serde_json::from_slice(&data)?;
                on_task(&mut task);

                // Write updated task to processing table
                let serialized = serde_json::to_vec(&task)?;
                let mut processing = write_txn.open_table(PROCESSING)?;
                processing.insert(task.id.as_str(), serialized.as_slice())?;

                Some(task)
            } else {
                None
            }
        }; // ← Tables dropped here

        if task.is_some() {
            write_txn.commit()?;
        } else {
            write_txn.abort()?;
        }

        Ok(task)
    }

    /// Get the first pending task without removing it
    /// Returns (priority, data) - note: priority extracted from composite key
    pub fn get_first_pending(&self) -> Result<Option<(u64, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let pending = read_txn.open_table(PENDING)?;

        if let Some((key, value)) = pending.first()? {
            // Extract priority from composite key "{priority:020}:{task_id}"
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

    /// Move a task from pending to processing (legacy method for tests)
    /// Prefer atomic_pop_pending() for production code to avoid race conditions
    pub fn move_to_processing(&self, priority: u64, task_id: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;

        // Remove from pending using composite key
        {
            let mut pending = write_txn.open_table(PENDING)?;
            let key = format!("{:020}:{}", priority, task_id);
            pending.remove(key.as_str())?;
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

        // Check pending table (requires iteration and deserialization)
        // Note: This is O(n) but pending queue should be relatively small
        let pending = read_txn.open_table(PENDING)?;
        for entry in pending.iter()? {
            let (_, value) = entry?;
            let data = value.value();

            // Deserialize to check task ID
            if let Ok(task) = serde_json::from_slice::<crate::models::Task>(data)
                && task.id == task_id
            {
                return Ok(Some(data.to_vec()));
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

    fn create_test_input() -> crate::models::NodeInput {
        use crate::models::{ManualTriggerInput, NodeInput};

        NodeInput::ManualTrigger(ManualTriggerInput {
            payload: Some(serde_json::json!({})),
        })
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

        // Insert tasks with different priorities
        queue
            .insert_pending(300, "task-low", b"low priority")
            .unwrap();
        queue
            .insert_pending(100, "task-high", b"high priority")
            .unwrap();
        queue
            .insert_pending(200, "task-med", b"medium priority")
            .unwrap();

        // Should get highest priority (lowest number) first
        let first = queue.get_first_pending().unwrap().unwrap();
        assert_eq!(first.0, 100);
        assert_eq!(first.1, b"high priority");
    }

    #[test]
    fn test_move_to_processing() {
        let (queue, _temp_dir) = setup_test_queue();

        let task_data = b"task to process";
        queue.insert_pending(100, "task-001", task_data).unwrap();

        // Move to processing
        queue
            .move_to_processing(100, "task-001", task_data)
            .unwrap();

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
        queue.insert_pending(100, "task-001", task_data).unwrap();
        queue
            .move_to_processing(100, "task-001", task_data)
            .unwrap();

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
        queue.insert_pending(100, "task-001", task_data).unwrap();
        queue
            .move_to_processing(100, "task-001", task_data)
            .unwrap();

        // Remove from processing
        queue.remove_from_processing("task-001").unwrap();

        // Should no longer be in processing
        let processing = queue.get_from_processing("task-001").unwrap();
        assert!(processing.is_none());
    }

    #[test]
    fn test_get_all_pending() {
        let (queue, _temp_dir) = setup_test_queue();

        queue.insert_pending(100, "task-001", b"task1").unwrap();
        queue.insert_pending(200, "task-002", b"task2").unwrap();
        queue.insert_pending(300, "task-003", b"task3").unwrap();

        let pending = queue.get_all_pending().unwrap();
        assert_eq!(pending.len(), 3);
    }

    #[test]
    fn test_get_all_processing() {
        let (queue, _temp_dir) = setup_test_queue();

        queue.insert_pending(100, "task-001", b"task1").unwrap();
        queue.move_to_processing(100, "task-001", b"task1").unwrap();

        queue.insert_pending(200, "task-002", b"task2").unwrap();
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

            queue
                .insert_pending(i as u64 * 100, &task_id, &task_data)
                .unwrap();
            queue
                .move_to_processing(i as u64 * 100, &task_id, &task_data)
                .unwrap();
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
        queue.insert_pending(100, "task-001", b"new task").unwrap();

        // Wait should complete quickly
        let was_notified = wait_handle.await.unwrap();
        assert!(was_notified);
    }

    #[test]
    fn test_get_from_any_table() {
        let (queue, _temp_dir) = setup_test_queue();

        // Test task in processing
        queue
            .insert_pending(100, "task-001", b"processing task")
            .unwrap();
        queue
            .move_to_processing(100, "task-001", b"processing task")
            .unwrap();

        let result = queue.get_from_any_table("task-001").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), b"processing task");

        // Test task in completed
        queue
            .move_to_completed("task-001", b"completed task")
            .unwrap();

        let result = queue.get_from_any_table("task-001").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), b"completed task");

        // Test non-existent task
        let result = queue.get_from_any_table("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_concurrent_same_priority_nanosecond() {
        use crate::engine::context::ExecutionContext;
        use crate::models::Task;

        let (queue, _temp_dir) = setup_test_queue();

        // Create 10 tasks concurrently (simulating high concurrency)
        let mut handles = vec![];
        for i in 0..10 {
            let queue_clone = queue.clone();
            let handle = tokio::spawn(async move {
                let task = Task::new(
                    format!("exec-{}", i),
                    "wf-1".to_string(),
                    format!("node-{}", i),
                    create_test_input(),
                    ExecutionContext::new(format!("exec-{}", i)),
                );
                let priority = task.priority();
                let task_id = task.id.clone();
                let serialized = serde_json::to_vec(&task).unwrap();
                queue_clone
                    .insert_pending(priority, &task_id, &serialized)
                    .unwrap();
            });
            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            handle.await.unwrap();
        }

        // Verify all 10 tasks are in pending
        let pending = queue.get_all_pending().unwrap();
        assert_eq!(pending.len(), 10, "All 10 tasks should be in pending queue");
    }

    #[test]
    fn test_get_from_any_table_pending() {
        use crate::engine::context::ExecutionContext;
        use crate::models::Task;

        let (queue, _temp_dir) = setup_test_queue();

        // Create a task and add to pending
        let task = Task::new(
            "exec-1".to_string(),
            "wf-1".to_string(),
            "node-1".to_string(),
            create_test_input(),
            ExecutionContext::new("exec-1".to_string()),
        );
        let task_id = task.id.clone();
        let priority = task.priority();
        let serialized = serde_json::to_vec(&task).unwrap();
        queue
            .insert_pending(priority, &task_id, &serialized)
            .unwrap();

        // Should find task in pending
        let result = queue.get_from_any_table(&task_id).unwrap();
        assert!(result.is_some(), "Should find task in pending table");

        // Deserialize and verify
        let found_task: Task = serde_json::from_slice(&result.unwrap()).unwrap();
        assert_eq!(found_task.id, task_id);
    }

    #[tokio::test]
    async fn test_concurrent_pop_no_duplicate() {
        use crate::engine::context::ExecutionContext;
        use crate::models::Task;
        use std::collections::HashSet;

        let (queue, _temp_dir) = setup_test_queue();

        // Insert 3 tasks
        for i in 0..3 {
            let task = Task::new(
                format!("exec-{}", i),
                "wf-1".to_string(),
                format!("node-{}", i),
                create_test_input(),
                ExecutionContext::new(format!("exec-{}", i)),
            );
            let priority = task.priority();
            let task_id = task.id.clone();
            let serialized = serde_json::to_vec(&task).unwrap();
            queue
                .insert_pending(priority, &task_id, &serialized)
                .unwrap();
        }

        // 10 workers concurrently pop (with no-op callback)
        let mut handles = vec![];
        for _ in 0..10 {
            let q = queue.clone();
            handles.push(tokio::spawn(async move {
                q.atomic_pop_pending(|_| {}).ok().flatten()
            }));
        }

        // Collect results
        let mut results = vec![];
        for h in handles {
            if let Some(task) = h.await.unwrap() {
                results.push(task.id); // task_id
            }
        }

        // Verify: exactly 3 tasks, no duplicates
        assert_eq!(results.len(), 3, "Should pop exactly 3 tasks");
        let unique: HashSet<_> = results.into_iter().collect();
        assert_eq!(
            unique.len(),
            3,
            "All task IDs should be unique (no duplicate execution)"
        );
    }

    #[test]
    fn test_composite_key_uniqueness() {
        use crate::engine::context::ExecutionContext;
        use crate::models::Task;

        let (queue, _temp_dir) = setup_test_queue();

        // Create 5 tasks with same priority
        let mut tasks = vec![];
        for i in 0..5 {
            let task = Task::new(
                "exec-1".to_string(),
                "wf-1".to_string(),
                format!("node-{}", i),
                create_test_input(),
                ExecutionContext::new("exec-1".to_string()),
            );
            tasks.push(task);
        }

        // Insert all with same priority
        let priority = tasks[0].priority();
        for task in &tasks {
            let serialized = serde_json::to_vec(task).unwrap();
            queue
                .insert_pending(priority, &task.id, &serialized)
                .unwrap();
        }

        // Verify all 5 tasks are preserved (no silent overwrite)
        let pending = queue.get_all_pending().unwrap();
        assert_eq!(
            pending.len(),
            5,
            "All tasks should be preserved despite same priority"
        );

        // Verify all can be retrieved by ID
        for task in &tasks {
            let result = queue.get_from_any_table(&task.id).unwrap();
            assert!(result.is_some(), "Each task should be retrievable by ID");
        }
    }

    #[test]
    fn test_atomic_pop_state_transition() {
        use crate::engine::context::ExecutionContext;
        use crate::models::{Task, TaskStatus};

        let (queue, _temp_dir) = setup_test_queue();

        let task = Task::new(
            "exec-1".to_string(),
            "wf-1".to_string(),
            "node-1".to_string(),
            create_test_input(),
            ExecutionContext::new("exec-1".to_string()),
        );
        let task_id = task.id.clone();
        let priority = task.priority();
        let serialized = serde_json::to_vec(&task).unwrap();
        queue
            .insert_pending(priority, &task_id, &serialized)
            .unwrap();

        // Atomic pop with state update
        let popped_task = queue
            .atomic_pop_pending(|task| task.start())
            .unwrap()
            .unwrap();
        assert_eq!(popped_task.id, task_id);

        // ✅ Verify: task status was updated atomically
        assert_eq!(
            popped_task.status,
            TaskStatus::Running,
            "Task should be Running after pop"
        );
        assert!(
            popped_task.started_at.is_some(),
            "Task should have started_at set"
        );

        // Verify: pending is now empty
        assert_eq!(
            queue.get_all_pending().unwrap().len(),
            0,
            "Pending should be empty after pop"
        );

        // Verify: task in processing has Running status (not Pending)
        let processing_data = queue.get_from_processing(&task_id).unwrap().unwrap();
        let processing_task: Task = serde_json::from_slice(&processing_data).unwrap();
        assert_eq!(
            processing_task.status,
            TaskStatus::Running,
            "Processing task should be Running"
        );
        assert!(
            processing_task.started_at.is_some(),
            "Processing task should have started_at"
        );

        // Verify second pop returns None (no duplicate)
        let second_pop = queue.atomic_pop_pending(|task| task.start()).unwrap();
        assert!(second_pop.is_none(), "Second pop should return None");
    }

    #[test]
    fn test_atomic_pop_no_dirty_data_on_crash() {
        use crate::engine::context::ExecutionContext;
        use crate::models::Task;

        let (queue, _temp_dir) = setup_test_queue();

        // Insert a task
        let task = Task::new(
            "exec-1".to_string(),
            "wf-1".to_string(),
            "node-1".to_string(),
            create_test_input(),
            ExecutionContext::new("exec-1".to_string()),
        );
        let task_id = task.id.clone();
        let priority = task.priority();
        let serialized = serde_json::to_vec(&task).unwrap();
        queue
            .insert_pending(priority, &task_id, &serialized)
            .unwrap();

        // Simulate callback panic (transaction should abort)
        let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            queue.atomic_pop_pending(|_task| {
                panic!("Simulated worker crash in callback!");
            })
        }));

        assert!(panic_result.is_err(), "Callback should panic");

        // ✅ Verify: task remains in pending (not moved to processing)
        let pending = queue.get_all_pending().unwrap();
        assert_eq!(
            pending.len(),
            1,
            "Task should still be in pending after panic"
        );

        // ✅ Verify: processing table is empty (no dirty data)
        let processing = queue.get_all_processing().unwrap();
        assert_eq!(
            processing.len(),
            0,
            "Processing should be empty (no dirty data)"
        );

        // ✅ Verify: task can be popped again successfully
        let retry_task = queue.atomic_pop_pending(|task| task.start()).unwrap();
        assert!(
            retry_task.is_some(),
            "Task should be retrievable after panic"
        );
        assert_eq!(retry_task.unwrap().id, task_id);
    }
}
