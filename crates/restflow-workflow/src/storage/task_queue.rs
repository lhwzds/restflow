//! Typed task queue wrapper.
//!
//! Provides typed access to the task queue with Task model.

use crate::models::Task;
use anyhow::Result;
use redb::Database;
use std::sync::Arc;

/// Typed task queue wrapper around restflow-storage::TaskQueue.
#[derive(Clone)]
pub struct TaskQueue {
    inner: restflow_storage::TaskQueue,
}

impl TaskQueue {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Ok(Self {
            inner: restflow_storage::TaskQueue::new(db)?,
        })
    }

    /// Insert a task into the pending queue
    pub fn insert_pending(&self, priority: u64, task_id: &str, task: &Task) -> Result<()> {
        let data = serde_json::to_vec(task)?;
        self.inner.insert_pending(priority, task_id, &data)
    }

    /// Atomically pop the first pending task and move it to processing
    /// Accepts a callback to update task state within the same transaction
    pub fn atomic_pop_pending<F>(&self, on_task: F) -> Result<Option<Task>>
    where
        F: FnOnce(&mut Task),
    {
        let result = self.inner.atomic_pop_pending(|data| {
            let mut task: Task = serde_json::from_slice(data)?;
            on_task(&mut task);
            Ok(serde_json::to_vec(&task)?)
        })?;

        match result {
            Some(data) => Ok(Some(serde_json::from_slice(&data)?)),
            None => Ok(None),
        }
    }

    /// Get the first pending task without removing it
    pub fn get_first_pending(&self) -> Result<Option<(u64, Task)>> {
        if let Some((priority, data)) = self.inner.get_first_pending()? {
            let task: Task = serde_json::from_slice(&data)?;
            Ok(Some((priority, task)))
        } else {
            Ok(None)
        }
    }

    /// Move a task from pending to processing
    pub fn move_to_processing(&self, priority: u64, task_id: &str, task: &Task) -> Result<()> {
        let data = serde_json::to_vec(task)?;
        self.inner.move_to_processing(priority, task_id, &data)
    }

    /// Move a task from processing to completed
    pub fn move_to_completed(&self, task_id: &str, task: &Task) -> Result<()> {
        let data = serde_json::to_vec(task)?;
        self.inner.move_to_completed(task_id, &data)
    }

    /// Get task from processing table
    pub fn get_from_processing(&self, task_id: &str) -> Result<Option<Task>> {
        if let Some(data) = self.inner.get_from_processing(task_id)? {
            let task: Task = serde_json::from_slice(&data)?;
            Ok(Some(task))
        } else {
            Ok(None)
        }
    }

    /// Remove task from processing table
    pub fn remove_from_processing(&self, task_id: &str) -> Result<()> {
        self.inner.remove_from_processing(task_id)
    }

    /// Get task from any table
    pub fn get_from_any_table(&self, task_id: &str) -> Result<Option<Task>> {
        if let Some(data) = self.inner.get_from_any_table(task_id)? {
            let task: Task = serde_json::from_slice(&data)?;
            Ok(Some(task))
        } else {
            Ok(None)
        }
    }

    /// Get all tasks from pending table
    pub fn get_all_pending(&self) -> Result<Vec<Task>> {
        let raw_tasks = self.inner.get_all_pending()?;
        let mut tasks = Vec::new();
        for data in raw_tasks {
            let task: Task = serde_json::from_slice(&data)?;
            tasks.push(task);
        }
        Ok(tasks)
    }

    /// Get all tasks from processing table
    pub fn get_all_processing(&self) -> Result<Vec<Task>> {
        let raw_tasks = self.inner.get_all_processing()?;
        let mut tasks = Vec::new();
        for data in raw_tasks {
            let task: Task = serde_json::from_slice(&data)?;
            tasks.push(task);
        }
        Ok(tasks)
    }

    /// Get all tasks from completed table
    pub fn get_all_completed(&self) -> Result<Vec<Task>> {
        let raw_tasks = self.inner.get_all_completed()?;
        let mut tasks = Vec::new();
        for data in raw_tasks {
            let task: Task = serde_json::from_slice(&data)?;
            tasks.push(task);
        }
        Ok(tasks)
    }

    /// Wait for a task to be available
    pub async fn wait_for_task(&self) {
        self.inner.wait_for_task().await
    }

    /// Check if there are pending tasks
    pub fn has_pending_tasks(&self) -> bool {
        self.inner.has_pending_tasks()
    }

    /// Clear all tasks from all queues
    pub fn clear_all(&self) -> Result<(usize, usize, usize)> {
        self.inner.clear_all()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::context::ExecutionContext;
    use crate::models::{ManualTriggerInput, NodeInput, TaskStatus};
    use tempfile::tempdir;

    fn setup_test_queue() -> (TaskQueue, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let queue = TaskQueue::new(db).unwrap();
        (queue, temp_dir)
    }

    fn create_test_input() -> NodeInput {
        NodeInput::ManualTrigger(ManualTriggerInput {
            payload: Some(serde_json::json!({})),
        })
    }

    fn create_test_task(exec_id: &str, node_id: &str) -> Task {
        Task::new(
            exec_id.to_string(),
            "wf-1".to_string(),
            node_id.to_string(),
            create_test_input(),
            ExecutionContext::new(exec_id.to_string()),
        )
    }

    #[test]
    fn test_insert_and_get_pending() {
        let (queue, _temp_dir) = setup_test_queue();

        let task = create_test_task("exec-1", "node-1");
        let task_id = task.id.clone();
        let priority = task.priority();
        queue.insert_pending(priority, &task_id, &task).unwrap();

        let pending = queue.get_first_pending().unwrap();
        assert!(pending.is_some());

        let (retrieved_priority, retrieved_task) = pending.unwrap();
        assert_eq!(retrieved_priority, priority);
        assert_eq!(retrieved_task.id, task_id);
    }

    #[test]
    fn test_atomic_pop_state_transition() {
        let (queue, _temp_dir) = setup_test_queue();

        let task = create_test_task("exec-1", "node-1");
        let task_id = task.id.clone();
        let priority = task.priority();
        queue.insert_pending(priority, &task_id, &task).unwrap();

        // Atomic pop with state update
        let popped_task = queue
            .atomic_pop_pending(|task| task.start())
            .unwrap()
            .unwrap();

        assert_eq!(popped_task.id, task_id);
        assert_eq!(popped_task.status, TaskStatus::Running);
        assert!(popped_task.started_at.is_some());

        // Verify pending is empty
        assert_eq!(queue.get_all_pending().unwrap().len(), 0);

        // Verify task in processing has Running status
        let processing_task = queue.get_from_processing(&task_id).unwrap().unwrap();
        assert_eq!(processing_task.status, TaskStatus::Running);
    }

    #[test]
    fn test_move_to_completed() {
        let (queue, _temp_dir) = setup_test_queue();

        let mut task = create_test_task("exec-1", "node-1");
        let task_id = task.id.clone();
        let priority = task.priority();
        queue.insert_pending(priority, &task_id, &task).unwrap();
        queue.move_to_processing(priority, &task_id, &task).unwrap();

        task.complete(crate::models::NodeOutput::Print(crate::models::PrintOutput {
            printed: "test".to_string(),
        }));
        queue.move_to_completed(&task_id, &task).unwrap();

        let processing = queue.get_from_processing(&task_id).unwrap();
        assert!(processing.is_none());

        let completed = queue.get_all_completed().unwrap();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].id, task_id);
    }

    #[test]
    fn test_get_from_any_table() {
        let (queue, _temp_dir) = setup_test_queue();

        let task = create_test_task("exec-1", "node-1");
        let task_id = task.id.clone();
        let priority = task.priority();
        queue.insert_pending(priority, &task_id, &task).unwrap();

        // Find in pending
        let result = queue.get_from_any_table(&task_id).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, task_id);

        // Move to processing and find
        queue.move_to_processing(priority, &task_id, &task).unwrap();
        let result = queue.get_from_any_table(&task_id).unwrap();
        assert!(result.is_some());

        // Non-existent
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

        let task = create_test_task("exec-1", "node-1");
        let task_id = task.id.clone();
        let priority = task.priority();
        queue.insert_pending(priority, &task_id, &task).unwrap();

        let was_notified = wait_handle.await.unwrap();
        assert!(was_notified);
    }
}
