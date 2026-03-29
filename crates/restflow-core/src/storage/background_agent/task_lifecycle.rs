use super::*;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ResolveTaskIdError {
    #[error("Task not found: {0}")]
    NotFound(String),
    #[error("Task ID prefix '{prefix}' is ambiguous. Candidates: {preview}")]
    Ambiguous { prefix: String, preview: String },
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl BackgroundAgentStorage {
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

    /// Resolve a task ID or short prefix to the full task ID.
    ///
    /// This method is designed for user-facing entry points where users may
    /// provide a short ID prefix (e.g., "9f275c7a") instead of the full UUID.
    ///
    /// # Behavior
    ///
    /// - If `id_or_prefix` matches an exact task ID, returns that ID.
    /// - Otherwise, searches for tasks whose ID starts with `id_or_prefix`.
    /// - If exactly one match is found, returns the full ID.
    /// - If no matches are found, returns an error "Task not found".
    /// - If multiple matches are found, returns an error with candidate IDs.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let full_id = storage.resolve_existing_task_id("9f275c7a")?;
    /// let task = storage.get_task(&full_id)?.unwrap();
    /// ```
    pub fn resolve_existing_task_id(&self, id_or_prefix: &str) -> Result<String> {
        self.resolve_existing_task_id_typed(id_or_prefix)
            .map_err(anyhow::Error::from)
    }

    pub fn resolve_existing_task_id_typed(
        &self,
        id_or_prefix: &str,
    ) -> std::result::Result<String, ResolveTaskIdError> {
        // First, try exact match (most common case)
        if self.get_task(id_or_prefix)?.is_some() {
            return Ok(id_or_prefix.to_string());
        }

        // Search for prefix matches
        let candidates: Vec<String> = self
            .list_tasks()?
            .into_iter()
            .filter(|task| task.id.starts_with(id_or_prefix))
            .map(|task| task.id)
            .collect();

        match candidates.len() {
            0 => Err(ResolveTaskIdError::NotFound(id_or_prefix.to_string())),
            1 => Ok(candidates.into_iter().next().unwrap()),
            _ => {
                let preview: Vec<String> = candidates
                    .iter()
                    .take(5)
                    .map(|id| {
                        // Show first 8 chars of each ID for readability
                        if id.len() > 8 {
                            format!("{}...", &id[..8])
                        } else {
                            id.clone()
                        }
                    })
                    .collect();
                Err(ResolveTaskIdError::Ambiguous {
                    prefix: id_or_prefix.to_string(),
                    preview: preview.join(", "),
                })
            }
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
        let indexed = self.inner.list_tasks_by_status_indexed(status.as_str())?;
        let mut result = Vec::new();
        let mut indexed_ids = HashSet::new();
        for (_, bytes) in indexed {
            let task: BackgroundAgent = serde_json::from_slice(&bytes)?;
            if task.status == status {
                indexed_ids.insert(task.id.clone());
                result.push(task);
            }
        }

        // Reconcile with a full scan to recover from partial status index drift.
        for task in self.list_tasks()? {
            if task.status == status && !indexed_ids.contains(&task.id) {
                result.push(task);
            }
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

    /// List tasks bound to the specified chat session.
    pub fn list_tasks_by_chat_session_id(&self, session_id: &str) -> Result<Vec<BackgroundAgent>> {
        let target = session_id.trim();
        if target.is_empty() {
            return Ok(Vec::new());
        }

        let tasks = self.list_tasks()?;
        Ok(tasks
            .into_iter()
            .filter(|task| task.chat_session_id.trim() == target)
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
                    BackgroundAgentStatus::Paused
                        | BackgroundAgentStatus::Running
                        | BackgroundAgentStatus::Interrupted
                ) || task.is_active()
            })
            .collect())
    }

    /// List tasks that are ready to run
    pub fn list_runnable_tasks(&self, current_time: i64) -> Result<Vec<BackgroundAgent>> {
        let mut runnable = Vec::new();
        let tasks = self.list_tasks()?;

        for task in tasks {
            let Some(task) = self.repair_runnable_task_if_needed(task)? else {
                continue;
            };

            if self.get_active_task_run(&task.id)?.is_some() {
                continue;
            }

            if task.should_run(current_time) {
                runnable.push(task);
            }
        }

        Ok(runnable)
    }

    fn needs_runnable_repair(task: &BackgroundAgent) -> bool {
        if task.next_run_at.is_none() {
            // Self-heal old tasks that have a cron/interval schedule but no
            // computed next run time (e.g., created before cron normalization).
            return true;
        }
        if let (Some(next_run), Some(last_run)) = (task.next_run_at, task.last_run_at) {
            // Self-heal tasks where next_run_at is stale (before last_run_at).
            // This can happen if the daemon was restarted mid-execution and
            // the completion handler didn't persist the updated schedule.
            return next_run < last_run;
        }
        false
    }

    pub(crate) fn repair_runnable_task_if_needed(
        &self,
        task_snapshot: BackgroundAgent,
    ) -> Result<Option<BackgroundAgent>> {
        if task_snapshot.status != BackgroundAgentStatus::Active {
            return Ok(Some(task_snapshot));
        }

        if !Self::needs_runnable_repair(&task_snapshot) {
            return Ok(Some(task_snapshot));
        }

        // Reload latest state to avoid persisting a stale task snapshot.
        let Some(mut latest) = self.get_task(&task_snapshot.id)? else {
            return Ok(None);
        };

        if latest.status != BackgroundAgentStatus::Active {
            // Status changed concurrently (e.g., pause/resume race). Do not
            // repair from stale snapshot or evaluate scheduling on it.
            return Ok(None);
        }

        if !Self::needs_runnable_repair(&latest) {
            return Ok(Some(latest));
        }

        latest.update_next_run();
        let persisted =
            match self.update_task_if_status_matches(&latest, BackgroundAgentStatus::Active) {
                Ok(persisted) => persisted,
                Err(err) => {
                    warn!(
                        "Failed to persist repaired next_run_at for task {}: {}",
                        latest.id, err
                    );
                    // Skip scheduling decisions for tasks whose repaired state
                    // failed to persist to storage.
                    return Ok(None);
                }
            };
        if !persisted {
            warn!(
                "Skipped runnable repair for task {} due to concurrent status change",
                latest.id
            );
            return Ok(None);
        }

        Ok(Some(latest))
    }

    /// Save an agent task (insert or replace).
    /// Unlike `update_task`, this does not require the task to already exist.
    pub fn save_task(&self, task: &BackgroundAgent) -> Result<()> {
        let json_bytes = serde_json::to_vec(task)?;
        if let Some(existing) = self.get_task(&task.id)? {
            self.inner.update_task_raw_with_status(
                &task.id,
                existing.status.as_str(),
                task.status.as_str(),
                &json_bytes,
            )?;
        } else {
            self.inner
                .put_task_raw_with_status(&task.id, task.status.as_str(), &json_bytes)?;
        }
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

    fn update_task_if_status_matches(
        &self,
        task: &BackgroundAgent,
        expected_status: BackgroundAgentStatus,
    ) -> Result<bool> {
        let json_bytes = serde_json::to_vec(task)?;
        self.inner.update_task_raw_if_status_matches(
            &task.id,
            expected_status.as_str(),
            task.status.as_str(),
            &json_bytes,
        )
    }

    /// Delete an agent task and all its events
    pub fn delete_task(&self, id: &str) -> Result<bool> {
        let task = self.get_task(id)?;
        self.delete_checkpoints_for_task(id)?;
        let deleted = self.inner.delete_task_cascade(id)?;
        if !deleted {
            return Ok(false);
        }

        let Some(task) = task else {
            return Ok(true);
        };
        let session_id = task.chat_session_id.trim();
        if session_id.is_empty() || !task.owns_chat_session {
            return Ok(true);
        }

        let session_reused = self
            .list_tasks()?
            .into_iter()
            .any(|other| other.id != task.id && other.chat_session_id.trim() == session_id);
        if session_reused {
            return Ok(true);
        }

        let _ = self.chat_sessions.archive(session_id)?;
        Ok(true)
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

        let expected_status = if task.status == BackgroundAgentStatus::Active {
            BackgroundAgentStatus::Active
        } else if task.status == BackgroundAgentStatus::Failed && task.next_run_at.is_some() {
            BackgroundAgentStatus::Failed
        } else {
            return Err(anyhow::anyhow!(
                "Task {} cannot start from status {}",
                id,
                task.status.as_str()
            ));
        };

        task.set_running();
        // Use CAS semantics so only one concurrent caller can transition the runnable task into
        // Running, including retryable Failed interval/cron tasks.
        let started = self.update_task_if_status_matches(&task, expected_status)?;
        if !started {
            let latest_status = self
                .get_task(id)?
                .map(|latest| latest.status.as_str().to_string())
                .unwrap_or_else(|| "deleted".to_string());
            return Err(anyhow::anyhow!(
                "Task {} cannot start from status {}",
                id,
                latest_status
            ));
        }

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
}
