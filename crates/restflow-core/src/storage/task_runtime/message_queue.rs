use super::*;

impl TaskStorage {
    // ============== Task Message Operations ==============

    /// Queue a message for a task.
    pub fn send_task_message(
        &self,
        task_id: &str,
        message: String,
        source: TaskMessageSource,
    ) -> Result<TaskMessage> {
        if self.get_task(task_id)?.is_none() {
            return Err(anyhow::anyhow!("Task {} not found", task_id));
        }

        let bg_message = TaskMessage::new(task_id.to_string(), source, message);
        self.persist_task_message(&bg_message, None)?;
        Ok(bg_message)
    }

    /// Persist an agent-originated reply message for a background task.
    ///
    /// The message is stored directly as consumed to avoid re-injection into
    /// the pending message pump (which only processes queued entries).
    pub fn log_task_reply(&self, task_id: &str, message: String) -> Result<TaskMessage> {
        if self.get_task(task_id)?.is_none() {
            return Err(anyhow::anyhow!("Task {} not found", task_id));
        }

        let mut bg_message =
            TaskMessage::new(task_id.to_string(), TaskMessageSource::Agent, message);
        bg_message.mark_delivered();
        bg_message.mark_consumed();
        self.persist_task_message(&bg_message, None)?;
        Ok(bg_message)
    }

    /// Get a task message by ID.
    pub fn get_task_message(&self, message_id: &str) -> Result<Option<TaskMessage>> {
        if let Some(bytes) = self.inner.get_task_message_raw(message_id)? {
            let message: TaskMessage = serde_json::from_slice(&bytes)?;
            Ok(Some(message))
        } else {
            Ok(None)
        }
    }

    /// List all task messages for a task, sorted by timestamp descending.
    pub fn list_task_messages(&self, task_id: &str, limit: usize) -> Result<Vec<TaskMessage>> {
        let raw = self.inner.list_task_messages_for_task_raw(task_id)?;
        let mut result = Vec::new();
        for (_, bytes) in raw {
            let message: TaskMessage = serde_json::from_slice(&bytes)?;
            result.push(message);
        }
        result.sort_by_key(|message| std::cmp::Reverse(message.created_at));
        Ok(result.into_iter().take(limit).collect())
    }

    /// List queued messages waiting for delivery.
    pub fn list_pending_task_messages(
        &self,
        task_id: &str,
        limit: usize,
    ) -> Result<Vec<TaskMessage>> {
        let raw = self.inner.list_task_messages_by_status_for_task_raw(
            task_id,
            TaskMessageStatus::Queued.as_str(),
        )?;
        let mut result = Vec::new();
        for (_, bytes) in raw {
            let message: TaskMessage = serde_json::from_slice(&bytes)?;
            result.push(message);
        }
        result.sort_by_key(|message| message.created_at);
        Ok(result.into_iter().take(limit).collect())
    }

    /// Mark a queued message as delivered.
    pub fn mark_task_message_delivered(&self, message_id: &str) -> Result<Option<TaskMessage>> {
        let mut message = match self.get_task_message(message_id)? {
            Some(message) => message,
            None => return Ok(None),
        };
        let previous_status = message.status.clone();
        message.mark_delivered();
        self.persist_task_message(&message, Some(previous_status))?;
        Ok(Some(message))
    }

    /// Mark a delivered message as consumed.
    pub fn mark_task_message_consumed(&self, message_id: &str) -> Result<Option<TaskMessage>> {
        let mut message = match self.get_task_message(message_id)? {
            Some(message) => message,
            None => return Ok(None),
        };
        let previous_status = message.status.clone();
        message.mark_consumed();
        self.persist_task_message(&message, Some(previous_status))?;
        Ok(Some(message))
    }

    /// Mark a message as failed with an error.
    pub fn mark_task_message_failed(
        &self,
        message_id: &str,
        error: String,
    ) -> Result<Option<TaskMessage>> {
        let mut message = match self.get_task_message(message_id)? {
            Some(message) => message,
            None => return Ok(None),
        };
        let previous_status = message.status.clone();
        message.mark_failed(error);
        self.persist_task_message(&message, Some(previous_status))?;
        Ok(Some(message))
    }

    fn persist_task_message(
        &self,
        message: &TaskMessage,
        previous_status: Option<TaskMessageStatus>,
    ) -> Result<()> {
        let json_bytes = serde_json::to_vec(message)?;
        if let Some(previous_status) = previous_status {
            self.inner.update_task_message_raw_with_status(
                &message.id,
                &message.task_id,
                previous_status.as_str(),
                message.status.as_str(),
                &json_bytes,
            )?;
        } else {
            self.inner.put_task_message_raw_with_status(
                &message.id,
                &message.task_id,
                message.status.as_str(),
                &json_bytes,
            )?;
        }
        Ok(())
    }
}
