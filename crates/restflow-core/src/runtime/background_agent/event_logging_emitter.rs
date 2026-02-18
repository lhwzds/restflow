//! Event-logging wrapper for StreamEmitter.
//!
//! Wraps any StreamEmitter and logs tool call events to an EventLog.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use chrono::Utc;
use tracing::warn;

use restflow_ai::agent::StreamEmitter;

use super::event_log::{AgentEvent, EventLog};

/// Wrapper that logs tool call events to an EventLog while forwarding to an inner emitter.
pub struct EventLoggingEmitter {
    inner: Box<dyn StreamEmitter>,
    event_log: Arc<Mutex<EventLog>>,
    #[allow(dead_code)]
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
        match self.event_log.lock() {
            Ok(mut log) => {
                if let Err(e) = log.append(&event) {
                    warn!("Failed to append event to log: {}", e);
                }
            }
            Err(e) => {
                warn!("EventLog mutex poisoned, cannot log event: {}", e);
            }
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
        self.inner.emit_tool_call_start(id, name, arguments).await;

        let step = self.next_step();
        self.tool_start_times.insert(id.to_string(), Instant::now());

        self.log_event(AgentEvent::ToolCallStarted {
            timestamp: Utc::now().timestamp_millis(),
            step,
            tool_name: name.to_string(),
            input: arguments.to_string(),
        });
    }

    async fn emit_tool_call_result(&mut self, id: &str, name: &str, result: &str, success: bool) {
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
/// Safe for UTF-8: will not panic on multi-byte character boundaries.
fn truncate_output(output: &str, max_len: usize) -> String {
    if output.len() > max_len {
        // Find a safe truncation point that doesn't split a multi-byte character
        let truncate_at = output
            .char_indices()
            .take_while(|(idx, _)| *idx < max_len)
            .last()
            .map(|(idx, c)| idx + c.len_utf8())
            .unwrap_or(0);
        format!(
            "{}... [truncated, {} bytes]",
            &output[..truncate_at],
            output.len()
        )
    } else {
        output.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_output_ascii() {
        let output = "hello world";
        assert_eq!(truncate_output(output, 100), output.to_string());
    }

    #[test]
    fn test_truncate_output_exact_boundary() {
        let output = "hello world";
        assert_eq!(truncate_output(output, 5), "hello... [truncated, 11 bytes]");
    }

    #[test]
    fn test_truncate_output_multibyte() {
        // Chinese characters: each is 3 bytes in UTF-8
        let output = "你好世界hello";
        // "你好" = 6 bytes, "世" starts at byte 6
        let result = truncate_output(output, 7);
        // Should truncate at byte 6 (end of "好"), not 7 (middle of "世")
        assert!(result.starts_with("你好"));
        assert!(result.contains("[truncated"));
    }

    #[test]
    fn test_truncate_output_emoji() {
        // Emoji: 4 bytes in UTF-8
        let output = "😀😁😂😃";
        let result = truncate_output(output, 5);
        // Should truncate at byte 4 (end of first emoji), not 5
        assert!(result.starts_with("😀"));
        assert!(result.contains("[truncated"));
    }

    #[test]
    fn test_truncate_output_empty() {
        assert_eq!(truncate_output("", 10), "");
    }

    #[test]
    fn test_truncate_output_single_multibyte() {
        // Single Chinese character (3 bytes)
        let output = "你";
        // Try to truncate at 1 byte (middle of character)
        let result = truncate_output(output, 1);
        // When max_len is less than the first character, we truncate to 0 chars
        // The result should be just the truncation message
        assert!(result.contains("[truncated"));
        assert!(result.ends_with("3 bytes]"));
    }
}
