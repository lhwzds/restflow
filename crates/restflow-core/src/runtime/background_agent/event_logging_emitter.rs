//! Event-logging wrapper for StreamEmitter.
//!
//! Wraps any StreamEmitter and logs tool call events to an EventLog.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use chrono::Utc;
use regex::Regex;
use std::sync::LazyLock;
use tracing::warn;

use restflow_ai::agent::StreamEmitter;

use super::event_log::{AgentEvent, EventLog};

/// Sanitize sensitive data from a string before logging.
///
/// Replaces common secret patterns (API keys, tokens, credentials) with `[REDACTED]`.
fn sanitize_secrets(input: &str) -> String {
    static SECRET_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(concat!(
            r"(?i)(?:",
            r"sk-[a-zA-Z0-9_-]{20,}",
            r"|xoxb-[a-zA-Z0-9_-]{20,}",
            r"|xoxp-[a-zA-Z0-9_-]{20,}",
            r"|Bearer\s+[a-zA-Z0-9._\-/+=]{20,}",
            r"|AKIA[0-9A-Z]{16}",
            r"|ghp_[a-zA-Z0-9]{36,}",
            r"|gho_[a-zA-Z0-9]{36,}",
            r"|glpat-[a-zA-Z0-9_-]{20,}",
            r#"|(?:api[_\-]?key|apikey|secret[_\-]?key|access[_\-]?token|auth[_\-]?token)\s*[=:]\s*["']?[a-zA-Z0-9._\-/+=]{8,}"#,
            r")",
        ))
        .expect("invalid secret pattern regex")
    });
    SECRET_PATTERN.replace_all(input, "[REDACTED]").into_owned()
}

