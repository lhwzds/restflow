use super::*;

impl TaskStorage {
    // ============== Task Operations ==============

    /// Validate a task creation spec without creating records or sessions.
    pub fn validate_create_spec(spec: &TaskSpec) -> Result<()> {
        Self::validate_timeout_secs(spec.timeout_secs)?;
        Self::validate_task_input(spec.input.as_deref(), spec.input_template.as_deref())
    }

    /// Validate a task update patch against the current task without mutating storage.
    pub fn validate_update_patch_for_task(task: &Task, patch: &TaskPatch) -> Result<()> {
        Self::validate_timeout_secs(patch.timeout_secs)?;
        let input = patch.input.as_deref().or(task.input.as_deref());
        let input_template = patch
            .input_template
            .as_deref()
            .or(task.input_template.as_deref());
        Self::validate_task_input(input, input_template)
    }

    /// Create a task from a rich spec.
    pub fn create_task_from_spec(&self, spec: TaskSpec) -> Result<Task> {
        let session_binding = TaskSessionBinding {
            session_id: Self::normalize_optional_id(spec.chat_session_id.clone())
                .unwrap_or_default(),
            owns_session: false,
        };
        self.create_task_from_spec_with_binding(spec, session_binding)
    }

    /// Create a task after the caller has resolved its chat-session binding.
    pub fn create_task_from_spec_with_binding(
        &self,
        spec: TaskSpec,
        session_binding: TaskSessionBinding,
    ) -> Result<Task> {
        Self::validate_create_spec(&spec)?;
        let TaskSpec {
            name,
            agent_id,
            chat_session_id: _,
            description,
            input,
            input_template,
            schedule,
            execution_mode,
            timeout_secs,
            resource_limits,
            prerequisites,
            continuation,
        } = spec;

        let mut task = Task::new(Uuid::new_v4().to_string(), name, agent_id, schedule);

        task.chat_session_id = session_binding.session_id;
        task.owns_chat_session = session_binding.owns_session;
        task.description = description;
        task.input = input;
        task.input_template = input_template;
        if let Some(execution_mode) = execution_mode {
            task.execution_mode = execution_mode;
        }
        task.timeout_secs = timeout_secs;
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
        let task = self
            .get_task(id)?
            .ok_or_else(|| anyhow::anyhow!("Task {} not found", id))?;
        Self::validate_update_patch_for_task(&task, &patch)?;
        let session_binding =
            if let Some(session_id) = Self::normalize_optional_id(patch.chat_session_id.clone()) {
                TaskSessionBinding {
                    session_id,
                    owns_session: false,
                }
            } else {
                TaskSessionBinding {
                    session_id: task.chat_session_id.clone(),
                    owns_session: task.owns_chat_session,
                }
            };
        self.update_task_from_patch_with_binding(id, patch, session_binding)
    }

    /// Update a task after the caller has resolved its chat-session binding.
    pub fn update_task_from_patch_with_binding(
        &self,
        id: &str,
        patch: TaskPatch,
        session_binding: TaskSessionBinding,
    ) -> Result<Task> {
        let mut task = self
            .get_task(id)?
            .ok_or_else(|| anyhow::anyhow!("Task {} not found", id))?;
        Self::validate_update_patch_for_task(&task, &patch)?;
        let TaskPatch {
            name,
            description,
            agent_id,
            chat_session_id: _,
            input,
            input_template,
            schedule,
            execution_mode,
            timeout_secs,
            resource_limits,
            prerequisites,
            continuation,
        } = patch;

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
        if let Some(execution_mode) = execution_mode {
            task.execution_mode = execution_mode;
        }
        if let Some(timeout_secs) = timeout_secs {
            task.timeout_secs = Some(timeout_secs);
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
        let pending_message_count = self.list_pending_task_messages(id, usize::MAX)?.len() as u32;

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
