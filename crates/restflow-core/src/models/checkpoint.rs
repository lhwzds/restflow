//! Checkpoint model for persisting agent execution state.
//!
//! Enables agents to save their full state at a point in time,
//! pause execution, and resume later - even after daemon restart.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// Persisted snapshot of agent execution state.
///
/// Captures everything needed to resume an interrupted agent:
/// the serialized `AgentState`, the reason for interruption, and
/// metadata for the caller that triggered the interrupt.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentCheckpoint {
    /// Unique checkpoint ID.
    pub id: String,
    /// Links to `AgentState.execution_id`.
    pub execution_id: String,
    /// Links to `BackgroundAgent.id` (if running as a background task).
    #[serde(default)]
    pub task_id: Option<String>,
    /// `AgentState.version` at checkpoint time.
    #[ts(type = "number")]
    pub version: u64,
    /// Current iteration when the checkpoint was taken.
    pub iteration: usize,
    /// Serialized `AgentState` (full JSON).
    #[ts(type = "number[]")]
    pub state_json: Vec<u8>,
    /// Why execution was interrupted.
    pub interrupt_reason: String,
    /// Extra data for the caller (e.g. tool_call_id, cost estimate).
    #[serde(default)]
    #[ts(type = "any")]
    pub interrupt_metadata: Value,
    /// Creation timestamp in milliseconds since epoch.
    #[ts(type = "number")]
    pub created_at: i64,
    /// When the checkpoint was resumed (None = still waiting).
    #[serde(default)]
    #[ts(type = "number | null")]
    pub resumed_at: Option<i64>,
    /// TTL: auto-cleanup after this timestamp (milliseconds since epoch).
    #[serde(default)]
    #[ts(type = "number | null")]
    pub expired_at: Option<i64>,
}

impl AgentCheckpoint {
    /// Create a new checkpoint.
    pub fn new(
        execution_id: String,
        task_id: Option<String>,
        version: u64,
        iteration: usize,
        state_json: Vec<u8>,
        interrupt_reason: String,
    ) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        // Default expiry: 24 hours from now
        let expired_at = Some(now + 86_400_000);
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            execution_id,
            task_id,
            version,
            iteration,
            state_json,
            interrupt_reason,
            interrupt_metadata: Value::Null,
            created_at: now,
            resumed_at: None,
            expired_at,
        }
    }

    /// Set interrupt metadata.
    pub fn with_metadata(mut self, metadata: Value) -> Self {
        self.interrupt_metadata = metadata;
        self
    }

    /// Set a custom TTL (in milliseconds from now).
    pub fn with_ttl_ms(mut self, ttl_ms: i64) -> Self {
        self.expired_at = Some(self.created_at + ttl_ms);
        self
    }

    /// Check if this checkpoint has been resumed.
    pub fn is_resumed(&self) -> bool {
        self.resumed_at.is_some()
    }

    /// Check if this checkpoint has expired.
    pub fn is_expired(&self, now_ms: i64) -> bool {
        self.expired_at.is_some_and(|exp| now_ms >= exp)
    }

    /// Mark as resumed.
    pub fn mark_resumed(&mut self) {
        self.resumed_at = Some(chrono::Utc::now().timestamp_millis());
    }
}

/// Payload provided when resuming from a checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ResumePayload {
    /// The checkpoint ID to resume from.
    pub checkpoint_id: String,
    /// Whether the action was approved.
    #[serde(default)]
    pub approved: bool,
    /// Optional message to inject into the conversation on resume.
    #[serde(default)]
    pub user_message: Option<String>,
    /// Arbitrary data the caller can pass back.
    #[serde(default)]
    #[ts(type = "any")]
    pub metadata: Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_new() {
        let cp = AgentCheckpoint::new(
            "exec-1".into(),
            Some("task-1".into()),
            5,
            3,
            b"{}".to_vec(),
            "security approval needed".into(),
        );
        assert!(!cp.id.is_empty());
        assert_eq!(cp.execution_id, "exec-1");
        assert_eq!(cp.task_id, Some("task-1".to_string()));
        assert_eq!(cp.version, 5);
        assert_eq!(cp.iteration, 3);
        assert_eq!(cp.interrupt_reason, "security approval needed");
        assert!(!cp.is_resumed());
        assert!(cp.expired_at.is_some());
    }

    #[test]
    fn test_checkpoint_with_metadata() {
        let cp = AgentCheckpoint::new(
            "exec-1".into(),
            None,
            1,
            0,
            b"{}".to_vec(),
            "test".into(),
        )
        .with_metadata(serde_json::json!({"tool_call_id": "call-1"}));

        assert_eq!(
            cp.interrupt_metadata,
            serde_json::json!({"tool_call_id": "call-1"})
        );
    }

    #[test]
    fn test_checkpoint_mark_resumed() {
        let mut cp = AgentCheckpoint::new(
            "exec-1".into(),
            None,
            1,
            0,
            b"{}".to_vec(),
            "test".into(),
        );
        assert!(!cp.is_resumed());
        cp.mark_resumed();
        assert!(cp.is_resumed());
        assert!(cp.resumed_at.is_some());
    }

    #[test]
    fn test_checkpoint_expired() {
        let mut cp = AgentCheckpoint::new(
            "exec-1".into(),
            None,
            1,
            0,
            b"{}".to_vec(),
            "test".into(),
        );
        let now = chrono::Utc::now().timestamp_millis();
        // Not expired yet
        assert!(!cp.is_expired(now));
        // Set expired_at to past
        cp.expired_at = Some(now - 1000);
        assert!(cp.is_expired(now));
    }

    #[test]
    fn test_checkpoint_serialization() {
        let cp = AgentCheckpoint::new(
            "exec-1".into(),
            Some("task-1".into()),
            5,
            3,
            b"{\"status\":\"Running\"}".to_vec(),
            "approval needed".into(),
        );

        let json = serde_json::to_string(&cp).unwrap();
        let deserialized: AgentCheckpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.execution_id, "exec-1");
        assert_eq!(deserialized.version, 5);
        assert_eq!(deserialized.iteration, 3);
        assert_eq!(deserialized.interrupt_reason, "approval needed");
    }

    #[test]
    fn test_resume_payload_serialization() {
        let payload = ResumePayload {
            checkpoint_id: "chk-1".into(),
            approved: true,
            user_message: Some("Go ahead".into()),
            metadata: serde_json::json!({"extra": "data"}),
        };
        let json = serde_json::to_string(&payload).unwrap();
        let deserialized: ResumePayload = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.checkpoint_id, "chk-1");
        assert!(deserialized.approved);
        assert_eq!(deserialized.user_message, Some("Go ahead".to_string()));
    }
}
