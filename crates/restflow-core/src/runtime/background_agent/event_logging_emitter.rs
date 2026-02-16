//! Event-logging wrapper for StreamEmitter.
//!
//! Wraps any StreamEmitter and logs tool call events to an EventLog.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use chrono::Utc;

use restflow_ai::agent::StreamEmitter;

use super::event_log::{AgentEvent, EventLog};

/// Wrapper that logs tool call events to an EventLog while forwarding to an inner emitter.
pub struct EventLoggingEmitter {
    inner: Box<dyn StreamEmitter>,
    event_log: Arc<Mutex<EventLog>>,
    task_id: String,
    current_step: u32,
    tool_start_times: HashMap<String, Instant>,
}

impl EventLoggingEmitter {
    /// Create a new EventLoggingEmitter.
    ///
    /// # Arguments
    /// * `inner` - The inner StreamEmitter to forward events to
    /// * `event_log` - The EventLog to write events to
    /// * `task_id` - The task ID for logging
    pub fn new(inner: Box<dyn StreamEmitter>, event_log: EventLog, task_id: String) -> Self {
        Self {
            inner,
            event_log: Arc::new(Mutex::new(event_log)),
            task_id,
            current_step: 0,
            tool_start_times: HashMap::new(),
        }
    }

    /// Create with pre-wrapped EventLog (for sharing across emitters).
    pub fn with_shared_log(
        inner: Box<dyn StreamEmitter>,
        event_log: Arc<Mutex<EventLog>>,
        task_id: String,
    ) -> Self {
        Self {
            inner,
            event_log,
            task_id,
            current_step: 0,
            tool_start_times: HashMap::new(),
        }
    }

    /// Log an event to the event log.
    fn log_event(&self, event: AgentEvent) {
        if let Ok(mut log) = self.event_log.lock() {
            let _ = log.append(&event);
        }
    }

    /// Get current step and increment.
    fn next_step(&mut self) -> u32 {
        self.current_step += 1;
        self.current_step
    }
}

#[async_trait]
impl StreamEmitter for EventLoggingEmitter {
    async fn emit_text_delta(&mut self, text: &str) {
        self.inner.emit_text_delta(text).await;
    }

    async fn emit_thinking_delta(&mut self, text: &str) {
        self.inner.emit_thinking_delta(text).await;
    }

    async fn emit_tool_call_start(&mut self, id: &str, name: &str, arguments: &str) {
        self.inner
            .emit_tool_call_start(id, name, arguments)
            .await;

        let step = self.next_step();
        self.tool_start_times.insert(id.to_string(), Instant::now());

        self.log_event(AgentEvent::ToolCallStarted {
            timestamp: Utc::now().timestamp_millis(),
            step,
            tool_name: name.to_string(),
            input: arguments.to_string(),
        });
    }

    async fn emit_tool_call_result(
        &mut self,
        id: &str,
        name: &str,
        result: &str,
        success: bool,
    ) {
        self.inner
            .emit_tool_call_result(id, name, result, success)
            .await;

        let duration_ms = self
            .tool_start_times
            .remove(id)
            .map(|start| start.elapsed().as_millis() as u64)
            .unwrap_or(0);

        self.log_event(AgentEvent::ToolCallCompleted {
            timestamp: Utc::now().timestamp_millis(),
            step: self.current_step,
            tool_name: name.to_string(),
            success,
            output: truncate_output(result, 10000),
            duration_ms,
        });
    }

    async fn emit_complete(&mut self) {
        self.inner.emit_complete().await;
    }
}

/// Truncate output to prevent log files from becoming too large.
fn truncate_output(output: &str, max_len: usize) -> String {
    if output.len() > max_len {
        format!("{}... [truncated, {} bytes]", &output[..max_len], output.len())
    } else {
        output.to_string()
    }
}
