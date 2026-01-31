//! Typed agent task storage wrapper.
//!
//! Provides type-safe access to agent task storage by wrapping the byte-level
//! APIs from restflow-storage with Rust types from our models.

use crate::models::{AgentTask, AgentTaskStatus, TaskEvent, TaskEventType, TaskSchedule};
use anyhow::Result;
use redb::Database;
use std::sync::Arc;
use uuid::Uuid;

/// Typed agent task storage wrapper around restflow-storage::AgentTaskStorage.
#[derive(Clone)]
pub struct AgentTaskStorage {
    inner: restflow_storage::AgentTaskStorage,
}

impl AgentTaskStorage {
    /// Create a new AgentTaskStorage instance
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Ok(Self {
            inner: restflow_storage::AgentTaskStorage::new(db)?,
        })
    }

    // ============== Agent Task Operations ==============

    /// Create a new agent task
    pub fn create_task(
        &self,
        name: String,
        agent_id: String,
        schedule: TaskSchedule,
    ) -> Result<AgentTask> {
        let task = AgentTask::new(Uuid::new_v4().to_string(), name, agent_id, schedule);

        let json_bytes = serde_json::to_vec(&task)?;
        self.inner.put_task_raw(&task.id, &json_bytes)?;

        // Create a "created" event
        let event =
            TaskEvent::new(task.id.clone(), TaskEventType::Created).with_message("Task created");
        self.add_event(&event)?;

        Ok(task)
    }

    /// Get an agent task by ID
    pub fn get_task(&self, id: &str) -> Result<Option<AgentTask>> {
        if let Some(bytes) = self.inner.get_task_raw(id)? {
            let task: AgentTask = serde_json::from_slice(&bytes)?;
            Ok(Some(task))
        } else {
            Ok(None)
        }
    }

    /// List all agent tasks
    pub fn list_tasks(&self) -> Result<Vec<AgentTask>> {
        let tasks = self.inner.list_tasks_raw()?;
        let mut result = Vec::new();
        for (_, bytes) in tasks {
            let task: AgentTask = serde_json::from_slice(&bytes)?;
            result.push(task);
        }
        Ok(result)
    }

    /// List tasks filtered by status
    pub fn list_tasks_by_status(&self, status: AgentTaskStatus) -> Result<Vec<AgentTask>> {
        let tasks = self.list_tasks()?;
        Ok(tasks.into_iter().filter(|t| t.status == status).collect())
    }

    /// List tasks that are ready to run
    pub fn list_runnable_tasks(&self, current_time: i64) -> Result<Vec<AgentTask>> {
        let tasks = self.list_tasks()?;
        Ok(tasks
            .into_iter()
            .filter(|t| t.should_run(current_time))
            .collect())
    }

    /// Update an existing agent task
    pub fn update_task(&self, task: &AgentTask) -> Result<()> {
        let json_bytes = serde_json::to_vec(task)?;
        self.inner.put_task_raw(&task.id, &json_bytes)?;
        Ok(())
    }

    /// Delete an agent task and all its events
    pub fn delete_task(&self, id: &str) -> Result<bool> {
        // First delete all events for this task
        self.inner.delete_events_for_task(id)?;
        // Then delete the task itself
        self.inner.delete_task(id)
    }

    /// Pause an agent task
    pub fn pause_task(&self, id: &str) -> Result<AgentTask> {
        let mut task = self
            .get_task(id)?
            .ok_or_else(|| anyhow::anyhow!("Task {} not found", id))?;

        task.pause();
        self.update_task(&task)?;

        // Record the pause event
        let event =
            TaskEvent::new(task.id.clone(), TaskEventType::Paused).with_message("Task paused");
        self.add_event(&event)?;

        Ok(task)
    }

    /// Resume an agent task
    pub fn resume_task(&self, id: &str) -> Result<AgentTask> {
        let mut task = self
            .get_task(id)?
            .ok_or_else(|| anyhow::anyhow!("Task {} not found", id))?;

        task.resume();
        self.update_task(&task)?;

        // Record the resume event
        let event =
            TaskEvent::new(task.id.clone(), TaskEventType::Resumed).with_message("Task resumed");
        self.add_event(&event)?;

        Ok(task)
    }

    /// Mark a task as running
    pub fn start_task_execution(&self, id: &str) -> Result<AgentTask> {
        let mut task = self
            .get_task(id)?
            .ok_or_else(|| anyhow::anyhow!("Task {} not found", id))?;

        task.set_running();
        self.update_task(&task)?;

        // Record the start event
        let event = TaskEvent::new(task.id.clone(), TaskEventType::Started)
            .with_message("Task execution started");
        self.add_event(&event)?;

        Ok(task)
    }

    /// Mark a task as completed
    pub fn complete_task_execution(
        &self,
        id: &str,
        output: Option<String>,
        duration_ms: i64,
    ) -> Result<AgentTask> {
        let mut task = self
            .get_task(id)?
            .ok_or_else(|| anyhow::anyhow!("Task {} not found", id))?;

        task.set_completed();
        self.update_task(&task)?;

        // Record the completion event
        let mut event = TaskEvent::new(task.id.clone(), TaskEventType::Completed)
            .with_message("Task execution completed")
            .with_duration(duration_ms);
        if let Some(out) = output {
            event = event.with_output(out);
        }
        self.add_event(&event)?;

        Ok(task)
    }

    /// Mark a task as failed
    pub fn fail_task_execution(
        &self,
        id: &str,
        error: String,
        duration_ms: i64,
    ) -> Result<AgentTask> {
        let mut task = self
            .get_task(id)?
            .ok_or_else(|| anyhow::anyhow!("Task {} not found", id))?;

        task.set_failed(error.clone());
        self.update_task(&task)?;

        // Record the failure event
        let event = TaskEvent::new(task.id.clone(), TaskEventType::Failed)
            .with_message(error)
            .with_duration(duration_ms);
        self.add_event(&event)?;

        Ok(task)
    }

    // ============== Task Event Operations ==============

    /// Add a new event for a task
    pub fn add_event(&self, event: &TaskEvent) -> Result<()> {
        let json_bytes = serde_json::to_vec(event)?;
        self.inner
            .put_event_raw(&event.id, &event.task_id, &json_bytes)?;
        Ok(())
    }

    /// Get an event by ID
    pub fn get_event(&self, event_id: &str) -> Result<Option<TaskEvent>> {
        if let Some(bytes) = self.inner.get_event_raw(event_id)? {
            let event: TaskEvent = serde_json::from_slice(&bytes)?;
            Ok(Some(event))
        } else {
            Ok(None)
        }
    }

    /// List all events for a task
    pub fn list_events_for_task(&self, task_id: &str) -> Result<Vec<TaskEvent>> {
        let events = self.inner.list_events_for_task_raw(task_id)?;
        let mut result = Vec::new();
        for (_, bytes) in events {
            let event: TaskEvent = serde_json::from_slice(&bytes)?;
            result.push(event);
        }

        // Sort by timestamp descending (most recent first)
        result.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        Ok(result)
    }

    /// List recent events for a task (with limit)
    pub fn list_recent_events_for_task(
        &self,
        task_id: &str,
        limit: usize,
    ) -> Result<Vec<TaskEvent>> {
        let events = self.list_events_for_task(task_id)?;
        Ok(events.into_iter().take(limit).collect())
    }

    /// Record a notification event
    pub fn record_notification_sent(&self, task_id: &str, message: String) -> Result<()> {
        let event = TaskEvent::new(task_id.to_string(), TaskEventType::NotificationSent)
            .with_message(message);
        self.add_event(&event)
    }

    /// Record a notification failure event
    pub fn record_notification_failed(&self, task_id: &str, error: String) -> Result<()> {
        let event = TaskEvent::new(task_id.to_string(), TaskEventType::NotificationFailed)
            .with_message(error);
        self.add_event(&event)
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
    fn test_create_and_get_task() {
        let storage = create_test_storage();

        let task = storage
            .create_task(
                "Test Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Interval {
                    interval_ms: 3600000,
                    start_at: None,
                },
            )
            .unwrap();

        assert!(!task.id.is_empty());
        assert_eq!(task.name, "Test Task");
        assert_eq!(task.agent_id, "agent-001");
        assert_eq!(task.status, AgentTaskStatus::Active);

        let retrieved = storage.get_task(&task.id).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Test Task");
    }

    #[test]
    fn test_list_tasks() {
        let storage = create_test_storage();

        storage
            .create_task(
                "Task 1".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();
        storage
            .create_task(
                "Task 2".to_string(),
                "agent-002".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        let tasks = storage.list_tasks().unwrap();
        assert_eq!(tasks.len(), 2);
    }

    #[test]
    fn test_list_tasks_by_status() {
        let storage = create_test_storage();

        let task1 = storage
            .create_task(
                "Active Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        let task2 = storage
            .create_task(
                "Will be Paused".to_string(),
                "agent-002".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        storage.pause_task(&task2.id).unwrap();

        let active_tasks = storage
            .list_tasks_by_status(AgentTaskStatus::Active)
            .unwrap();
        let paused_tasks = storage
            .list_tasks_by_status(AgentTaskStatus::Paused)
            .unwrap();

        assert_eq!(active_tasks.len(), 1);
        assert_eq!(active_tasks[0].id, task1.id);
        assert_eq!(paused_tasks.len(), 1);
        assert_eq!(paused_tasks[0].id, task2.id);
    }

    #[test]
    fn test_delete_task() {
        let storage = create_test_storage();

        let task = storage
            .create_task(
                "To Delete".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        // Add some events
        let event = TaskEvent::new(task.id.clone(), TaskEventType::Started);
        storage.add_event(&event).unwrap();

        // Delete the task
        let deleted = storage.delete_task(&task.id).unwrap();
        assert!(deleted);

        // Task should be gone
        let retrieved = storage.get_task(&task.id).unwrap();
        assert!(retrieved.is_none());

        // Events should also be gone
        let events = storage.list_events_for_task(&task.id).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn test_pause_and_resume_task() {
        let storage = create_test_storage();

        let task = storage
            .create_task(
                "Test Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        // Pause the task
        let paused = storage.pause_task(&task.id).unwrap();
        assert_eq!(paused.status, AgentTaskStatus::Paused);

        // Resume the task
        let resumed = storage.resume_task(&task.id).unwrap();
        assert_eq!(resumed.status, AgentTaskStatus::Active);

        // Check events were recorded
        let events = storage.list_events_for_task(&task.id).unwrap();
        let event_types: Vec<_> = events.iter().map(|e| &e.event_type).collect();
        assert!(event_types.contains(&&TaskEventType::Paused));
        assert!(event_types.contains(&&TaskEventType::Resumed));
    }

    #[test]
    fn test_task_execution_lifecycle() {
        let storage = create_test_storage();

        let task = storage
            .create_task(
                "Test Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        // Start execution
        let running = storage.start_task_execution(&task.id).unwrap();
        assert_eq!(running.status, AgentTaskStatus::Running);
        assert!(running.last_run_at.is_some());

        // Complete execution
        let completed = storage
            .complete_task_execution(&task.id, Some("Success output".to_string()), 1500)
            .unwrap();
        assert_eq!(completed.status, AgentTaskStatus::Active);
        assert_eq!(completed.success_count, 1);

        // Check events
        let events = storage.list_events_for_task(&task.id).unwrap();
        let event_types: Vec<_> = events.iter().map(|e| &e.event_type).collect();
        assert!(event_types.contains(&&TaskEventType::Started));
        assert!(event_types.contains(&&TaskEventType::Completed));
    }

    #[test]
    fn test_task_execution_failure() {
        let storage = create_test_storage();

        let task = storage
            .create_task(
                "Test Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        // Start and fail execution
        storage.start_task_execution(&task.id).unwrap();
        let failed = storage
            .fail_task_execution(&task.id, "Test error".to_string(), 500)
            .unwrap();

        assert_eq!(failed.status, AgentTaskStatus::Failed);
        assert_eq!(failed.failure_count, 1);
        assert_eq!(failed.last_error, Some("Test error".to_string()));

        // Check events
        let events = storage.list_events_for_task(&task.id).unwrap();
        let failed_events: Vec<_> = events
            .iter()
            .filter(|e| e.event_type == TaskEventType::Failed)
            .collect();
        assert_eq!(failed_events.len(), 1);
        assert_eq!(failed_events[0].message, Some("Test error".to_string()));
    }

    #[test]
    fn test_list_recent_events() {
        let storage = create_test_storage();

        let task = storage
            .create_task(
                "Test Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        // Add multiple events
        for i in 0..5 {
            let event = TaskEvent::new(task.id.clone(), TaskEventType::Started)
                .with_message(format!("Event {}", i));
            storage.add_event(&event).unwrap();
        }

        let recent = storage.list_recent_events_for_task(&task.id, 3).unwrap();
        assert_eq!(recent.len(), 3);
    }

    #[test]
    fn test_notification_events() {
        let storage = create_test_storage();

        let task = storage
            .create_task(
                "Test Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        // Record notification sent
        storage
            .record_notification_sent(&task.id, "Notification delivered".to_string())
            .unwrap();

        // Record notification failure
        storage
            .record_notification_failed(&task.id, "Network error".to_string())
            .unwrap();

        let events = storage.list_events_for_task(&task.id).unwrap();
        let notification_events: Vec<_> = events
            .iter()
            .filter(|e| {
                e.event_type == TaskEventType::NotificationSent
                    || e.event_type == TaskEventType::NotificationFailed
            })
            .collect();

        assert_eq!(notification_events.len(), 2);
    }

    #[test]
    fn test_list_runnable_tasks() {
        let storage = create_test_storage();

        // Create a task with a past run time
        let past_time = chrono::Utc::now().timestamp_millis() - 10000;
        let task1 = storage
            .create_task(
                "Ready Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Once { run_at: past_time },
            )
            .unwrap();

        // Manually fix the next_run_at to be in the past
        let mut task1_updated = task1;
        task1_updated.next_run_at = Some(past_time);
        storage.update_task(&task1_updated).unwrap();

        // Create a task with a future run time
        let future_time = chrono::Utc::now().timestamp_millis() + 3600000;
        storage
            .create_task(
                "Future Task".to_string(),
                "agent-002".to_string(),
                TaskSchedule::Once {
                    run_at: future_time,
                },
            )
            .unwrap();

        let current_time = chrono::Utc::now().timestamp_millis();
        let runnable = storage.list_runnable_tasks(current_time).unwrap();

        assert_eq!(runnable.len(), 1);
        assert_eq!(runnable[0].name, "Ready Task");
    }

    #[test]
    fn test_get_nonexistent_task() {
        let storage = create_test_storage();

        let result = storage.get_task("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_pause_nonexistent_task() {
        let storage = create_test_storage();

        let result = storage.pause_task("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
}
