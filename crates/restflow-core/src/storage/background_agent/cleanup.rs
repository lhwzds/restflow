use super::*;

impl BackgroundAgentStorage {
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
                // Verify status is still terminal for cleanup.
                if !matches!(
                    current.status,
                    BackgroundAgentStatus::Completed
                        | BackgroundAgentStatus::Failed
                            if current.next_run_at.is_none()
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
}
