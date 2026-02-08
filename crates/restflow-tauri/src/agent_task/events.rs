//! Real-time streaming events for agent task execution.
//!
//! These event types are designed for Tauri's event system to stream
//! task execution updates to the frontend in real-time.
//!
//! # Event Flow
//!
//! ```text
//! TaskStarted → [TaskOutput]* → TaskCompleted/TaskFailed
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use tauri::Manager;
//! use restflow_tauri::agent_task::events::{TaskStreamEvent, StreamEventKind};
//!
//! // In the runner, emit events to the frontend
//! app_handle.emit("background-agent:stream", TaskStreamEvent::started(task_id));
//!
//! // Stream output as it arrives
//! app_handle.emit("background-agent:stream", TaskStreamEvent::output(task_id, "Processing...", false));
//!
//! // On completion
//! app_handle.emit("background-agent:stream", TaskStreamEvent::completed(task_id, result, duration_ms));
//! ```

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Event name constant for Tauri event emission
pub const TASK_STREAM_EVENT: &str = "background-agent:stream";

/// Real-time streaming event for task execution
///
/// This is the primary event type emitted via Tauri's event system
/// for real-time updates during task execution.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TaskStreamEvent {
    /// ID of the task this event belongs to
    pub task_id: String,
    /// Timestamp of the event (milliseconds since epoch)
    #[ts(type = "number")]
    pub timestamp: i64,
    /// The kind of event and its associated data
    pub kind: StreamEventKind,
}

/// Discriminated union for different stream event types
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEventKind {
    /// Task execution has started
    Started {
        /// Name of the task
        task_name: String,
        /// Agent ID being executed
        agent_id: String,
        /// Execution mode description (e.g., "api", "cli:claude", "cli:aider")
        execution_mode: String,
    },

    /// Output from task execution (stdout or stderr)
    Output {
        /// The output text
        text: String,
        /// Whether this is stderr (true) or stdout (false)
        is_stderr: bool,
        /// Whether this is a complete line (ends with newline)
        is_complete: bool,
    },

    /// Progress update (for long-running tasks)
    Progress {
        /// Current step or phase description
        phase: String,
        /// Progress percentage (0-100), if determinable
        #[serde(skip_serializing_if = "Option::is_none")]
        percent: Option<u8>,
        /// Additional details about current progress
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<String>,
    },

    /// Task completed successfully
    Completed {
        /// Final result/output from the task
        result: String,
        /// Execution duration in milliseconds
        #[ts(type = "number")]
        duration_ms: i64,
        /// Summary statistics
        #[serde(skip_serializing_if = "Option::is_none")]
        stats: Option<ExecutionStats>,
    },

    /// Task failed with an error
    Failed {
        /// Error message
        error: String,
        /// Error code, if available
        #[serde(skip_serializing_if = "Option::is_none")]
        error_code: Option<String>,
        /// Execution duration in milliseconds before failure
        #[ts(type = "number")]
        duration_ms: i64,
        /// Whether the error is recoverable (can retry)
        recoverable: bool,
    },

    /// Task was cancelled (e.g., timeout or user cancellation)
    Cancelled {
        /// Reason for cancellation
        reason: String,
        /// Execution duration in milliseconds before cancellation
        #[ts(type = "number")]
        duration_ms: i64,
    },

    /// Heartbeat to indicate the task is still running
    Heartbeat {
        /// How long the task has been running (milliseconds)
        #[ts(type = "number")]
        elapsed_ms: i64,
    },
}

/// Statistics about task execution
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExecutionStats {
    /// Number of output lines produced
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_lines: Option<u32>,
    /// Total bytes of output
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_bytes: Option<u64>,
    /// Number of API calls made (for API mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_calls: Option<u32>,
    /// Tokens used (for API mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_used: Option<u32>,
    /// Cost in USD (for API mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
}

