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

    /// Delete expired checkpoints.
    pub fn cleanup_expired_checkpoints(&self) -> Result<usize> {
        self.checkpoints.cleanup_expired()
    }

    /// Delete a persistent savepoint if it exists.
    pub fn delete_checkpoint_savepoint(&self, savepoint_id: u64) -> Result<bool> {
        self.checkpoints.delete_savepoint(savepoint_id)
    }
}
