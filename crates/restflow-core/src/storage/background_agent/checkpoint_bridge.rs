use super::*;

impl BackgroundAgentStorage {
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

    /// Load a checkpoint by checkpoint ID.
    pub fn load_checkpoint(&self, checkpoint_id: &str) -> Result<Option<AgentCheckpoint>> {
        self.checkpoints.load(checkpoint_id)
    }

    /// Validate that a checkpoint belongs to the expected task/run pair.
    pub fn validate_checkpoint_ownership(
        &self,
        task_id: &str,
        execution_id: &str,
        checkpoint_id: Option<&str>,
    ) -> Result<Option<AgentCheckpoint>> {
        let Some(checkpoint_id) = checkpoint_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(None);
        };

        let checkpoint = self
            .load_checkpoint(checkpoint_id)?
            .ok_or_else(|| anyhow::anyhow!("checkpoint '{}' not found", checkpoint_id))?;
        if checkpoint.task_id.as_deref() != Some(task_id) {
            anyhow::bail!(
                "checkpoint '{}' belongs to task {:?}, expected '{}'",
                checkpoint_id,
                checkpoint.task_id,
                task_id
            );
        }
        if checkpoint.execution_id != execution_id {
            anyhow::bail!(
                "checkpoint '{}' belongs to execution '{}', expected '{}'",
                checkpoint_id,
                checkpoint.execution_id,
                execution_id
            );
        }

        Ok(Some(checkpoint))
    }

    /// Delete one checkpoint payload and its indices.
    pub fn delete_checkpoint(&self, checkpoint: &AgentCheckpoint) -> Result<()> {
        self.checkpoints.delete(checkpoint)
    }

    /// Delete all checkpoints/savepoints owned by one background task.
    pub fn delete_checkpoints_for_task(&self, task_id: &str) -> Result<usize> {
        let mut deleted = 0usize;
        let mut seen_ids = std::collections::HashSet::new();

        while let Some(checkpoint) = self.load_checkpoint_by_task_id(task_id)? {
            if !seen_ids.insert(checkpoint.id.clone()) {
                anyhow::bail!(
                    "checkpoint cleanup for task '{}' encountered repeated checkpoint '{}'",
                    task_id,
                    checkpoint.id
                );
            }

            if let Some(savepoint_id) = checkpoint.savepoint_id {
                let _ = self.delete_checkpoint_savepoint(savepoint_id)?;
            }
            self.delete_checkpoint(&checkpoint)?;
            deleted += 1;
        }

        Ok(deleted)
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