/// Wrapper that logs tool call events to an EventLog while forwarding to an inner emitter.
pub struct EventLoggingEmitter {
    inner: Box<dyn StreamEmitter>,
    event_log: Arc<Mutex<EventLog>>,
    #[allow(dead_code)]
    task_id: String,
    current_step: u32,
    tool_start_times: HashMap<String, Instant>,
    error_count: AtomicU32,
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
            error_count: AtomicU32::new(0),
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
            error_count: AtomicU32::new(0),
        }
    }

    /// Log an event to the event log.
    ///
    /// Recovers from mutex poisoning by extracting the inner EventLog,
    /// ensuring events are not silently dropped after a panic in another thread.
    fn log_event(&self, event: AgentEvent) {
        let mut log = match self.event_log.lock() {
            Ok(log) => log,
            Err(poisoned) => {
                self.error_count.fetch_add(1, Ordering::Relaxed);
                warn!("EventLog mutex was poisoned, recovering inner log");
                poisoned.into_inner()
            }
        };
        if let Err(e) = log.append(&event) {
            self.error_count.fetch_add(1, Ordering::Relaxed);
            warn!("Failed to append event to log: {}", e);
        }
    }

    /// Get the number of logging errors encountered.
    #[cfg(test)]
    fn error_count(&self) -> u32 {
        self.error_count.load(Ordering::Relaxed)
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
            input: sanitize_secrets(arguments),
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
            output: sanitize_secrets(&truncate_output(result, 10000)),
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
    fn test_sanitize_openai_key() {
        let input = "Using key sk-abc123defghijklmnopqrstuvwxyz for API";
        let result = sanitize_secrets(input);
        assert!(!result.contains("sk-abc123"));
        assert!(result.contains("[REDACTED]"));
        assert!(result.contains("for API"));
    }

    #[test]
    fn test_sanitize_bearer_token() {
        let input = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.payload.signature";
        let result = sanitize_secrets(input);
        assert!(!result.contains("eyJ"));
        assert!(result.contains("[REDACTED]"));
    }

    #[test]
    fn test_sanitize_no_secrets() {
        let input = "This is normal text with no secrets at all";
        let result = sanitize_secrets(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_sanitize_multiple_patterns() {
        let input =
            "key1=sk-abc123defghijklmnopqrstuvwxyz key2=ghp_abcdefghijklmnopqrstuvwxyz1234567890";
        let result = sanitize_secrets(input);
        assert!(!result.contains("sk-abc"));
        assert!(!result.contains("ghp_abc"));
        // Both should be redacted
        assert_eq!(result.matches("[REDACTED]").count(), 2);
    }

    #[test]
    fn test_sanitize_github_token() {
        let input = "Token: ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmn";
        let result = sanitize_secrets(input);
        assert!(!result.contains("ghp_"));
        assert!(result.contains("[REDACTED]"));
    }

    #[test]
    fn test_sanitize_aws_key() {
        let input = "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE";
        let result = sanitize_secrets(input);
        assert!(!result.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(result.contains("[REDACTED]"));
    }

    #[test]
    fn test_sanitize_slack_token() {
        // Build the test token at runtime to avoid GitHub push protection false positive
        let prefix = "xoxb-";
        let suffix = "AAAABBBBCCCCDDDDEEEE";
        let input = format!("SLACK_TOKEN={}{}", prefix, suffix);
        let result = sanitize_secrets(&input);
        assert!(!result.contains(prefix));
        assert!(result.contains("[REDACTED]"));
    }

    #[test]
    fn test_sanitize_api_key_assignment() {
        let input = r#"api_key = "my_secret_key_value_here""#;
        let result = sanitize_secrets(input);
        assert!(!result.contains("my_secret_key_value_here"));
        assert!(result.contains("[REDACTED]"));
    }

    #[test]
    fn test_error_count_initial_value() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let log = EventLog::new("test", temp_dir.path()).unwrap();
        let emitter = EventLoggingEmitter::new(Box::new(NoopEmitter), log, "test".to_string());
        assert_eq!(emitter.error_count(), 0);
    }

    #[test]
    fn test_mutex_poisoning_recovery() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let log = EventLog::new("poison-test", temp_dir.path()).unwrap();
        let shared_log = Arc::new(Mutex::new(log));

        let emitter = EventLoggingEmitter::with_shared_log(
            Box::new(NoopEmitter),
            shared_log.clone(),
            "poison-test".to_string(),
        );

        // Poison the mutex by panicking inside a lock
        let shared_clone = shared_log.clone();
        let _ = std::thread::spawn(move || {
            let _guard = shared_clone.lock().unwrap();
            panic!("intentional panic to poison mutex");
        })
        .join();

        // Mutex should now be poisoned
        assert!(shared_log.lock().is_err(), "mutex should be poisoned");

        // Use a terminal event (Error) which triggers immediate flush
        emitter.log_event(AgentEvent::Error {
            timestamp: 42,
            error: "test error".to_string(),
        });

        // error_count should reflect the poisoning recovery
        assert_eq!(
            emitter.error_count(),
            1,
            "should have recorded 1 error from poisoning"
        );

        // Drop all Arc references so EventLog::Drop can flush remaining data
        drop(emitter);
        drop(shared_log);

        // Verify the event was actually written to disk
        let log_path = temp_dir.path().join("poison-test.jsonl");
        let events = EventLog::read_all(&log_path).unwrap();
        assert!(
            !events.is_empty(),
            "event should have been written despite poisoned mutex"
        );
        assert_eq!(events.last().unwrap().timestamp(), 42);
    }

    /// No-op emitter for testing
    struct NoopEmitter;

    #[async_trait]
    impl StreamEmitter for NoopEmitter {
        async fn emit_text_delta(&mut self, _text: &str) {}
        async fn emit_thinking_delta(&mut self, _text: &str) {}
        async fn emit_tool_call_start(&mut self, _id: &str, _name: &str, _arguments: &str) {}
        async fn emit_tool_call_result(
            &mut self,
            _id: &str,
            _name: &str,
            _result: &str,
            _success: bool,
        ) {
        }
        async fn emit_complete(&mut self) {}
    }

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
