//! Typed agent task storage wrapper.
//!
//! Provides type-safe access to agent task storage by wrapping the byte-level
//! APIs from restflow-storage with Rust types from our models.

use crate::models::{
    BackgroundAgent, BackgroundAgentControlAction, BackgroundAgentEvent, BackgroundAgentEventType,
    BackgroundAgentPatch, BackgroundAgentSchedule, BackgroundAgentSpec, BackgroundAgentStatus,
    BackgroundMessage, BackgroundMessageSource, BackgroundMessageStatus, BackgroundProgress,
};
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
        schedule: BackgroundAgentSchedule,
    ) -> Result<BackgroundAgent> {
        let task = BackgroundAgent::new(Uuid::new_v4().to_string(), name, agent_id, schedule);

        let json_bytes = serde_json::to_vec(&task)?;
        self.inner
            .put_task_raw_with_status(&task.id, task.status.as_str(), &json_bytes)?;

        // Create a "created" event
        let event = BackgroundAgentEvent::new(task.id.clone(), BackgroundAgentEventType::Created)
            .with_message("Task created");
        self.add_event(&event)?;

        Ok(task)
    }

    /// Get an agent task by ID
    pub fn get_task(&self, id: &str) -> Result<Option<BackgroundAgent>> {
        if let Some(bytes) = self.inner.get_task_raw(id)? {
            let task: BackgroundAgent = serde_json::from_slice(&bytes)?;
            Ok(Some(task))
        } else {
            Ok(None)
        }
    }

    /// List all agent tasks
    pub fn list_tasks(&self) -> Result<Vec<BackgroundAgent>> {
        let tasks = self.inner.list_tasks_raw()?;
        let mut result = Vec::new();
        for (_, bytes) in tasks {
            let task: BackgroundAgent = serde_json::from_slice(&bytes)?;
            result.push(task);
        }
        Ok(result)
    }

    /// List tasks filtered by status
    pub fn list_tasks_by_status(
        &self,
        status: BackgroundAgentStatus,
    ) -> Result<Vec<BackgroundAgent>> {
        let tasks = self.inner.list_tasks_by_status_indexed(status.as_str())?;

        if tasks.is_empty() {
            let tasks = self.list_tasks()?;
            return Ok(tasks
                .into_iter()
                .filter(|task| task.status == status)
                .collect());
        }

        let mut result = Vec::new();
        for (_, bytes) in tasks {
            let task: BackgroundAgent = serde_json::from_slice(&bytes)?;
            result.push(task);
        }
        Ok(result)
    }

    /// List tasks that are ready to run
    pub fn list_runnable_tasks(&self, current_time: i64) -> Result<Vec<BackgroundAgent>> {
        let tasks = self.list_tasks()?;
        Ok(tasks
            .into_iter()
            .filter(|t| t.should_run(current_time))
            .collect())
    }

    /// Update an existing agent task
    pub fn update_task(&self, task: &BackgroundAgent) -> Result<()> {
        let json_bytes = serde_json::to_vec(task)?;
        let previous_status = self
            .get_task(&task.id)?
            .map(|existing| existing.status)
            .unwrap_or_else(|| task.status.clone());
        self.inner.update_task_raw_with_status(
            &task.id,
            previous_status.as_str(),
            task.status.as_str(),
            &json_bytes,
        )?;
        Ok(())
    }

    /// Delete an agent task and all its events
    pub fn delete_task(&self, id: &str) -> Result<bool> {
        let task = match self.get_task(id)? {
            Some(task) => task,
            None => return Ok(false),
        };

        // First delete all queued background messages for this task
        self.inner.delete_background_messages_for_task(id)?;
        // First delete all events for this task
        self.inner.delete_events_for_task(id)?;
        // Then delete the task itself with status index cleanup
        self.inner.delete_task_with_status(id, task.status.as_str())
    }

    /// Pause an agent task
    pub fn pause_task(&self, id: &str) -> Result<BackgroundAgent> {
        let mut task = self
            .get_task(id)?
            .ok_or_else(|| anyhow::anyhow!("Task {} not found", id))?;

        task.pause();
        self.update_task(&task)?;

        // Record the pause event
        let event = BackgroundAgentEvent::new(task.id.clone(), BackgroundAgentEventType::Paused)
            .with_message("Task paused");
        self.add_event(&event)?;

        Ok(task)
    }

    /// Resume an agent task
    pub fn resume_task(&self, id: &str) -> Result<BackgroundAgent> {
        let mut task = self
            .get_task(id)?
            .ok_or_else(|| anyhow::anyhow!("Task {} not found", id))?;

        task.resume();
        self.update_task(&task)?;

        // Record the resume event
        let event = BackgroundAgentEvent::new(task.id.clone(), BackgroundAgentEventType::Resumed)
            .with_message("Task resumed");
        self.add_event(&event)?;

        Ok(task)
    }

    /// Mark a task as running
    pub fn start_task_execution(&self, id: &str) -> Result<BackgroundAgent> {
        let mut task = self
            .get_task(id)?
            .ok_or_else(|| anyhow::anyhow!("Task {} not found", id))?;

        if task.status != BackgroundAgentStatus::Active {
            return Err(anyhow::anyhow!(
                "Task {} cannot start from status {}",
                id,
                task.status.as_str()
            ));
        }

        task.set_running();
        self.update_task(&task)?;

        // Record the start event
        let event = BackgroundAgentEvent::new(task.id.clone(), BackgroundAgentEventType::Started)
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
    ) -> Result<BackgroundAgent> {
        let mut task = self
            .get_task(id)?
            .ok_or_else(|| anyhow::anyhow!("Task {} not found", id))?;

        task.set_completed();
        self.update_task(&task)?;

        // Record the completion event
        let mut event =
            BackgroundAgentEvent::new(task.id.clone(), BackgroundAgentEventType::Completed)
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
    ) -> Result<BackgroundAgent> {
        let mut task = self
            .get_task(id)?
            .ok_or_else(|| anyhow::anyhow!("Task {} not found", id))?;

        task.set_failed(error.clone());
        self.update_task(&task)?;

        // Record the failure event
        let event = BackgroundAgentEvent::new(task.id.clone(), BackgroundAgentEventType::Failed)
            .with_message(error)
            .with_duration(duration_ms);
        self.add_event(&event)?;

        Ok(task)
    }

    // ============== Background Agent Operations ==============

    /// Create a background agent from a rich spec.
    pub fn create_background_agent(&self, spec: BackgroundAgentSpec) -> Result<BackgroundAgent> {
        let mut task = self.create_task(spec.name, spec.agent_id, spec.schedule)?;

        task.description = spec.description;
        task.input = spec.input;
        task.input_template = spec.input_template;
        if let Some(notification) = spec.notification {
            task.notification = notification;
        }
        if let Some(execution_mode) = spec.execution_mode {
            task.execution_mode = execution_mode;
        }
        if let Some(memory) = spec.memory {
            task.memory = memory;
        }
        task.updated_at = chrono::Utc::now().timestamp_millis();
        self.update_task(&task)?;
        Ok(task)
    }

    /// Update a background agent with a partial patch.
    pub fn update_background_agent(
        &self,
        id: &str,
        patch: BackgroundAgentPatch,
    ) -> Result<BackgroundAgent> {
        let mut task = self
            .get_task(id)?
            .ok_or_else(|| anyhow::anyhow!("Task {} not found", id))?;

        if let Some(name) = patch.name {
            task.name = name;
        }
        if let Some(description) = patch.description {
            task.description = Some(description);
        }
        if let Some(agent_id) = patch.agent_id {
            task.agent_id = agent_id;
        }
        if let Some(input) = patch.input {
            task.input = Some(input);
        }
        if let Some(input_template) = patch.input_template {
            task.input_template = Some(input_template);
        }
        if let Some(schedule) = patch.schedule {
            task.schedule = schedule;
            task.update_next_run();
        }
        if let Some(notification) = patch.notification {
            task.notification = notification;
        }
        if let Some(execution_mode) = patch.execution_mode {
            task.execution_mode = execution_mode;
        }
        if let Some(memory) = patch.memory {
            task.memory = memory;
        }

        task.updated_at = chrono::Utc::now().timestamp_millis();
        self.update_task(&task)?;
        Ok(task)
    }

    /// Apply a control action to a background agent.
    pub fn control_background_agent(
        &self,
        id: &str,
        action: BackgroundAgentControlAction,
    ) -> Result<BackgroundAgent> {
        let mut task = self
            .get_task(id)?
            .ok_or_else(|| anyhow::anyhow!("Task {} not found", id))?;

        let now = chrono::Utc::now().timestamp_millis();
        let event = match action {
            BackgroundAgentControlAction::Start => {
                task.status = BackgroundAgentStatus::Active;
                task.next_run_at = Some(now);
                task.updated_at = now;
                BackgroundAgentEvent::new(task.id.clone(), BackgroundAgentEventType::Resumed)
                    .with_message("Background agent started")
            }
            BackgroundAgentControlAction::Pause => {
                task.pause();
                BackgroundAgentEvent::new(task.id.clone(), BackgroundAgentEventType::Paused)
                    .with_message("Background agent paused")
            }
            BackgroundAgentControlAction::Resume => {
                task.resume();
                BackgroundAgentEvent::new(task.id.clone(), BackgroundAgentEventType::Resumed)
                    .with_message("Background agent resumed")
            }
            BackgroundAgentControlAction::Stop => {
                task.pause();
                BackgroundAgentEvent::new(task.id.clone(), BackgroundAgentEventType::Paused)
                    .with_message("Background agent stopped")
            }
            BackgroundAgentControlAction::RunNow => {
                task.status = BackgroundAgentStatus::Active;
                task.next_run_at = Some(now);
                task.updated_at = now;
                BackgroundAgentEvent::new(task.id.clone(), BackgroundAgentEventType::Resumed)
                    .with_message("Background agent scheduled for immediate run")
            }
        };

        self.update_task(&task)?;
        self.add_event(&event)?;
        Ok(task)
    }

    /// Get aggregated progress for a background agent.
    pub fn get_background_agent_progress(
        &self,
        id: &str,
        event_limit: usize,
    ) -> Result<BackgroundProgress> {
        let task = self
            .get_task(id)?
            .ok_or_else(|| anyhow::anyhow!("Task {} not found", id))?;
        let recent_events = self.list_recent_events_for_task(id, event_limit.max(1))?;
        let recent_event = recent_events.first().cloned();
        let stage = recent_event
            .as_ref()
            .map(|event| Self::event_stage_label(&event.event_type));
        let pending_message_count =
            self.list_pending_background_messages(id, usize::MAX)?.len() as u32;

        Ok(BackgroundProgress {
            background_agent_id: task.id.clone(),
            status: task.status,
            stage,
            recent_event,
            recent_events,
            last_run_at: task.last_run_at,
            next_run_at: task.next_run_at,
            total_tokens_used: task.total_tokens_used,
            total_cost_usd: task.total_cost_usd,
            success_count: task.success_count,
            failure_count: task.failure_count,
            pending_message_count,
        })
    }

    // ============== Background Message Operations ==============

    /// Queue a message for a background agent.
    pub fn send_background_agent_message(
        &self,
        background_agent_id: &str,
        message: String,
        source: BackgroundMessageSource,
    ) -> Result<BackgroundMessage> {
        if self.get_task(background_agent_id)?.is_none() {
            return Err(anyhow::anyhow!("Task {} not found", background_agent_id));
        }

        let bg_message = BackgroundMessage::new(background_agent_id.to_string(), source, message);
        self.persist_background_message(&bg_message, None)?;
        Ok(bg_message)
    }

    /// Get a background message by ID.
    pub fn get_background_message(&self, message_id: &str) -> Result<Option<BackgroundMessage>> {
        if let Some(bytes) = self.inner.get_background_message_raw(message_id)? {
            let message: BackgroundMessage = serde_json::from_slice(&bytes)?;
            Ok(Some(message))
        } else {
            Ok(None)
        }
    }

    /// List all background messages for an agent, sorted by timestamp descending.
    pub fn list_background_agent_messages(
        &self,
        background_agent_id: &str,
        limit: usize,
    ) -> Result<Vec<BackgroundMessage>> {
        let raw = self
            .inner
            .list_background_messages_for_task_raw(background_agent_id)?;
        let mut result = Vec::new();
        for (_, bytes) in raw {
            let message: BackgroundMessage = serde_json::from_slice(&bytes)?;
            result.push(message);
        }
        result.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(result.into_iter().take(limit).collect())
    }

    /// List queued messages waiting for delivery.
    pub fn list_pending_background_messages(
        &self,
        background_agent_id: &str,
        limit: usize,
    ) -> Result<Vec<BackgroundMessage>> {
        let raw = self.inner.list_background_messages_by_status_for_task_raw(
            background_agent_id,
            BackgroundMessageStatus::Queued.as_str(),
        )?;
        let mut result = Vec::new();
        for (_, bytes) in raw {
            let message: BackgroundMessage = serde_json::from_slice(&bytes)?;
            result.push(message);
        }
        result.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(result.into_iter().take(limit).collect())
    }

    /// Mark a queued message as delivered.
    pub fn mark_background_message_delivered(
        &self,
        message_id: &str,
    ) -> Result<Option<BackgroundMessage>> {
        let mut message = match self.get_background_message(message_id)? {
            Some(message) => message,
            None => return Ok(None),
        };
        let previous_status = message.status.clone();
        message.mark_delivered();
        self.persist_background_message(&message, Some(previous_status))?;
        Ok(Some(message))
    }

    /// Mark a delivered message as consumed.
    pub fn mark_background_message_consumed(
        &self,
        message_id: &str,
    ) -> Result<Option<BackgroundMessage>> {
        let mut message = match self.get_background_message(message_id)? {
            Some(message) => message,
            None => return Ok(None),
        };
        let previous_status = message.status.clone();
        message.mark_consumed();
        self.persist_background_message(&message, Some(previous_status))?;
        Ok(Some(message))
    }

    /// Mark a message as failed with an error.
    pub fn mark_background_message_failed(
        &self,
        message_id: &str,
        error: String,
    ) -> Result<Option<BackgroundMessage>> {
        let mut message = match self.get_background_message(message_id)? {
            Some(message) => message,
            None => return Ok(None),
        };
        let previous_status = message.status.clone();
        message.mark_failed(error);
        self.persist_background_message(&message, Some(previous_status))?;
        Ok(Some(message))
    }

    fn persist_background_message(
        &self,
        message: &BackgroundMessage,
        previous_status: Option<BackgroundMessageStatus>,
    ) -> Result<()> {
        let json_bytes = serde_json::to_vec(message)?;
        if let Some(previous_status) = previous_status {
            self.inner.update_background_message_raw_with_status(
                &message.id,
                &message.background_agent_id,
                previous_status.as_str(),
                message.status.as_str(),
                &json_bytes,
            )?;
        } else {
            self.inner.put_background_message_raw_with_status(
                &message.id,
                &message.background_agent_id,
                message.status.as_str(),
                &json_bytes,
            )?;
        }
        Ok(())
    }

    fn event_stage_label(event_type: &BackgroundAgentEventType) -> String {
        match event_type {
            BackgroundAgentEventType::Created => "created",
            BackgroundAgentEventType::Started => "running",
            BackgroundAgentEventType::Completed => "completed",
            BackgroundAgentEventType::Failed => "failed",
            BackgroundAgentEventType::Paused => "paused",
            BackgroundAgentEventType::Resumed => "active",
            BackgroundAgentEventType::NotificationSent => "notification_sent",
            BackgroundAgentEventType::NotificationFailed => "notification_failed",
            BackgroundAgentEventType::Compaction => "compaction",
        }
        .to_string()
    }

    // ============== Task Event Operations ==============

    /// Add a new event for a task
    pub fn add_event(&self, event: &BackgroundAgentEvent) -> Result<()> {
        let json_bytes = serde_json::to_vec(event)?;
        self.inner
            .put_event_raw(&event.id, &event.task_id, &json_bytes)?;
        Ok(())
    }

    /// Get an event by ID
    pub fn get_event(&self, event_id: &str) -> Result<Option<BackgroundAgentEvent>> {
        if let Some(bytes) = self.inner.get_event_raw(event_id)? {
            let event: BackgroundAgentEvent = serde_json::from_slice(&bytes)?;
            Ok(Some(event))
        } else {
            Ok(None)
        }
    }

    /// List all events for a task
    pub fn list_events_for_task(&self, task_id: &str) -> Result<Vec<BackgroundAgentEvent>> {
        let events = self.inner.list_events_for_task_raw(task_id)?;
        let mut result = Vec::new();
        for (_, bytes) in events {
            let event: BackgroundAgentEvent = serde_json::from_slice(&bytes)?;
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
    ) -> Result<Vec<BackgroundAgentEvent>> {
        let events = self.list_events_for_task(task_id)?;
        Ok(events.into_iter().take(limit).collect())
    }

    /// Record a notification event
    pub fn record_notification_sent(&self, task_id: &str, message: String) -> Result<()> {
        let event = BackgroundAgentEvent::new(
            task_id.to_string(),
            BackgroundAgentEventType::NotificationSent,
        )
        .with_message(message);
        self.add_event(&event)
    }

    /// Record a notification failure event
    pub fn record_notification_failed(&self, task_id: &str, error: String) -> Result<()> {
        let event = BackgroundAgentEvent::new(
            task_id.to_string(),
            BackgroundAgentEventType::NotificationFailed,
        )
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
                BackgroundAgentSchedule::Interval {
                    interval_ms: 3600000,
                    start_at: None,
                },
            )
            .unwrap();

        assert!(!task.id.is_empty());
        assert_eq!(task.name, "Test Task");
        assert_eq!(task.agent_id, "agent-001");
        assert_eq!(task.status, BackgroundAgentStatus::Active);

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
                BackgroundAgentSchedule::default(),
            )
            .unwrap();
        storage
            .create_task(
                "Task 2".to_string(),
                "agent-002".to_string(),
                BackgroundAgentSchedule::default(),
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
                BackgroundAgentSchedule::default(),
            )
            .unwrap();

        let task2 = storage
            .create_task(
                "Will be Paused".to_string(),
                "agent-002".to_string(),
                BackgroundAgentSchedule::default(),
            )
            .unwrap();

        storage.pause_task(&task2.id).unwrap();

        let active_tasks = storage
            .list_tasks_by_status(BackgroundAgentStatus::Active)
            .unwrap();
        let paused_tasks = storage
            .list_tasks_by_status(BackgroundAgentStatus::Paused)
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
                BackgroundAgentSchedule::default(),
            )
            .unwrap();

        // Add some events
        let event = BackgroundAgentEvent::new(task.id.clone(), BackgroundAgentEventType::Started);
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
                BackgroundAgentSchedule::default(),
            )
            .unwrap();

        // Pause the task
        let paused = storage.pause_task(&task.id).unwrap();
        assert_eq!(paused.status, BackgroundAgentStatus::Paused);

        // Resume the task
        let resumed = storage.resume_task(&task.id).unwrap();
        assert_eq!(resumed.status, BackgroundAgentStatus::Active);

        // Check events were recorded
        let events = storage.list_events_for_task(&task.id).unwrap();
        let event_types: Vec<_> = events.iter().map(|e| &e.event_type).collect();
        assert!(event_types.contains(&&BackgroundAgentEventType::Paused));
        assert!(event_types.contains(&&BackgroundAgentEventType::Resumed));
    }

    #[test]
    fn test_task_execution_lifecycle() {
        let storage = create_test_storage();

        let task = storage
            .create_task(
                "Test Task".to_string(),
                "agent-001".to_string(),
                BackgroundAgentSchedule::default(),
            )
            .unwrap();

        // Start execution
        let running = storage.start_task_execution(&task.id).unwrap();
        assert_eq!(running.status, BackgroundAgentStatus::Running);
        assert!(running.last_run_at.is_some());

        // Complete execution
        let completed = storage
            .complete_task_execution(&task.id, Some("Success output".to_string()), 1500)
            .unwrap();
        assert_eq!(completed.status, BackgroundAgentStatus::Active);
        assert_eq!(completed.success_count, 1);

        // Check events
        let events = storage.list_events_for_task(&task.id).unwrap();
        let event_types: Vec<_> = events.iter().map(|e| &e.event_type).collect();
        assert!(event_types.contains(&&BackgroundAgentEventType::Started));
        assert!(event_types.contains(&&BackgroundAgentEventType::Completed));
    }

    #[test]
    fn test_task_execution_failure() {
        let storage = create_test_storage();

        let task = storage
            .create_task(
                "Test Task".to_string(),
                "agent-001".to_string(),
                BackgroundAgentSchedule::default(),
            )
            .unwrap();

        // Start and fail execution
        storage.start_task_execution(&task.id).unwrap();
        let failed = storage
            .fail_task_execution(&task.id, "Test error".to_string(), 500)
            .unwrap();

        assert_eq!(failed.status, BackgroundAgentStatus::Failed);
        assert_eq!(failed.failure_count, 1);
        assert_eq!(failed.last_error, Some("Test error".to_string()));

        // Check events
        let events = storage.list_events_for_task(&task.id).unwrap();
        let failed_events: Vec<_> = events
            .iter()
            .filter(|e| e.event_type == BackgroundAgentEventType::Failed)
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
                BackgroundAgentSchedule::default(),
            )
            .unwrap();

        // Add multiple events
        for i in 0..5 {
            let event =
                BackgroundAgentEvent::new(task.id.clone(), BackgroundAgentEventType::Started)
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
                BackgroundAgentSchedule::default(),
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
                e.event_type == BackgroundAgentEventType::NotificationSent
                    || e.event_type == BackgroundAgentEventType::NotificationFailed
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
                BackgroundAgentSchedule::Once { run_at: past_time },
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
                BackgroundAgentSchedule::Once {
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

    #[test]
    fn test_background_agent_lifecycle() {
        let storage = create_test_storage();

        let created = storage
            .create_background_agent(BackgroundAgentSpec {
                name: "BG Agent".to_string(),
                agent_id: "agent-001".to_string(),
                description: Some("Background agent".to_string()),
                input: Some("Run checks".to_string()),
                input_template: None,
                schedule: BackgroundAgentSchedule::Interval {
                    interval_ms: 60_000,
                    start_at: None,
                },
                notification: None,
                execution_mode: None,
                memory: None,
            })
            .unwrap();
        assert_eq!(created.name, "BG Agent");

        let updated = storage
            .update_background_agent(
                &created.id,
                BackgroundAgentPatch {
                    name: Some("BG Agent Updated".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(updated.name, "BG Agent Updated");

        let paused = storage
            .control_background_agent(&created.id, BackgroundAgentControlAction::Pause)
            .unwrap();
        assert_eq!(paused.status, BackgroundAgentStatus::Paused);

        let resumed = storage
            .control_background_agent(&created.id, BackgroundAgentControlAction::Resume)
            .unwrap();
        assert_eq!(resumed.status, BackgroundAgentStatus::Active);

        let run_now = storage
            .control_background_agent(&created.id, BackgroundAgentControlAction::RunNow)
            .unwrap();
        assert_eq!(run_now.status, BackgroundAgentStatus::Active);
        assert!(run_now.next_run_at.is_some());
    }

    #[test]
    fn test_background_message_queue_and_progress() {
        let storage = create_test_storage();
        let task = storage
            .create_task(
                "Message Task".to_string(),
                "agent-001".to_string(),
                BackgroundAgentSchedule::default(),
            )
            .unwrap();

        let queued = storage
            .send_background_agent_message(
                &task.id,
                "Please also verify logs".to_string(),
                BackgroundMessageSource::User,
            )
            .unwrap();
        assert_eq!(queued.status, BackgroundMessageStatus::Queued);

        let pending = storage
            .list_pending_background_messages(&task.id, 10)
            .unwrap();
        assert_eq!(pending.len(), 1);

        let delivered = storage
            .mark_background_message_delivered(&queued.id)
            .unwrap()
            .unwrap();
        assert_eq!(delivered.status, BackgroundMessageStatus::Delivered);

        let consumed = storage
            .mark_background_message_consumed(&queued.id)
            .unwrap()
            .unwrap();
        assert_eq!(consumed.status, BackgroundMessageStatus::Consumed);

        let progress = storage.get_background_agent_progress(&task.id, 5).unwrap();
        assert_eq!(progress.background_agent_id, task.id);
        assert_eq!(progress.pending_message_count, 0);
    }

    #[test]
    fn test_create_background_agent_with_template_and_memory_scope() {
        use crate::models::{MemoryConfig, MemoryScope};

        let storage = create_test_storage();
        let created = storage
            .create_background_agent(BackgroundAgentSpec {
                name: "Templated Task".to_string(),
                agent_id: "agent-001".to_string(),
                description: None,
                input: Some("fallback".to_string()),
                input_template: Some("Run task {{task.id}}".to_string()),
                schedule: BackgroundAgentSchedule::default(),
                notification: None,
                execution_mode: None,
                memory: Some(MemoryConfig {
                    max_messages: 120,
                    enable_file_memory: true,
                    persist_on_complete: true,
                    memory_scope: MemoryScope::PerBackgroundAgent,
                }),
            })
            .unwrap();

        assert_eq!(
            created.input_template.as_deref(),
            Some("Run task {{task.id}}")
        );
        assert_eq!(created.memory.memory_scope, MemoryScope::PerBackgroundAgent);
    }

    #[test]
    fn test_update_background_agent_updates_template_and_memory_scope() {
        use crate::models::{MemoryConfig, MemoryScope};

        let storage = create_test_storage();
        let created = storage
            .create_background_agent(BackgroundAgentSpec {
                name: "Updatable Task".to_string(),
                agent_id: "agent-001".to_string(),
                description: None,
                input: None,
                input_template: None,
                schedule: BackgroundAgentSchedule::default(),
                notification: None,
                execution_mode: None,
                memory: None,
            })
            .unwrap();

        let updated = storage
            .update_background_agent(
                &created.id,
                BackgroundAgentPatch {
                    input_template: Some("Template {{task.name}}".to_string()),
                    memory: Some(MemoryConfig {
                        max_messages: 80,
                        enable_file_memory: false,
                        persist_on_complete: true,
                        memory_scope: MemoryScope::PerBackgroundAgent,
                    }),
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(
            updated.input_template.as_deref(),
            Some("Template {{task.name}}")
        );
        assert_eq!(updated.memory.memory_scope, MemoryScope::PerBackgroundAgent);
    }
}
