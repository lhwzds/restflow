//! Transactional checkpoint for atomic persistence.
//!
//! Implements a prepare-then-execute-then-commit pattern where checkpoints
//! are only persisted after tool execution succeeds. This prevents orphaned
//! or corrupted checkpoints when tools fail.
//!
//! Inspired by:
//! - gitbutler: UnmaterializedOplogSnapshot (prepare → execute → commit)
//! - langgraph: Version-based incremental checkpointing
//! - monty: Serializable execution snapshots

use crate::models::AgentCheckpoint;
use crate::storage::BackgroundAgentStorage;
use anyhow::Result;
use restflow_ai::AgentState;

/// Metadata for an uncommitted checkpoint.
#[derive(Debug, Clone)]
pub struct CheckpointMeta {
    /// Task ID for the checkpoint.
    pub task_id: String,
    /// Reason for the checkpoint (e.g., "tool execution").
    pub reason: String,
    /// Iteration number when checkpoint was prepared.
    pub iteration: usize,
}

/// An in-memory checkpoint that has not yet been persisted.
///
/// Captures the agent state before a risky operation (like tool execution)
/// and only commits to the database when the operation succeeds.
#[derive(Debug)]
pub struct UncommittedCheckpoint {
    /// The serialized agent state.
    pub state_json: Vec<u8>,
    /// Metadata about this checkpoint.
    pub meta: CheckpointMeta,
    /// Execution ID from the agent state.
    pub execution_id: String,
    /// Version from the agent state.
    pub version: u64,
}

impl UncommittedCheckpoint {
    /// Create a new uncommitted checkpoint from an agent state.
    pub fn new(state: &AgentState, task_id: String, reason: String) -> Self {
        let state_json = serde_json::to_vec(state).unwrap_or_default();
        let meta = CheckpointMeta {
            task_id,
            reason,
            iteration: state.iteration,
        };

        Self {
            state_json,
            meta,
            execution_id: state.execution_id.clone(),
            version: state.version,
        }
    }

    /// Convert to a persistable AgentCheckpoint.
    ///
    /// This should only be called when the tool execution succeeds.
    pub fn into_persistable(self) -> AgentCheckpoint {
        AgentCheckpoint::new(
            self.execution_id,
            Some(self.meta.task_id),
            self.version,
            self.meta.iteration,
            self.state_json,
            self.meta.reason,
        )
    }
}

/// Prepare a checkpoint by capturing the current agent state in memory.
///
/// This does NOT write to the database. The checkpoint is held in memory
/// until `commit_if_success` is called with a successful result.
pub fn prepare(state: &AgentState, task_id: String, reason: String) -> UncommittedCheckpoint {
    UncommittedCheckpoint::new(state, task_id, reason)
}

/// Persist the checkpoint only if the result is Ok.
///
/// If the result is Err, the checkpoint is simply dropped without any DB write.
/// This prevents orphaned/corrupted checkpoints from polluting the database.
pub fn commit_if_success(
    storage: &BackgroundAgentStorage,
    uncommitted: Option<UncommittedCheckpoint>,
    result: &Result<()>,
) -> Result<()> {
    let Some(checkpoint) = uncommitted else {
        return Ok(());
    };

    if result.is_ok() {
        let persistable = checkpoint.into_persistable();
        storage.save_checkpoint(&persistable)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::Database;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn create_test_storage() -> BackgroundAgentStorage {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        BackgroundAgentStorage::new(db).unwrap()
    }

    #[test]
    fn test_uncommitted_checkpoint_new() {
        let mut state = AgentState::new("exec-1".to_string(), 10);
        state.iteration = 5;
        state.add_message(restflow_ai::llm::Message::user("test"));

        let checkpoint =
            UncommittedCheckpoint::new(&state, "task-123".to_string(), "tool execution".to_string());

        assert_eq!(checkpoint.meta.task_id, "task-123");
        assert_eq!(checkpoint.meta.iteration, 5);
        assert_eq!(checkpoint.meta.reason, "tool execution");
        assert_eq!(checkpoint.execution_id, "exec-1");
        assert!(!checkpoint.state_json.is_empty());
    }

    #[test]
    fn test_into_persistable() {
        let mut state = AgentState::new("exec-2".to_string(), 10);
        state.iteration = 3;
        state.add_message(restflow_ai::llm::Message::user("test"));

        let uncommitted = UncommittedCheckpoint::new(
            &state,
            "task-456".to_string(),
            "security checkpoint".to_string(),
        );

        let persisted = uncommitted.into_persistable();

        assert_eq!(persisted.execution_id, "exec-2");
        assert_eq!(persisted.task_id, Some("task-456".to_string()));
        assert_eq!(persisted.iteration, 3);
        assert_eq!(persisted.interrupt_reason, "security checkpoint");
        // ID should be a valid UUID (36 chars with hyphens)
        assert_eq!(persisted.id.len(), 36);
        assert!(persisted.id.contains('-'));
    }

    #[test]
    fn test_prepare_creates_uncommitted_checkpoint() {
        let mut state = AgentState::new("exec-3".to_string(), 10);
        state.iteration = 2;

        let checkpoint = prepare(&state, "task-789".to_string(), "before tool call".to_string());

        assert_eq!(checkpoint.meta.task_id, "task-789");
        assert_eq!(checkpoint.meta.reason, "before tool call");
        assert_eq!(checkpoint.execution_id, "exec-3");
    }

    #[test]
    fn test_commit_if_success_none_checkpoint() {
        let storage = create_test_storage();

        let result: Result<()> = Ok(());
        let commit_result = commit_if_success(&storage, None, &result);
        assert!(commit_result.is_ok());
    }

    #[test]
    fn test_commit_if_success_with_ok_result() {
        let storage = create_test_storage();

        let mut state = AgentState::new("exec-4".to_string(), 10);
        state.iteration = 1;

        let checkpoint = prepare(&state, "task-111".to_string(), "after success".to_string());
        let result: Result<()> = Ok(());

        let commit_result = commit_if_success(&storage, Some(checkpoint), &result);
        assert!(commit_result.is_ok());

        // Verify checkpoint was saved
        let loaded = storage.load_checkpoint_by_task_id("task-111").unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.execution_id, "exec-4");
        assert_eq!(loaded.iteration, 1);
    }

    #[test]
    fn test_commit_if_success_with_err_result() {
        let storage = create_test_storage();

        let mut state = AgentState::new("exec-5".to_string(), 10);
        state.iteration = 1;

        let checkpoint = prepare(&state, "task-222".to_string(), "should not persist".to_string());
        let result: Result<()> = Err(anyhow::anyhow!("tool failed"));

        let commit_result = commit_if_success(&storage, Some(checkpoint), &result);
        assert!(commit_result.is_ok()); // commit_if_success returns Ok even on dropped checkpoint

        // Verify checkpoint was NOT saved
        let loaded = storage.load_checkpoint_by_task_id("task-222").unwrap();
        assert!(loaded.is_none());
    }
}
