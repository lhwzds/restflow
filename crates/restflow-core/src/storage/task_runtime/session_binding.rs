use super::*;

impl TaskStorage {
    // ============== Task Operations ==============

    /// Create a task from a rich spec.
    pub fn create_task_from_spec(&self, spec: TaskSpec) -> Result<Task> {
        let TaskSpec {
            name,
            agent_id,
            chat_session_id,
            description,
            input,
            input_template,
            schedule,
            notification,
            execution_mode,
            timeout_secs,
            memory,
            durability_mode,
            resource_limits,
            prerequisites,
            continuation,
        } = spec;

        Self::validate_timeout_secs(timeout_secs)?;
        Self::validate_task_input(input.as_deref(), input_template.as_deref())?;
        let session_binding =
            self.resolve_chat_session_id_for_create(chat_session_id, &agent_id, &name)?;
        let mut task = Task::new(Uuid::new_v4().to_string(), name, agent_id, schedule);

        task.chat_session_id = session_binding.session_id;
        task.owns_chat_session = session_binding.owns_session;
        task.description = description;
        task.input = input;
        task.input_template = input_template;
        if let Some(notification) = notification {
            task.notification = notification;
        }
        if let Some(execution_mode) = execution_mode {
            task.execution_mode = execution_mode;
        }
        task.timeout_secs = timeout_secs;
        if let Some(memory) = memory {
            task.memory = memory;
        }
        if let Some(durability_mode) = durability_mode {
            task.durability_mode = durability_mode;
        }
        if let Some(resource_limits) = resource_limits {
            task.resource_limits = resource_limits;
        }
        task.prerequisites = prerequisites;
        if let Some(continuation) = continuation {
            task.continuation = continuation;
        }
        task.updated_at = chrono::Utc::now().timestamp_millis();

        self.save_task(&task)?;
        let event =
            TaskEvent::new(task.id.clone(), TaskEventType::Created).with_message("Task created");
        self.add_event(&event)?;
        Ok(task)
    }

    /// Update a task with a partial patch.
    pub fn update_task_from_patch(&self, id: &str, patch: TaskPatch) -> Result<Task> {
        let TaskPatch {
            name,
            description,
            agent_id,
            chat_session_id,
            input,
            input_template,
            schedule,
            notification,
            execution_mode,
            timeout_secs,
            memory,
            durability_mode,
            resource_limits,
            prerequisites,
            continuation,
        } = patch;
        Self::validate_timeout_secs(timeout_secs)?;
        let mut task = self
            .get_task(id)?
            .ok_or_else(|| anyhow::anyhow!("Task {} not found", id))?;

        let next_agent_id = agent_id.clone().unwrap_or_else(|| task.agent_id.clone());
        let session_binding =
            self.resolve_chat_session_id_for_update(&task, chat_session_id, &next_agent_id)?;

        if let Some(name) = name {
            task.name = name;
        }
        if let Some(description) = description {
            task.description = Some(description);
        }
        if let Some(agent_id) = agent_id {
            task.agent_id = agent_id;
        }
        task.chat_session_id = session_binding.session_id;
        task.owns_chat_session = session_binding.owns_session;
        if let Some(input) = input {
            task.input = Some(input);
        }
        if let Some(input_template) = input_template {
            task.input_template = Some(input_template);
        }
        if let Some(schedule) = schedule {
            task.schedule = schedule;
            task.update_next_run();
        }
        if let Some(notification) = notification {
            task.notification = notification;
        }
        if let Some(execution_mode) = execution_mode {
            task.execution_mode = execution_mode;
        }
        if let Some(timeout_secs) = timeout_secs {
            task.timeout_secs = Some(timeout_secs);
        }
        if let Some(memory) = memory {
            task.memory = memory;
        }
        if let Some(durability_mode) = durability_mode {
            task.durability_mode = durability_mode;
        }
        if let Some(resource_limits) = resource_limits {
            task.resource_limits = resource_limits;
        }
        if let Some(prerequisites) = prerequisites {
            task.prerequisites = prerequisites;
        }
        if let Some(continuation) = continuation {
            task.continuation = continuation;
            task.continuation_total_iterations = 0;
            task.continuation_segments_completed = 0;
        }
        Self::validate_task_input(task.input.as_deref(), task.input_template.as_deref())?;

        task.updated_at = chrono::Utc::now().timestamp_millis();
        self.update_task(&task)?;
        Ok(task)
    }

    /// Apply a control action to a task.
    pub fn control_task(&self, id: &str, action: TaskControlAction) -> Result<Task> {
        let mut task = self
            .get_task(id)?
            .ok_or_else(|| anyhow::anyhow!("Task {} not found", id))?;

        let now = chrono::Utc::now().timestamp_millis();
        let event = match action {
            TaskControlAction::Start => {
                task.status = TaskStatus::Active;
                task.next_run_at = Some(now);
                task.updated_at = now;
                TaskEvent::new(task.id.clone(), TaskEventType::Resumed)
                    .with_message("Background agent started")
            }
            TaskControlAction::Pause => {
                task.pause();
                TaskEvent::new(task.id.clone(), TaskEventType::Paused)
                    .with_message("Background agent paused")
            }
            TaskControlAction::Resume => {
                task.resume();
                TaskEvent::new(task.id.clone(), TaskEventType::Resumed)
                    .with_message("Background agent resumed")
            }
            TaskControlAction::Stop => {
                task.set_interrupted();
                TaskEvent::new(task.id.clone(), TaskEventType::Interrupted)
                    .with_message("Background agent stopped")
            }
            TaskControlAction::RunNow => {
                task.status = TaskStatus::Active;
                task.next_run_at = Some(now);
                task.updated_at = now;
                TaskEvent::new(task.id.clone(), TaskEventType::Resumed)
                    .with_message("Background agent scheduled for immediate run")
            }
        };

        self.update_task(&task)?;
        self.add_event(&event)?;
        Ok(task)
    }

    /// Get aggregated progress for a task.
    pub fn get_task_progress(&self, id: &str, event_limit: usize) -> Result<TaskProgress> {
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

        Ok(TaskProgress {
            task_id: task.id.clone(),
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
}
