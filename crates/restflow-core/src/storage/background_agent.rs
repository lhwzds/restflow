//! Typed agent task storage wrapper.
//!
//! Provides type-safe access to agent task storage by wrapping the byte-level
//! APIs from restflow-storage with Rust types from our models.

use crate::models::{
    AgentCheckpoint, BackgroundAgent, BackgroundAgentControlAction, BackgroundAgentEvent,
    BackgroundAgentEventType, BackgroundAgentPatch, BackgroundAgentSchedule, BackgroundAgentSpec,
    BackgroundAgentStatus, BackgroundMessage, BackgroundMessageSource, BackgroundMessageStatus,
    BackgroundProgress,
};
use anyhow::Result;
use redb::Database;
use std::sync::Arc;
use tracing::warn;
use uuid::Uuid;

use super::CheckpointStorage;

/// Typed agent task storage wrapper around restflow-storage::BackgroundAgentStorage.
#[derive(Clone)]
pub struct BackgroundAgentStorage {
    inner: restflow_storage::BackgroundAgentStorage,
    checkpoints: CheckpointStorage,
}

impl BackgroundAgentStorage {
    const MIN_TASK_TIMEOUT_SECS: u64 = 10;

    fn has_non_empty_text(value: Option<&str>) -> bool {
        value.is_some_and(|text| !text.trim().is_empty())
    }

    fn validate_timeout_secs(timeout_secs: Option<u64>) -> Result<()> {
        if let Some(timeout) = timeout_secs
            && timeout < Self::MIN_TASK_TIMEOUT_SECS
        {
            return Err(anyhow::anyhow!(
                "timeout_secs must be at least {} seconds",
                Self::MIN_TASK_TIMEOUT_SECS
            ));
        }
        Ok(())
    }

    fn validate_task_input(input: Option<&str>, input_template: Option<&str>) -> Result<()> {
        if Self::resolve_effective_input_for_validation(input, input_template).is_some() {
            return Ok(());
        }
        Err(anyhow::anyhow!(
            "background agent requires non-empty input or input_template"
        ))
    }

    fn resolve_effective_input_for_validation(
        input: Option<&str>,
        input_template: Option<&str>,
    ) -> Option<String> {
        let fallback_input = input
            .filter(|value| Self::has_non_empty_text(Some(value)))
            .map(str::to_string);

        if let Some(template) = input_template {
            let rendered = Self::render_input_template_for_validation(template, input);
            if !rendered.trim().is_empty() {
                return Some(rendered);
            }
            return fallback_input;
        }

        fallback_input
    }

    fn render_input_template_for_validation(template: &str, input: Option<&str>) -> String {
        let input_value = input.unwrap_or_default();
        let replacements = std::collections::HashMap::from([
            ("{{task.input}}", input_value),
            ("{{input}}", input_value),
        ]);
        crate::utils::template::render_template_single_pass(template, &replacements)
    }

