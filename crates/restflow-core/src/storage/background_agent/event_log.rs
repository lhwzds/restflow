use super::*;

impl BackgroundAgentStorage {
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
