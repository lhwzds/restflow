use crate::models::ChatExecutionEvent;
use crate::storage::ChatExecutionEventStorage;
use async_trait::async_trait;
use restflow_ai::agent::StreamEmitter;
use std::collections::HashMap;
use std::time::Instant;
use tracing::warn;

const MAX_EVENT_TEXT_CHARS: usize = 10_000;

fn truncate_text(value: &str, max_chars: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_chars {
        return value.to_string();
    }
    let mut truncated = value.chars().take(max_chars).collect::<String>();
    truncated.push_str("...");
    truncated
}

fn normalize_payload(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let normalized = match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(json) => json.to_string(),
        Err(_) => trimmed.to_string(),
    };
    Some(truncate_text(&normalized, MAX_EVENT_TEXT_CHARS))
}

/// Append turn-start event to chat execution event storage.
pub fn append_turn_started(storage: &ChatExecutionEventStorage, session_id: &str, turn_id: &str) {
    let event = ChatExecutionEvent::turn_started(session_id, turn_id);
    if let Err(error) = storage.append(&event) {
        warn!(
            session_id,
            turn_id,
            error = %error,
            "Failed to append turn started event"
        );
    }
}

/// Append turn-completed event to chat execution event storage.
pub fn append_turn_completed(storage: &ChatExecutionEventStorage, session_id: &str, turn_id: &str) {
    let event = ChatExecutionEvent::turn_completed(session_id, turn_id);
    if let Err(error) = storage.append(&event) {
        warn!(
            session_id,
            turn_id,
            error = %error,
            "Failed to append turn completed event"
        );
    }
}

/// Append turn-failed event to chat execution event storage.
pub fn append_turn_failed(
    storage: &ChatExecutionEventStorage,
    session_id: &str,
    turn_id: &str,
    error_text: &str,
) {
    let event = ChatExecutionEvent::turn_failed(
        session_id,
        turn_id,
        truncate_text(error_text, MAX_EVENT_TEXT_CHARS),
    );
    if let Err(error) = storage.append(&event) {
        warn!(
            session_id,
            turn_id,
            error = %error,
            "Failed to append turn failed event"
        );
    }
}

/// Append turn-cancelled event to chat execution event storage.
pub fn append_turn_cancelled(
    storage: &ChatExecutionEventStorage,
    session_id: &str,
    turn_id: &str,
    reason: &str,
) {
    let event = ChatExecutionEvent::turn_cancelled(
        session_id,
        turn_id,
        truncate_text(reason, MAX_EVENT_TEXT_CHARS),
    );
    if let Err(error) = storage.append(&event) {
        warn!(
            session_id,
            turn_id,
            error = %error,
            "Failed to append turn cancelled event"
        );
    }
}

/// Stream emitter that forwards events and persists tool call records to storage.
pub struct PersistingStreamEmitter {
    inner: Box<dyn StreamEmitter>,
    event_storage: ChatExecutionEventStorage,
    session_id: String,
    turn_id: String,
    tool_start_times: HashMap<String, Instant>,
}

impl PersistingStreamEmitter {
    /// Create a new persisting emitter.
    pub fn new(
        inner: Box<dyn StreamEmitter>,
        event_storage: ChatExecutionEventStorage,
        session_id: impl Into<String>,
        turn_id: impl Into<String>,
    ) -> Self {
        Self {
            inner,
            event_storage,
            session_id: session_id.into(),
            turn_id: turn_id.into(),
            tool_start_times: HashMap::new(),
        }
    }

    fn append_event(&self, event: ChatExecutionEvent) {
        if let Err(error) = self.event_storage.append(&event) {
            warn!(
                session_id = %self.session_id,
                turn_id = %self.turn_id,
                error = %error,
                "Failed to append chat execution event"
            );
        }
    }
}

#[async_trait]
impl StreamEmitter for PersistingStreamEmitter {
    async fn emit_text_delta(&mut self, text: &str) {
        self.inner.emit_text_delta(text).await;
    }

    async fn emit_thinking_delta(&mut self, text: &str) {
        self.inner.emit_thinking_delta(text).await;
    }

    async fn emit_tool_call_start(&mut self, id: &str, name: &str, arguments: &str) {
        self.inner.emit_tool_call_start(id, name, arguments).await;
        self.tool_start_times.insert(id.to_string(), Instant::now());
        let event = ChatExecutionEvent::tool_call_started(
            self.session_id.clone(),
            self.turn_id.clone(),
            id.to_string(),
            name.to_string(),
            normalize_payload(arguments),
        );
        self.append_event(event);
    }

    async fn emit_tool_call_result(&mut self, id: &str, name: &str, result: &str, success: bool) {
        self.inner
            .emit_tool_call_result(id, name, result, success)
            .await;

        let duration_ms = self
            .tool_start_times
            .remove(id)
            .map(|start| start.elapsed().as_millis() as u64);
        let output = normalize_payload(result);
        let error = if success {
            None
        } else {
            Some(
                output
                    .clone()
                    .unwrap_or_else(|| "tool call failed".to_string()),
            )
        };
        let event = ChatExecutionEvent::tool_call_completed(
            self.session_id.clone(),
            self.turn_id.clone(),
            id.to_string(),
            name.to_string(),
            output,
            success,
            duration_ms,
            error,
        );
        self.append_event(event);
    }

    async fn emit_complete(&mut self) {
        self.inner.emit_complete().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::Database;
    use restflow_ai::agent::NullEmitter;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn setup_storage() -> ChatExecutionEventStorage {
        let temp_dir = tempdir().expect("tempdir");
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).expect("db"));
        ChatExecutionEventStorage::new(db).expect("storage")
    }

    #[tokio::test]
    async fn test_persisting_stream_emitter_writes_tool_events() {
        let storage = setup_storage();
        let mut emitter = PersistingStreamEmitter::new(
            Box::new(NullEmitter),
            storage.clone(),
            "session-1",
            "turn-1",
        );

        emitter
            .emit_tool_call_start("call-1", "bash", "{\"cmd\":\"echo hi\"}")
            .await;
        emitter
            .emit_tool_call_result("call-1", "bash", "{\"ok\":true}", true)
            .await;

        let events = storage
            .list_by_session_turn("session-1", "turn-1", None)
            .expect("list");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].tool_name.as_deref(), Some("bash"));
        assert_eq!(events[1].success, Some(true));
    }
}
