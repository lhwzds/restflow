//! Typed checkpoint storage wrapper.
//!
//! Provides type-safe access to checkpoint persistence by wrapping the
//! byte-level API from restflow-storage with `AgentCheckpoint` models.

use anyhow::Result;
use redb::Database;
use std::sync::Arc;

use crate::models::AgentCheckpoint;

/// Typed checkpoint storage wrapper.
#[derive(Clone)]
pub struct CheckpointStorage {
    inner: restflow_storage::CheckpointStorage,
}

impl CheckpointStorage {
    /// Create a new typed checkpoint storage.
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Ok(Self {
            inner: restflow_storage::CheckpointStorage::new(db)?,
        })
    }

    /// Save a checkpoint.
    pub fn save(&self, checkpoint: &AgentCheckpoint) -> Result<()> {
        let data = serde_json::to_vec(checkpoint)?;
        self.inner.save(
            &checkpoint.id,
            &checkpoint.execution_id,
            checkpoint.task_id.as_deref(),
            &data,
        )
    }

    /// Save a checkpoint and attach a persistent redb savepoint ID.
    pub fn save_with_savepoint(&self, checkpoint: &AgentCheckpoint) -> Result<u64> {
        let data = serde_json::to_vec(checkpoint)?;
        self.inner.save_with_savepoint(
            &checkpoint.id,
            &checkpoint.execution_id,
            checkpoint.task_id.as_deref(),
            &data,
        )
    }

    /// Load a checkpoint by ID.
    pub fn load(&self, id: &str) -> Result<Option<AgentCheckpoint>> {
        match self.inner.load(id)? {
            Some(data) => Ok(Some(serde_json::from_slice(&data)?)),
            None => Ok(None),
        }
    }

    /// Load the most recent checkpoint for an execution ID.
    pub fn load_by_execution_id(&self, execution_id: &str) -> Result<Option<AgentCheckpoint>> {
        match self.inner.load_by_execution_id(execution_id)? {
            Some(data) => Ok(Some(serde_json::from_slice(&data)?)),
            None => Ok(None),
        }
    }

    /// Load the most recent checkpoint for a task ID.
    pub fn load_by_task_id(&self, task_id: &str) -> Result<Option<AgentCheckpoint>> {
        match self.inner.load_by_task_id(task_id)? {
            Some(data) => Ok(Some(serde_json::from_slice(&data)?)),
            None => Ok(None),
        }
    }

    /// Delete a checkpoint.
    pub fn delete(&self, checkpoint: &AgentCheckpoint) -> Result<()> {
        self.inner.delete(
            &checkpoint.id,
            &checkpoint.execution_id,
            checkpoint.task_id.as_deref(),
        )
    }

    /// Delete expired checkpoints. Returns the number deleted.
    pub fn cleanup_expired(&self) -> Result<usize> {
        let now = chrono::Utc::now().timestamp_millis();
        self.inner.cleanup_expired(now)
    }

    /// Delete a persistent savepoint.
    pub fn delete_savepoint(&self, savepoint_id: u64) -> Result<bool> {
        self.inner.delete_savepoint(savepoint_id)
    }
}
