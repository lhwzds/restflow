use super::*;

impl BackgroundAgentStorage {
    // ============== Background Agent Operations ==============

    /// Create a background agent from a rich spec.
    pub fn create_background_agent(&self, spec: BackgroundAgentSpec) -> Result<BackgroundAgent> {
        let BackgroundAgentSpec {
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
        let mut task = BackgroundAgent::new(Uuid::new_v4().to_string(), name, agent_id, schedule);

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
        let event = BackgroundAgentEvent::new(task.id.clone(), BackgroundAgentEventType::Created)
            .with_message("Task created");
        self.add_event(&event)?;
        Ok(task)
    }

    /// Update a background agent with a partial patch.
    pub fn update_background_agent(
        &self,
        id: &str,
        patch: BackgroundAgentPatch,
    ) -> Result<BackgroundAgent> {
        let BackgroundAgentPatch {
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
                task.set_interrupted();
                BackgroundAgentEvent::new(task.id.clone(), BackgroundAgentEventType::Interrupted)
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
}
