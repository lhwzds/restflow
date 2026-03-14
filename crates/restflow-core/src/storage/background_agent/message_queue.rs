use super::*;

impl BackgroundAgentStorage {
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

    /// Persist an agent-originated reply message for a background task.
    ///
    /// The message is stored directly as consumed to avoid re-injection into
    /// the pending message pump (which only processes queued entries).
    pub fn log_background_agent_reply(
        &self,
        background_agent_id: &str,
        message: String,
    ) -> Result<BackgroundMessage> {
        if self.get_task(background_agent_id)?.is_none() {
            return Err(anyhow::anyhow!("Task {} not found", background_agent_id));
        }

        let mut bg_message = BackgroundMessage::new(
            background_agent_id.to_string(),
            BackgroundMessageSource::Agent,
            message,
        );
        bg_message.mark_delivered();
        bg_message.mark_consumed();
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
}