    /// Create a new BackgroundAgentStorage instance
    pub fn new(db: Arc<Database>) -> Result<Self> {
        let checkpoints = CheckpointStorage::new(db.clone())?;
        Ok(Self {
            inner: restflow_storage::BackgroundAgentStorage::new(db)?,
            checkpoints,
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

    /// List tasks filtered by agent ID.
    pub fn list_tasks_by_agent_id(&self, agent_id: &str) -> Result<Vec<BackgroundAgent>> {
        let tasks = self.list_tasks()?;
        Ok(tasks
            .into_iter()
            .filter(|task| task.agent_id == agent_id)
            .collect())
    }

    /// List non-terminal tasks filtered by agent ID.
    pub fn list_active_tasks_by_agent_id(&self, agent_id: &str) -> Result<Vec<BackgroundAgent>> {
        let tasks = self.list_tasks_by_agent_id(agent_id)?;
        Ok(tasks
            .into_iter()
            .filter(|task| {
                matches!(
                    task.status,
                    BackgroundAgentStatus::Active
                        | BackgroundAgentStatus::Paused
                        | BackgroundAgentStatus::Running
                        | BackgroundAgentStatus::Interrupted
                )
            })
            .collect())
    }

    /// List tasks that are ready to run
    pub fn list_runnable_tasks(&self, current_time: i64) -> Result<Vec<BackgroundAgent>> {
        let tasks = self.list_tasks()?;
        let mut runnable = Vec::new();

        for mut task in tasks {
            if task.status == BackgroundAgentStatus::Active {
                let needs_repair = if task.next_run_at.is_none() {
                    // Self-heal old tasks that have a cron/interval schedule but no
                    // computed next run time (e.g., created before cron normalization).
                    true
                } else if let (Some(next_run), Some(last_run)) =
                    (task.next_run_at, task.last_run_at)
                {
                    // Self-heal tasks where next_run_at is stale (before last_run_at).
                    // This can happen if the daemon was restarted mid-execution and
                    // the completion handler didn't persist the updated schedule.
                    next_run < last_run
                } else {
                    false
                };

                if needs_repair {
                    task.update_next_run();
                    if let Err(err) = self.update_task(&task) {
                        warn!(
                            "Failed to persist repaired next_run_at for task {}: {}",
                            task.id, err
                        );
                    }
                }
            }

            if task.should_run(current_time) {
                runnable.push(task);
            }
        }

        Ok(runnable)
    }

    /// Save an agent task (insert or replace).
    /// Unlike `update_task`, this does not require the task to already exist.
    pub fn save_task(&self, task: &BackgroundAgent) -> Result<()> {
        let json_bytes = serde_json::to_vec(task)?;
        self.inner
            .put_task_raw_with_status(&task.id, task.status.as_str(), &json_bytes)?;
        Ok(())
    }

    /// Update an existing agent task.
    /// Returns an error if the task does not exist.
    pub fn update_task(&self, task: &BackgroundAgent) -> Result<()> {
        let json_bytes = serde_json::to_vec(task)?;
        let previous_status = self
            .get_task(&task.id)?
            .map(|existing| existing.status)
            .ok_or_else(|| anyhow::anyhow!("Task {} not found", task.id))?;
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
        self.inner.delete_task_cascade(id)
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
        Self::validate_timeout_secs(spec.timeout_secs)?;
        Self::validate_task_input(spec.input.as_deref(), spec.input_template.as_deref())?;
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
        task.timeout_secs = spec.timeout_secs;
        if let Some(memory) = spec.memory {
            task.memory = memory;
        }
        if let Some(durability_mode) = spec.durability_mode {
            task.durability_mode = durability_mode;
        }
        if let Some(resource_limits) = spec.resource_limits {
            task.resource_limits = resource_limits;
        }
        task.prerequisites = spec.prerequisites;
        if let Some(continuation) = spec.continuation {
            task.continuation = continuation;
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
        Self::validate_timeout_secs(patch.timeout_secs)?;
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
        if let Some(timeout_secs) = patch.timeout_secs {
            task.timeout_secs = Some(timeout_secs);
        }
        if let Some(memory) = patch.memory {
            task.memory = memory;
        }
        if let Some(durability_mode) = patch.durability_mode {
            task.durability_mode = durability_mode;
        }
        if let Some(resource_limits) = patch.resource_limits {
            task.resource_limits = resource_limits;
        }
        if let Some(prerequisites) = patch.prerequisites {
            task.prerequisites = prerequisites;
        }
        if let Some(continuation) = patch.continuation {
            task.continuation = continuation;
            task.continuation_total_iterations = 0;
            task.continuation_segments_completed = 0;
        }
        Self::validate_task_input(task.input.as_deref(), task.input_template.as_deref())?;

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
            BackgroundAgentEventType::Interrupted => "interrupted",
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

    /// Delete old terminal tasks and their related messages/events.
    ///
    /// Returns the number of deleted tasks.
    pub fn cleanup_old_tasks(&self, older_than_ms: i64) -> Result<usize> {
        let tasks = self.list_tasks()?;
        let mut deleted = 0usize;

        for task in tasks {
            // Re-fetch current state before deleting to avoid race condition.
            // Between the initial list_tasks() snapshot and delete_task(),
            // another thread could have changed task status or timestamp.
            if let Some(current) = self.get_task(&task.id)? {
                // Verify status is still terminal (Completed or Failed)
                if !matches!(
                    current.status,
                    BackgroundAgentStatus::Completed | BackgroundAgentStatus::Failed
                ) {
                    continue;
                }

                // Verify timestamp is still old enough for deletion
                if current.updated_at >= older_than_ms {
                    continue;
                }
            } else {
                // Task was already deleted, skip
                continue;
            }

            if self.delete_task(&task.id)? {
                deleted += 1;
            }
        }

        Ok(deleted)
    }

    // ============== Checkpoint Operations ==============

    /// Save an agent checkpoint.
    pub fn save_checkpoint(&self, checkpoint: &AgentCheckpoint) -> Result<()> {
        self.checkpoints.save(checkpoint)
    }

    /// Save an agent checkpoint and return a persistent savepoint ID.
    pub fn save_checkpoint_with_savepoint(&self, checkpoint: &AgentCheckpoint) -> Result<u64> {
        self.checkpoints.save_with_savepoint(checkpoint)
    }

    /// Save a checkpoint with an already-obtained savepoint ID (atomic).
    pub fn save_checkpoint_with_savepoint_id(&self, checkpoint: &AgentCheckpoint) -> Result<()> {
        self.checkpoints.save_with_savepoint_id(checkpoint)
    }

    /// Load a checkpoint by task ID.
    pub fn load_checkpoint_by_task_id(&self, task_id: &str) -> Result<Option<AgentCheckpoint>> {
        self.checkpoints.load_by_task_id(task_id)
    }

    /// Delete expired checkpoints.
    pub fn cleanup_expired_checkpoints(&self) -> Result<usize> {
        self.checkpoints.cleanup_expired()
    }

    /// Delete a persistent savepoint if it exists.
    pub fn delete_checkpoint_savepoint(&self, savepoint_id: u64) -> Result<bool> {
        self.checkpoints.delete_savepoint(savepoint_id)
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
    fn test_list_tasks_by_agent_id() {
        let storage = create_test_storage();

        let task1 = storage
            .create_task(
                "Agent One Active".to_string(),
                "agent-001".to_string(),
                BackgroundAgentSchedule::default(),
            )
            .unwrap();
        let task2 = storage
            .create_task(
                "Agent One Paused".to_string(),
                "agent-001".to_string(),
                BackgroundAgentSchedule::default(),
            )
            .unwrap();
        let _task3 = storage
            .create_task(
                "Agent Two Active".to_string(),
                "agent-002".to_string(),
                BackgroundAgentSchedule::default(),
            )
            .unwrap();

        storage.pause_task(&task2.id).unwrap();

        let mut tasks = storage.list_tasks_by_agent_id("agent-001").unwrap();
        tasks.sort_by(|a, b| a.name.cmp(&b.name));

        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].id, task1.id);
        assert_eq!(tasks[1].id, task2.id);
    }

    #[test]
    fn test_list_active_tasks_by_agent_id() {
        let storage = create_test_storage();

        let active = storage
            .create_task(
                "Active".to_string(),
                "agent-001".to_string(),
                BackgroundAgentSchedule::default(),
            )
            .unwrap();
        let paused = storage
            .create_task(
                "Paused".to_string(),
                "agent-001".to_string(),
                BackgroundAgentSchedule::default(),
            )
            .unwrap();
        let completed = storage
            .create_task(
                "Completed".to_string(),
                "agent-001".to_string(),
                BackgroundAgentSchedule::Once {
                    run_at: chrono::Utc::now().timestamp_millis(),
                },
            )
            .unwrap();

        storage.pause_task(&paused.id).unwrap();
        storage.start_task_execution(&completed.id).unwrap();
        storage
            .complete_task_execution(&completed.id, Some("done".to_string()), 100)
            .unwrap();

        let mut tasks = storage.list_active_tasks_by_agent_id("agent-001").unwrap();
        tasks.sort_by(|a, b| a.name.cmp(&b.name));

        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].id, active.id);
        assert_eq!(tasks[1].id, paused.id);
    }

    #[test]
    fn test_cleanup_old_tasks_keeps_non_terminal() {
        let storage = create_test_storage();
        let now = chrono::Utc::now().timestamp_millis();

        let terminal = storage
            .create_task(
                "Terminal Task".to_string(),
                "agent-001".to_string(),
                BackgroundAgentSchedule::default(),
            )
            .unwrap();
        storage
            .fail_task_execution(&terminal.id, "failed".to_string(), 1)
            .unwrap();
        let mut terminal_updated = storage.get_task(&terminal.id).unwrap().unwrap();
        terminal_updated.updated_at = now - (10 * 24 * 60 * 60 * 1000);
        storage.update_task(&terminal_updated).unwrap();

        let mut active = storage
            .create_task(
                "Active Task".to_string(),
                "agent-001".to_string(),
                BackgroundAgentSchedule::default(),
            )
            .unwrap();
        active.updated_at = now - (30 * 24 * 60 * 60 * 1000);
        storage.update_task(&active).unwrap();

        let cutoff = now - (7 * 24 * 60 * 60 * 1000);
        let deleted = storage.cleanup_old_tasks(cutoff).unwrap();
        assert_eq!(deleted, 1);
        assert!(storage.get_task(&terminal.id).unwrap().is_none());
        assert!(storage.get_task(&active.id).unwrap().is_some());
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
        let bg_message = storage
            .send_background_agent_message(
                &task.id,
                "queued message".to_string(),
                BackgroundMessageSource::User,
            )
            .unwrap();
        assert_eq!(bg_message.status, BackgroundMessageStatus::Queued);

        // Delete the task
        let deleted = storage.delete_task(&task.id).unwrap();
        assert!(deleted);

        // Task should be gone
        let retrieved = storage.get_task(&task.id).unwrap();
        assert!(retrieved.is_none());

        // Events should also be gone
        let events = storage.list_events_for_task(&task.id).unwrap();
        assert!(events.is_empty());

        // Background messages should also be gone
        let messages = storage
            .list_background_agent_messages(&task.id, 10)
            .unwrap();
        assert!(messages.is_empty());
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
    fn test_list_runnable_tasks_repairs_missing_next_run_for_cron() {
        let storage = create_test_storage();

        let created = storage
            .create_background_agent(BackgroundAgentSpec {
                name: "Cron Task".to_string(),
                agent_id: "agent-001".to_string(),
                description: None,
                input: Some("hello".to_string()),
                input_template: None,
                schedule: BackgroundAgentSchedule::Cron {
                    expression: "* * * * *".to_string(),
                    timezone: None,
                },
                notification: None,
                execution_mode: None,
                timeout_secs: None,
                memory: None,
                durability_mode: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();

        // Simulate legacy data where next_run_at was not computed.
        let mut broken = created.clone();
        broken.next_run_at = None;
        storage.update_task(&broken).unwrap();

        let now = chrono::Utc::now().timestamp_millis();
        let _ = storage.list_runnable_tasks(now).unwrap();

        let repaired = storage.get_task(&created.id).unwrap().unwrap();
        assert!(repaired.next_run_at.is_some());
    }

    #[test]
    fn test_list_runnable_tasks_repairs_stale_next_run() {
        let storage = create_test_storage();

        let created = storage
            .create_background_agent(BackgroundAgentSpec {
                name: "Stale Task".to_string(),
                agent_id: "agent-001".to_string(),
                description: None,
                input: Some("hello".to_string()),
                input_template: None,
                schedule: BackgroundAgentSchedule::Interval {
                    interval_ms: 900_000,
                    start_at: None,
                },
                notification: None,
                execution_mode: None,
                timeout_secs: None,
                memory: None,
                durability_mode: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();

        // Simulate stale state: next_run_at is before last_run_at.
        // This happens when the daemon restarts mid-execution and
        // the completion handler doesn't persist the updated schedule.
        let now = chrono::Utc::now().timestamp_millis();
        let mut broken = created.clone();
        broken.next_run_at = Some(now - 3_600_000); // 1 hour ago
        broken.last_run_at = Some(now - 1_800_000); // 30 min ago (more recent)
        storage.update_task(&broken).unwrap();

        // Verify the stale condition
        let before = storage.get_task(&created.id).unwrap().unwrap();
        assert!(before.next_run_at.unwrap() < before.last_run_at.unwrap());

        // list_runnable_tasks should repair this
        let _ = storage.list_runnable_tasks(now).unwrap();

        let repaired = storage.get_task(&created.id).unwrap().unwrap();
        assert!(
            repaired.next_run_at.unwrap() > now,
            "next_run_at should be in the future after repair"
        );
    }

    #[test]
    fn test_get_nonexistent_task() {
        let storage = create_test_storage();

        let result = storage.get_task("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_update_nonexistent_task_returns_error() {
        use crate::models::TaskSchedule;
        let storage = create_test_storage();
        let task = BackgroundAgent::new(
            "nonexistent".to_string(),
            "Ghost".to_string(),
            "agent-000".to_string(),
            TaskSchedule::Once {
                run_at: chrono::Utc::now().timestamp_millis() + 60_000,
            },
        );
        let result = storage.update_task(&task);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
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
                timeout_secs: None,
                memory: None,
                durability_mode: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
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
                timeout_secs: None,
                memory: Some(MemoryConfig {
                    max_messages: 120,
                    enable_file_memory: true,
                    persist_on_complete: true,
                    memory_scope: MemoryScope::PerBackgroundAgent,
                    enable_compaction: true,
                    compaction_threshold_ratio: 0.80,
                    max_summary_tokens: 2_000,
                }),
                durability_mode: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
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
                input: Some("Fallback task input".to_string()),
                input_template: None,
                schedule: BackgroundAgentSchedule::default(),
                notification: None,
                execution_mode: None,
                timeout_secs: None,
                memory: None,
                durability_mode: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
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
                        enable_compaction: true,
                        compaction_threshold_ratio: 0.80,
                        max_summary_tokens: 2_000,
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

    #[test]
    fn test_create_background_agent_rejects_timeout_below_minimum() {
        let storage = create_test_storage();
        let result = storage.create_background_agent(BackgroundAgentSpec {
            name: "Too Fast Timeout".to_string(),
            agent_id: "agent-001".to_string(),
            description: None,
            input: Some("Run timeout validation".to_string()),
            input_template: None,
            schedule: BackgroundAgentSchedule::default(),
            notification: None,
            execution_mode: None,
            timeout_secs: Some(5),
            memory: None,
            durability_mode: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        });

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("timeout_secs must be at least")
        );
    }

    #[test]
    fn test_update_background_agent_updates_timeout_secs() {
        let storage = create_test_storage();
        let created = storage
            .create_background_agent(BackgroundAgentSpec {
                name: "Timeout Update Task".to_string(),
                agent_id: "agent-001".to_string(),
                description: None,
                input: Some("Run timeout update".to_string()),
                input_template: None,
                schedule: BackgroundAgentSchedule::default(),
                notification: None,
                execution_mode: None,
                timeout_secs: None,
                memory: None,
                durability_mode: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();

        let updated = storage
            .update_background_agent(
                &created.id,
                BackgroundAgentPatch {
                    timeout_secs: Some(900),
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(updated.timeout_secs, Some(900));
    }

    #[test]
    fn test_background_agent_resource_limits_roundtrip() {
        use crate::models::ResourceLimits;

        let storage = create_test_storage();
        let created = storage
            .create_background_agent(BackgroundAgentSpec {
                name: "Resource Limits Task".to_string(),
                agent_id: "agent-001".to_string(),
                description: None,
                input: Some("Run resource limit roundtrip".to_string()),
                input_template: None,
                schedule: BackgroundAgentSchedule::default(),
                notification: None,
                execution_mode: None,
                timeout_secs: None,
                memory: None,
                durability_mode: None,
                resource_limits: Some(ResourceLimits {
                    max_tool_calls: 12,
                    max_duration_secs: 90,
                    max_output_bytes: 2048,
                    max_cost_usd: Some(1.25),
                }),
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();

        assert_eq!(created.resource_limits.max_tool_calls, 12);
        assert_eq!(created.resource_limits.max_duration_secs, 90);
        assert_eq!(created.resource_limits.max_output_bytes, 2048);
        assert_eq!(created.resource_limits.max_cost_usd, Some(1.25));

        let updated = storage
            .update_background_agent(
                &created.id,
                BackgroundAgentPatch {
                    resource_limits: Some(ResourceLimits {
                        max_tool_calls: 34,
                        max_duration_secs: 120,
                        max_output_bytes: 4096,
                        max_cost_usd: Some(2.5),
                    }),
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(updated.resource_limits.max_tool_calls, 34);
        assert_eq!(updated.resource_limits.max_duration_secs, 120);
        assert_eq!(updated.resource_limits.max_output_bytes, 4096);
        assert_eq!(updated.resource_limits.max_cost_usd, Some(2.5));
    }

    #[test]
    fn test_background_agent_continuation_roundtrip() {
        use crate::models::ContinuationConfig;

        let storage = create_test_storage();
        let created = storage
            .create_background_agent(BackgroundAgentSpec {
                name: "Continuation Task".to_string(),
                agent_id: "agent-001".to_string(),
                description: None,
                input: Some("Run continuation roundtrip".to_string()),
                input_template: None,
                schedule: BackgroundAgentSchedule::default(),
                notification: None,
                execution_mode: None,
                timeout_secs: None,
                memory: None,
                durability_mode: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: Some(ContinuationConfig {
                    enabled: true,
                    segment_iterations: 40,
                    max_total_iterations: 800,
                    max_total_cost_usd: Some(4.5),
                    inter_segment_pause_ms: 250,
                }),
            })
            .unwrap();

        assert!(created.continuation.enabled);
        assert_eq!(created.continuation.segment_iterations, 40);
        assert_eq!(created.continuation.max_total_iterations, 800);
        assert_eq!(created.continuation.max_total_cost_usd, Some(4.5));
        assert_eq!(created.continuation.inter_segment_pause_ms, 250);
        assert_eq!(created.continuation_total_iterations, 0);
        assert_eq!(created.continuation_segments_completed, 0);

        let mut advanced = created.clone();
        advanced.continuation_total_iterations = 120;
        advanced.continuation_segments_completed = 3;
        storage.update_task(&advanced).unwrap();

        let updated = storage
            .update_background_agent(
                &created.id,
                BackgroundAgentPatch {
                    continuation: Some(ContinuationConfig {
                        enabled: true,
                        segment_iterations: 60,
                        max_total_iterations: 1_200,
                        max_total_cost_usd: Some(6.0),
                        inter_segment_pause_ms: 500,
                    }),
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(updated.continuation.segment_iterations, 60);
        assert_eq!(updated.continuation.max_total_iterations, 1_200);
        assert_eq!(updated.continuation.max_total_cost_usd, Some(6.0));
        assert_eq!(updated.continuation.inter_segment_pause_ms, 500);
        assert_eq!(updated.continuation_total_iterations, 0);
        assert_eq!(updated.continuation_segments_completed, 0);
    }

    #[test]
    fn test_create_background_agent_rejects_missing_input_and_template() {
        let storage = create_test_storage();
        let result = storage.create_background_agent(BackgroundAgentSpec {
            name: "Missing Input".to_string(),
            agent_id: "agent-001".to_string(),
            description: None,
            input: None,
            input_template: None,
            schedule: BackgroundAgentSchedule::default(),
            notification: None,
            execution_mode: None,
            timeout_secs: None,
            memory: None,
            durability_mode: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        });

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("requires non-empty input or input_template")
        );
    }

    #[test]
    fn test_update_background_agent_rejects_empty_input_and_template() {
        let storage = create_test_storage();
        let created = storage
            .create_background_agent(BackgroundAgentSpec {
                name: "Mutable Input".to_string(),
                agent_id: "agent-001".to_string(),
                description: None,
                input: Some("Initial input".to_string()),
                input_template: Some("Template {{task.name}}".to_string()),
                schedule: BackgroundAgentSchedule::default(),
                notification: None,
                execution_mode: None,
                timeout_secs: None,
                memory: None,
                durability_mode: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();

        let result = storage.update_background_agent(
            &created.id,
            BackgroundAgentPatch {
                input: Some("".to_string()),
                input_template: Some("   ".to_string()),
                ..Default::default()
            },
        );

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("requires non-empty input or input_template")
        );
    }

    #[test]
    fn test_create_background_agent_allows_empty_template_render_when_fallback_input_exists() {
        let storage = create_test_storage();
        let result = storage.create_background_agent(BackgroundAgentSpec {
            name: "Fallback Input".to_string(),
            agent_id: "agent-001".to_string(),
            description: None,
            input: Some("Use fallback".to_string()),
            input_template: Some("{{input}}".to_string()),
            schedule: BackgroundAgentSchedule::default(),
            notification: None,
            execution_mode: None,
            timeout_secs: None,
            memory: None,
            durability_mode: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        });

        assert!(result.is_ok());
    }

    #[test]
    fn test_create_background_agent_rejects_template_that_renders_empty_without_fallback() {
        let storage = create_test_storage();
        let result = storage.create_background_agent(BackgroundAgentSpec {
            name: "Empty Template".to_string(),
            agent_id: "agent-001".to_string(),
            description: None,
            input: None,
            input_template: Some("{{input}}".to_string()),
            schedule: BackgroundAgentSchedule::default(),
            notification: None,
            execution_mode: None,
            timeout_secs: None,
            memory: None,
            durability_mode: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        });

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("requires non-empty input or input_template")
        );
    }

    #[test]
    fn test_create_background_agent_keeps_non_empty_template_compatibility() {
        let storage = create_test_storage();
        let result = storage.create_background_agent(BackgroundAgentSpec {
            name: "Template Compatibility".to_string(),
            agent_id: "agent-001".to_string(),
            description: None,
            input: None,
            input_template: Some("Task {{task.name}}".to_string()),
            schedule: BackgroundAgentSchedule::default(),
            notification: None,
            execution_mode: None,
            timeout_secs: None,
            memory: None,
            durability_mode: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        });

        assert!(result.is_ok());
    }
}