impl TaskStreamEvent {
    /// Create a new stream event with the current timestamp
    pub fn new(task_id: impl Into<String>, kind: StreamEventKind) -> Self {
        Self {
            task_id: task_id.into(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            kind,
        }
    }

    /// Create a task started event
    pub fn started(
        task_id: impl Into<String>,
        task_name: impl Into<String>,
        agent_id: impl Into<String>,
        execution_mode: impl Into<String>,
    ) -> Self {
        Self::new(
            task_id,
            StreamEventKind::Started {
                task_name: task_name.into(),
                agent_id: agent_id.into(),
                execution_mode: execution_mode.into(),
            },
        )
    }

    /// Create an output event
    pub fn output(task_id: impl Into<String>, text: impl Into<String>, is_stderr: bool) -> Self {
        let text = text.into();
        let is_complete = text.ends_with('\n');
        Self::new(
            task_id,
            StreamEventKind::Output {
                text,
                is_stderr,
                is_complete,
            },
        )
    }

    /// Create an output event with explicit completeness
    pub fn output_partial(
        task_id: impl Into<String>,
        text: impl Into<String>,
        is_stderr: bool,
        is_complete: bool,
    ) -> Self {
        Self::new(
            task_id,
            StreamEventKind::Output {
                text: text.into(),
                is_stderr,
                is_complete,
            },
        )
    }

    /// Create a progress event
    pub fn progress(
        task_id: impl Into<String>,
        phase: impl Into<String>,
        percent: Option<u8>,
        details: Option<String>,
    ) -> Self {
        Self::new(
            task_id,
            StreamEventKind::Progress {
                phase: phase.into(),
                percent,
                details,
            },
        )
    }

    /// Create a completed event
    pub fn completed(
        task_id: impl Into<String>,
        result: impl Into<String>,
        duration_ms: i64,
    ) -> Self {
        Self::new(
            task_id,
            StreamEventKind::Completed {
                result: result.into(),
                duration_ms,
                stats: None,
            },
        )
    }

    /// Create a completed event with statistics
    pub fn completed_with_stats(
        task_id: impl Into<String>,
        result: impl Into<String>,
        duration_ms: i64,
        stats: ExecutionStats,
    ) -> Self {
        Self::new(
            task_id,
            StreamEventKind::Completed {
                result: result.into(),
                duration_ms,
                stats: Some(stats),
            },
        )
    }

    /// Create a failed event
    pub fn failed(
        task_id: impl Into<String>,
        error: impl Into<String>,
        duration_ms: i64,
        recoverable: bool,
    ) -> Self {
        Self::new(
            task_id,
            StreamEventKind::Failed {
                error: error.into(),
                error_code: None,
                duration_ms,
                recoverable,
            },
        )
    }

    /// Create a failed event with error code
    pub fn failed_with_code(
        task_id: impl Into<String>,
        error: impl Into<String>,
        error_code: impl Into<String>,
        duration_ms: i64,
        recoverable: bool,
    ) -> Self {
        Self::new(
            task_id,
            StreamEventKind::Failed {
                error: error.into(),
                error_code: Some(error_code.into()),
                duration_ms,
                recoverable,
            },
        )
    }

    /// Create a cancelled event
    pub fn cancelled(
        task_id: impl Into<String>,
        reason: impl Into<String>,
        duration_ms: i64,
    ) -> Self {
        Self::new(
            task_id,
            StreamEventKind::Cancelled {
                reason: reason.into(),
                duration_ms,
            },
        )
    }

    /// Create a timeout event (convenience for cancelled with timeout reason)
    pub fn timeout(task_id: impl Into<String>, timeout_secs: u64, duration_ms: i64) -> Self {
        Self::cancelled(
            task_id,
            format!("Task timed out after {} seconds", timeout_secs),
            duration_ms,
        )
    }

    /// Create a heartbeat event
    pub fn heartbeat(task_id: impl Into<String>, elapsed_ms: i64) -> Self {
        Self::new(task_id, StreamEventKind::Heartbeat { elapsed_ms })
    }
}

/// Trait for emitting task stream events
///
/// This trait allows the runner to emit events without being coupled
/// to a specific implementation (Tauri, channel, etc.)
#[async_trait::async_trait]
pub trait TaskEventEmitter: Send + Sync {
    /// Emit a task stream event
    async fn emit(&self, event: TaskStreamEvent);
}

/// No-op event emitter for when streaming is not needed
pub struct NoopEventEmitter;

#[async_trait::async_trait]
impl TaskEventEmitter for NoopEventEmitter {
    async fn emit(&self, _event: TaskStreamEvent) {
        // No-op
    }
}

/// Channel-based event emitter for testing or async streaming
pub struct ChannelEventEmitter {
    sender: tokio::sync::mpsc::UnboundedSender<TaskStreamEvent>,
}

impl ChannelEventEmitter {
    /// Create a new channel-based emitter and return it with the receiver
    pub fn new() -> (Self, tokio::sync::mpsc::UnboundedReceiver<TaskStreamEvent>) {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        (Self { sender }, receiver)
    }
}

#[async_trait::async_trait]
impl TaskEventEmitter for ChannelEventEmitter {
    async fn emit(&self, event: TaskStreamEvent) {
        let _ = self.sender.send(event);
    }
}

/// Tauri-based event emitter that uses AppHandle to emit events to the frontend
///
/// This emitter integrates with Tauri's event system to stream real-time
/// task execution updates to the frontend via the `background-agent:stream` event.
///
/// # Example
///
/// ```ignore
/// use restflow_tauri::agent_task::events::TauriEventEmitter;
/// use tauri::Manager;
///
/// // In a Tauri command
/// let emitter = TauriEventEmitter::new(app_handle.clone());
/// emitter.emit(TaskStreamEvent::started("task-1", "My Task", "agent-1", "api")).await;
/// ```
#[derive(Clone)]
pub struct TauriEventEmitter {
    app_handle: tauri::AppHandle,
}

impl TauriEventEmitter {
    /// Create a new Tauri event emitter
    pub fn new(app_handle: tauri::AppHandle) -> Self {
        Self { app_handle }
    }
}

#[async_trait::async_trait]
impl TaskEventEmitter for TauriEventEmitter {
    async fn emit(&self, event: TaskStreamEvent) {
        use tauri::Emitter;
        if let Err(e) = self.app_handle.emit(TASK_STREAM_EVENT, &event) {
            tracing::warn!("Failed to emit task stream event: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_started_event() {
        let event = TaskStreamEvent::started("task-1", "My Task", "agent-1", "api");

        assert_eq!(event.task_id, "task-1");
        assert!(event.timestamp > 0);

        match &event.kind {
            StreamEventKind::Started {
                task_name,
                agent_id,
                execution_mode,
            } => {
                assert_eq!(task_name, "My Task");
                assert_eq!(agent_id, "agent-1");
                assert_eq!(execution_mode, "api");
            }
            _ => panic!("Expected Started event"),
        }
    }

    #[test]
    fn test_output_event() {
        let event = TaskStreamEvent::output("task-1", "Hello world\n", false);

        match &event.kind {
            StreamEventKind::Output {
                text,
                is_stderr,
                is_complete,
            } => {
                assert_eq!(text, "Hello world\n");
                assert!(!is_stderr);
                assert!(is_complete); // ends with newline
            }
            _ => panic!("Expected Output event"),
        }

        // Test partial output
        let event = TaskStreamEvent::output_partial("task-1", "partial", false, false);
        match &event.kind {
            StreamEventKind::Output { is_complete, .. } => {
                assert!(!is_complete);
            }
            _ => panic!("Expected Output event"),
        }
    }

    #[test]
    fn test_progress_event() {
        let event =
            TaskStreamEvent::progress("task-1", "Compiling", Some(50), Some("main.rs".into()));

        match &event.kind {
            StreamEventKind::Progress {
                phase,
                percent,
                details,
            } => {
                assert_eq!(phase, "Compiling");
                assert_eq!(*percent, Some(50));
                assert_eq!(details.as_deref(), Some("main.rs"));
            }
            _ => panic!("Expected Progress event"),
        }
    }

    #[test]
    fn test_completed_event() {
        let event = TaskStreamEvent::completed("task-1", "Success!", 1500);

        match &event.kind {
            StreamEventKind::Completed {
                result,
                duration_ms,
                stats,
            } => {
                assert_eq!(result, "Success!");
                assert_eq!(*duration_ms, 1500);
                assert!(stats.is_none());
            }
            _ => panic!("Expected Completed event"),
        }
    }

    #[test]
    fn test_completed_with_stats() {
        let stats = ExecutionStats {
            output_lines: Some(100),
            output_bytes: Some(5000),
            api_calls: None,
            tokens_used: None,
            cost_usd: None,
        };
        let event = TaskStreamEvent::completed_with_stats("task-1", "Done", 2000, stats);

        match &event.kind {
            StreamEventKind::Completed { stats, .. } => {
                let s = stats.as_ref().unwrap();
                assert_eq!(s.output_lines, Some(100));
                assert_eq!(s.output_bytes, Some(5000));
            }
            _ => panic!("Expected Completed event"),
        }
    }

    #[test]
    fn test_failed_event() {
        let event = TaskStreamEvent::failed("task-1", "Connection refused", 500, true);

        match &event.kind {
            StreamEventKind::Failed {
                error,
                error_code,
                duration_ms,
                recoverable,
            } => {
                assert_eq!(error, "Connection refused");
                assert!(error_code.is_none());
                assert_eq!(*duration_ms, 500);
                assert!(*recoverable);
            }
            _ => panic!("Expected Failed event"),
        }
    }

    #[test]
    fn test_failed_with_code() {
        let event =
            TaskStreamEvent::failed_with_code("task-1", "API Error", "E_API_001", 1000, false);

        match &event.kind {
            StreamEventKind::Failed {
                error_code,
                recoverable,
                ..
            } => {
                assert_eq!(error_code.as_deref(), Some("E_API_001"));
                assert!(!recoverable);
            }
            _ => panic!("Expected Failed event"),
        }
    }

    #[test]
    fn test_cancelled_event() {
        let event = TaskStreamEvent::cancelled("task-1", "User requested", 3000);

        match &event.kind {
            StreamEventKind::Cancelled {
                reason,
                duration_ms,
            } => {
                assert_eq!(reason, "User requested");
                assert_eq!(*duration_ms, 3000);
            }
            _ => panic!("Expected Cancelled event"),
        }
    }

    #[test]
    fn test_timeout_event() {
        let event = TaskStreamEvent::timeout("task-1", 300, 300000);

        match &event.kind {
            StreamEventKind::Cancelled { reason, .. } => {
                assert!(reason.contains("300 seconds"));
            }
            _ => panic!("Expected Cancelled event"),
        }
    }

    #[test]
    fn test_heartbeat_event() {
        let event = TaskStreamEvent::heartbeat("task-1", 5000);

        match &event.kind {
            StreamEventKind::Heartbeat { elapsed_ms } => {
                assert_eq!(*elapsed_ms, 5000);
            }
            _ => panic!("Expected Heartbeat event"),
        }
    }

    #[test]
    fn test_serialization() {
        let event = TaskStreamEvent::started("task-1", "Test Task", "agent-1", "cli:claude");
        let json = serde_json::to_string(&event).unwrap();

        assert!(json.contains("task-1"));
        assert!(json.contains("started"));
        assert!(json.contains("Test Task"));
        assert!(json.contains("cli:claude"));

        // Verify deserialization
        let deserialized: TaskStreamEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.task_id, "task-1");
    }

    #[test]
    fn test_output_event_serialization() {
        let event = TaskStreamEvent::output("task-1", "Hello\n", true);
        let json = serde_json::to_string(&event).unwrap();

        assert!(json.contains("output"));
        assert!(json.contains("is_stderr"));
        assert!(json.contains("true"));
    }

    #[tokio::test]
    async fn test_channel_emitter() {
        let (emitter, mut receiver) = ChannelEventEmitter::new();

        emitter
            .emit(TaskStreamEvent::started("task-1", "Test", "agent-1", "api"))
            .await;
        emitter
            .emit(TaskStreamEvent::output("task-1", "Hello\n", false))
            .await;
        emitter
            .emit(TaskStreamEvent::completed("task-1", "Done", 1000))
            .await;

        let mut events = Vec::new();
        while let Ok(event) = receiver.try_recv() {
            events.push(event);
        }

        assert_eq!(events.len(), 3);
        assert!(matches!(&events[0].kind, StreamEventKind::Started { .. }));
        assert!(matches!(&events[1].kind, StreamEventKind::Output { .. }));
        assert!(matches!(&events[2].kind, StreamEventKind::Completed { .. }));
    }

    #[tokio::test]
    async fn test_noop_emitter() {
        let emitter = NoopEventEmitter;
        // Should not panic
        emitter
            .emit(TaskStreamEvent::started("task-1", "Test", "agent-1", "api"))
            .await;
    }

    #[test]
    fn test_task_stream_event_constant() {
        // Verify the event name constant for frontend usage
        assert_eq!(TASK_STREAM_EVENT, "background-agent:stream");
    }

    #[test]
    fn test_event_json_structure() {
        // Test that the JSON structure matches what the frontend expects
        let event =
            TaskStreamEvent::started("task-123", "Build Project", "agent-456", "cli:claude");
        let json = serde_json::to_value(&event).unwrap();

        // Verify structure
        assert!(json.get("task_id").is_some());
        assert!(json.get("timestamp").is_some());
        assert!(json.get("kind").is_some());

        // Verify kind has type discriminator
        let kind = json.get("kind").unwrap();
        assert_eq!(kind.get("type").unwrap(), "started");
        assert_eq!(kind.get("task_name").unwrap(), "Build Project");
        assert_eq!(kind.get("agent_id").unwrap(), "agent-456");
        assert_eq!(kind.get("execution_mode").unwrap(), "cli:claude");
    }

    #[test]
    fn test_output_event_json_structure() {
        let event = TaskStreamEvent::output("task-1", "Building crate...\n", false);
        let json = serde_json::to_value(&event).unwrap();

        let kind = json.get("kind").unwrap();
        assert_eq!(kind.get("type").unwrap(), "output");
        assert_eq!(kind.get("text").unwrap(), "Building crate...\n");
        assert_eq!(kind.get("is_stderr").unwrap(), false);
        assert_eq!(kind.get("is_complete").unwrap(), true);
    }

    #[test]
    fn test_completed_event_json_structure() {
        let stats = ExecutionStats {
            output_lines: Some(150),
            output_bytes: Some(8000),
            api_calls: Some(3),
            tokens_used: Some(1500),
            cost_usd: None,
        };
        let event =
            TaskStreamEvent::completed_with_stats("task-1", "Build successful", 45000, stats);
        let json = serde_json::to_value(&event).unwrap();

        let kind = json.get("kind").unwrap();
        assert_eq!(kind.get("type").unwrap(), "completed");
        assert_eq!(kind.get("duration_ms").unwrap(), 45000);

        let stats = kind.get("stats").unwrap();
        assert_eq!(stats.get("output_lines").unwrap(), 150);
        assert_eq!(stats.get("tokens_used").unwrap(), 1500);
    }

    #[test]
    fn test_failed_event_json_structure() {
        let event = TaskStreamEvent::failed_with_code(
            "task-1",
            "Build failed: syntax error",
            "E_COMPILE",
            5000,
            true,
        );
        let json = serde_json::to_value(&event).unwrap();

        let kind = json.get("kind").unwrap();
        assert_eq!(kind.get("type").unwrap(), "failed");
        assert_eq!(kind.get("error").unwrap(), "Build failed: syntax error");
        assert_eq!(kind.get("error_code").unwrap(), "E_COMPILE");
        assert_eq!(kind.get("recoverable").unwrap(), true);
    }
}
