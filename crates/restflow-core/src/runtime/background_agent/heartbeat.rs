//! Heartbeat types and emitters for agent task runner status.
//!
//! This module provides types for status events that are emitted inline
//! by the BackgroundAgentRunner during its poll cycle.
//!
//! # Usage
//!
//! ```ignore
//! use restflow_core::runtime::background_agent::{
//!     HeartbeatEvent, HeartbeatPulse, RunnerStatus, RunnerStatusEvent,
//! };
//!
//! let pulse = HeartbeatEvent::Pulse(HeartbeatPulse {
//!     sequence: 1,
//!     timestamp: chrono::Utc::now().timestamp_millis(),
//!     active_tasks: 0,
//!     pending_tasks: 0,
//!     uptime_ms: 1_000,
//!     stats: None,
//! });
//! let status = HeartbeatEvent::StatusChange(RunnerStatusEvent {
//!     status: RunnerStatus::Running,
//!     timestamp: chrono::Utc::now().timestamp_millis(),
//!     message: None,
//! });
//! ```

use serde::{Deserialize, Serialize};
use specta::Type;
use tokio::sync::mpsc;
use ts_rs::TS;

/// Event name for heartbeat and runner status streams.
pub const HEARTBEAT_EVENT: &str = "background-agent:heartbeat";

/// Heartbeat event sent to connected daemon clients.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type)]
#[specta(skip_attr = "ts")]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HeartbeatEvent {
    /// Regular heartbeat pulse with status
    Pulse(HeartbeatPulse),
    /// Runner status changed
    StatusChange(RunnerStatusEvent),
    /// Warning about issues
    Warning(HeartbeatWarning),
}

/// Regular heartbeat pulse data
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type)]
#[specta(skip_attr = "ts")]
#[ts(export)]
pub struct HeartbeatPulse {
    /// Sequence number for this heartbeat
    #[ts(type = "number")]
    pub sequence: u64,
    /// Timestamp of this heartbeat (milliseconds since epoch)
    #[ts(type = "number")]
    pub timestamp: i64,
    /// Number of active (running) tasks
    pub active_tasks: u32,
    /// Number of pending tasks (scheduled but not yet run)
    pub pending_tasks: u32,
    /// Runner uptime in milliseconds
    #[ts(type = "number")]
    pub uptime_ms: u64,
    /// Optional system stats
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<SystemStats>,
}

/// System statistics included in heartbeat
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type)]
#[specta(skip_attr = "ts")]
#[ts(export)]
pub struct SystemStats {
    /// Memory usage in bytes (if available)
    #[ts(type = "number | null")]
    pub memory_bytes: Option<u64>,
    /// Number of tokio tasks (if available)
    pub tokio_tasks: Option<u32>,
}

/// Runner status change event
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type)]
#[specta(skip_attr = "ts")]
#[ts(export)]
pub struct RunnerStatusEvent {
    /// Current runner status
    pub status: RunnerStatus,
    /// Timestamp of the status change
    #[ts(type = "number")]
    pub timestamp: i64,
    /// Optional message about the status change
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Runner status enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Type)]
#[specta(skip_attr = "ts")]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum RunnerStatus {
    /// Runner is starting up
    Starting,
    /// Runner is running normally
    Running,
    /// Runner is paused
    Paused,
    /// Runner is stopping
    Stopping,
    /// Runner has stopped
    Stopped,
    /// Runner encountered an error
    Error,
}

/// Warning event for issues detected during execution
#[derive(Debug, Clone, Serialize, Deserialize, TS, Type)]
#[specta(skip_attr = "ts")]
#[ts(export)]
pub struct HeartbeatWarning {
    /// Warning code for categorization
    pub code: String,
    /// Human-readable warning message
    pub message: String,
    /// Timestamp of the warning
    #[ts(type = "number")]
    pub timestamp: i64,
}

/// Trait for emitting heartbeat events (allows dependency injection)
#[async_trait::async_trait]
pub trait HeartbeatEmitter: Send + Sync {
    /// Emit a heartbeat event
    async fn emit(&self, event: HeartbeatEvent);
}

/// Channel-based heartbeat emitter for testing
pub struct ChannelHeartbeatEmitter {
    sender: mpsc::Sender<HeartbeatEvent>,
}

impl ChannelHeartbeatEmitter {
    pub fn new(sender: mpsc::Sender<HeartbeatEvent>) -> Self {
        Self { sender }
    }
}

#[async_trait::async_trait]
impl HeartbeatEmitter for ChannelHeartbeatEmitter {
    async fn emit(&self, event: HeartbeatEvent) {
        let _ = self.sender.send(event).await;
    }
}

/// No-op heartbeat emitter for when heartbeats are disabled
pub struct NoopHeartbeatEmitter;

#[async_trait::async_trait]
impl HeartbeatEmitter for NoopHeartbeatEmitter {
    async fn emit(&self, _event: HeartbeatEvent) {
        // No-op
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_heartbeat_pulse_serialization() {
        let pulse = HeartbeatPulse {
            sequence: 42,
            timestamp: 1704067200000,
            active_tasks: 3,
            pending_tasks: 7,
            uptime_ms: 60000,
            stats: Some(SystemStats {
                memory_bytes: Some(1024 * 1024 * 100),
                tokio_tasks: Some(15),
            }),
        };

        let event = HeartbeatEvent::Pulse(pulse);
        let json = serde_json::to_string(&event).unwrap();

        assert!(json.contains("\"kind\":\"pulse\""));
        assert!(json.contains("\"sequence\":42"));
        assert!(json.contains("\"active_tasks\":3"));
    }

    #[tokio::test]
    async fn test_runner_status_serialization() {
        let status = RunnerStatusEvent {
            status: RunnerStatus::Running,
            timestamp: 1704067200000,
            message: Some("All systems go".to_string()),
        };

        let event = HeartbeatEvent::StatusChange(status);
        let json = serde_json::to_string(&event).unwrap();

        assert!(json.contains("\"kind\":\"status_change\""));
        assert!(json.contains("\"status\":\"running\""));
    }

    #[tokio::test]
    async fn test_warning_serialization() {
        let warning = HeartbeatWarning {
            code: "TEST_WARNING".to_string(),
            message: "This is a test".to_string(),
            timestamp: 1704067200000,
        };

        let event = HeartbeatEvent::Warning(warning);
        let json = serde_json::to_string(&event).unwrap();

        assert!(json.contains("\"kind\":\"warning\""));
        assert!(json.contains("\"code\":\"TEST_WARNING\""));
    }

    #[tokio::test]
    async fn test_noop_emitter() {
        let emitter = NoopHeartbeatEmitter;

        // Should not panic or error
        emitter
            .emit(HeartbeatEvent::Pulse(HeartbeatPulse {
                sequence: 1,
                timestamp: 0,
                active_tasks: 0,
                pending_tasks: 0,
                uptime_ms: 0,
                stats: None,
            }))
            .await;
    }

    #[test]
    fn test_runner_status_variants() {
        let statuses = vec![
            RunnerStatus::Starting,
            RunnerStatus::Running,
            RunnerStatus::Paused,
            RunnerStatus::Stopping,
            RunnerStatus::Stopped,
            RunnerStatus::Error,
        ];

        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let deserialized: RunnerStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, deserialized);
        }
    }
}
