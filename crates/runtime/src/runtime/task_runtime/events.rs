//! Real-time streaming events for task execution.
//!
//! These event types are shared across daemon HTTP streams and any in-process
//! publishers that need to broadcast task execution updates.
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
//! use runtime::runtime::task_runtime::events::{TaskStreamEvent, StreamEventKind};
//!
//! let started = TaskStreamEvent::started("task-1", "Build Project", "agent-1", "api");
//! let output = TaskStreamEvent::output("task-1", "Processing...\n", false);
//! let completed = TaskStreamEvent::completed("task-1", "Done", 1_500);
//! ```

pub use types::{ExecutionStats, StreamEventKind, TASK_STREAM_EVENT, TaskStreamEvent};

/// Trait for emitting task stream events
///
/// This trait allows the runner to emit events without being coupled
/// to a specific transport or buffering strategy.
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
    fn test_interrupted_event() {
        let event = TaskStreamEvent::interrupted("task-1", "Stopped by user", 3000);

        match &event.kind {
            StreamEventKind::Interrupted {
                reason,
                duration_ms,
            } => {
                assert_eq!(reason, "Stopped by user");
                assert_eq!(*duration_ms, 3000);
            }
            _ => panic!("Expected Interrupted event"),
        }
    }

    #[test]
    fn test_timeout_event() {
        let event = TaskStreamEvent::timeout("task-1", 300, 300000);

        match &event.kind {
            StreamEventKind::Interrupted { reason, .. } => {
                assert!(reason.contains("300 seconds"));
            }
            _ => panic!("Expected Interrupted event"),
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
        // Verify the event name constant for client usage
        assert_eq!(TASK_STREAM_EVENT, "task:stream");
    }

    #[test]
    fn test_event_json_structure() {
        // Test that the JSON structure matches what runtime clients expect
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
