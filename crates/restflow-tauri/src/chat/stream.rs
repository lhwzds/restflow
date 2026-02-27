//! Chat stream handler for managing streaming responses.
//!
//! Provides utilities for handling LLM stream chunks and emitting
//! Tauri events to the frontend.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use restflow_ai::agent::StreamEmitter;
use tauri::{AppHandle, Emitter};
use tokio::sync::broadcast;
use tracing::{debug, warn};

use super::events::{CHAT_STREAM_EVENT, ChatStreamEvent};

/// Handle for cancelling an active stream
#[derive(Debug, Clone)]
pub struct StreamCancelHandle {
    sender: broadcast::Sender<()>,
}

impl StreamCancelHandle {
    /// Create a new cancel handle
    pub fn new() -> (Self, StreamCancelReceiver) {
        let (sender, receiver) = broadcast::channel(1);
        (Self { sender }, StreamCancelReceiver { receiver })
    }

    /// Cancel the stream
    pub fn cancel(&self) {
        let _ = self.sender.send(());
    }
}

impl Default for StreamCancelHandle {
    fn default() -> Self {
        Self::new().0
    }
}

/// Receiver for stream cancellation
pub struct StreamCancelReceiver {
    receiver: broadcast::Receiver<()>,
}

impl StreamCancelReceiver {
    /// Check if cancellation was requested (non-blocking)
    pub fn is_cancelled(&mut self) -> bool {
        self.receiver.try_recv().is_ok()
    }

    /// Wait for cancellation
    pub async fn cancelled(&mut self) {
        let _ = self.receiver.recv().await;
    }
}

/// State for an active chat stream
pub struct ChatStreamState {
    /// Session ID
    pub session_id: String,
    /// Message ID being generated
    pub message_id: String,
    /// Model being used
    pub model: String,
    /// Accumulated content
    pub content: String,
    /// Approximate token count
    pub token_count: u32,
    /// Input tokens used
    pub input_tokens: u32,
    /// Output tokens generated
    pub output_tokens: u32,
    /// Stream start time
    pub start_time: Instant,
    /// Cancel handle
    cancel_handle: StreamCancelHandle,
    /// Tauri app handle for emitting events
    app_handle: AppHandle,
}

impl ChatStreamState {
    /// Create a new stream state
    pub fn new(
        app_handle: AppHandle,
        session_id: String,
        message_id: String,
        model: String,
    ) -> (Self, StreamCancelHandle) {
        let (cancel_handle, _receiver) = StreamCancelHandle::new();
        let cancel_clone = cancel_handle.clone();

        let state = Self {
            session_id,
            message_id,
            model,
            content: String::new(),
            token_count: 0,
            input_tokens: 0,
            output_tokens: 0,
            start_time: Instant::now(),
            cancel_handle,
            app_handle,
        };

        (state, cancel_clone)
    }

    /// Get a cancel receiver for this stream
    pub fn cancel_receiver(&self) -> StreamCancelReceiver {
        StreamCancelReceiver {
            receiver: self.cancel_handle.sender.subscribe(),
        }
    }

    /// Emit stream started event
    pub fn emit_started(&self) {
        self.emit_event(ChatStreamEvent::started(
            &self.session_id,
            &self.message_id,
            &self.model,
        ));
    }

    /// Emit a token event and accumulate content
    pub fn emit_token(&mut self, text: &str) {
        self.content.push_str(text);
        self.token_count += 1; // Approximate count
        self.emit_event(ChatStreamEvent::token(
            &self.session_id,
            &self.message_id,
            text,
            self.token_count,
        ));
    }

    /// Emit an acknowledgement event
    pub fn emit_acknowledgement(&self, content: &str) {
        self.emit_event(ChatStreamEvent::ack(
            &self.session_id,
            &self.message_id,
            content,
        ));
    }

    /// Emit a thinking event
    pub fn emit_thinking(&self, content: &str) {
        self.emit_event(ChatStreamEvent::thinking(
            &self.session_id,
            &self.message_id,
            content,
        ));
    }

    /// Emit a tool call start event
    pub fn emit_tool_call_start(&self, tool_id: &str, tool_name: &str, arguments: &str) {
        self.emit_event(ChatStreamEvent::tool_call_start(
            &self.session_id,
            &self.message_id,
            tool_id,
            tool_name,
            arguments,
        ));
    }

    /// Emit a tool call end event
    pub fn emit_tool_call_end(&self, tool_id: &str, result: &str, success: bool) {
        self.emit_event(ChatStreamEvent::tool_call_end(
            &self.session_id,
            &self.message_id,
            tool_id,
            result,
            success,
        ));
    }

    /// Update token usage
    pub fn update_usage(&mut self, input_tokens: u32, output_tokens: u32) {
        self.input_tokens = input_tokens;
        self.output_tokens = output_tokens;
        self.emit_event(ChatStreamEvent::usage(
            &self.session_id,
            &self.message_id,
            input_tokens,
            output_tokens,
        ));
    }

    /// Emit stream completed event
    pub fn emit_completed(&self) {
        let duration_ms = self.start_time.elapsed().as_millis() as u64;
        let total_tokens = if self.input_tokens + self.output_tokens > 0 {
            self.input_tokens + self.output_tokens
        } else {
            self.token_count
        };
        self.emit_event(ChatStreamEvent::completed(
            &self.session_id,
            &self.message_id,
            &self.content,
            duration_ms,
            total_tokens,
        ));
    }

    /// Emit stream failed event
    pub fn emit_failed(&self, error: &str) {
        let partial = if self.content.is_empty() {
            None
        } else {
            Some(self.content.clone())
        };
        self.emit_event(ChatStreamEvent::failed(
            &self.session_id,
            &self.message_id,
            error,
            partial,
        ));
    }

    /// Emit stream cancelled event
    pub fn emit_cancelled(&self) {
        let partial = if self.content.is_empty() {
            None
        } else {
            Some(self.content.clone())
        };
        self.emit_event(ChatStreamEvent::cancelled(
            &self.session_id,
            &self.message_id,
            partial,
        ));
    }

    /// Get duration since stream started
    pub fn duration_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }

    /// Get the accumulated content
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Get total tokens used
    pub fn total_tokens(&self) -> u32 {
        self.input_tokens + self.output_tokens
    }

    /// Emit a chat stream event
    fn emit_event(&self, event: ChatStreamEvent) {
        if let Err(e) = self.app_handle.emit(CHAT_STREAM_EVENT, &event) {
            warn!(error = %e, "Failed to emit chat stream event");
        } else {
            debug!(
                session_id = %self.session_id,
                message_id = %self.message_id,
                event_type = ?std::mem::discriminant(&event.kind),
                "Emitted chat stream event"
            );
        }
    }
}

#[async_trait]
impl StreamEmitter for ChatStreamState {
    async fn emit_text_delta(&mut self, text: &str) {
        self.emit_token(text);
    }

    async fn emit_thinking_delta(&mut self, text: &str) {
        self.emit_thinking(text);
    }

    async fn emit_tool_call_start(&mut self, id: &str, name: &str, arguments: &str) {
        ChatStreamState::emit_tool_call_start(self, id, name, arguments);
    }

    async fn emit_tool_call_result(&mut self, id: &str, _name: &str, result: &str, success: bool) {
        ChatStreamState::emit_tool_call_end(self, id, result, success);
    }

    async fn emit_complete(&mut self) {
        self.emit_completed();
    }
}

/// Manager for tracking active streams
pub struct StreamManager {
    /// Active stream cancel handles keyed by message_id
    active_streams: Arc<dashmap::DashMap<String, StreamCancelHandle>>,
}

impl StreamManager {
    /// Create a new stream manager
    pub fn new() -> Self {
        Self {
            active_streams: Arc::new(dashmap::DashMap::new()),
        }
    }

    /// Register a new stream
    pub fn register(&self, message_id: &str, cancel_handle: StreamCancelHandle) {
        self.active_streams
            .insert(message_id.to_string(), cancel_handle);
    }

    /// Cancel a stream by message ID
    pub fn cancel(&self, message_id: &str) -> bool {
        if let Some((_, handle)) = self.active_streams.remove(message_id) {
            handle.cancel();
            true
        } else {
            false
        }
    }

    /// Remove a stream from tracking (called when stream ends)
    pub fn remove(&self, message_id: &str) {
        self.active_streams.remove(message_id);
    }

    /// Check if a stream is active
    pub fn is_active(&self, message_id: &str) -> bool {
        self.active_streams.contains_key(message_id)
    }

    /// Get count of active streams
    pub fn active_count(&self) -> usize {
        self.active_streams.len()
    }
}

impl Default for StreamManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for StreamManager {
    fn clone(&self) -> Self {
        Self {
            active_streams: Arc::clone(&self.active_streams),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_cancel_handle() {
        let (handle, mut receiver) = StreamCancelHandle::new();
        assert!(!receiver.is_cancelled());
        handle.cancel();
        assert!(receiver.is_cancelled());
    }

    #[test]
    fn test_stream_manager() {
        let manager = StreamManager::new();
        let (handle, _) = StreamCancelHandle::new();

        manager.register("msg-1", handle);
        assert!(manager.is_active("msg-1"));
        assert_eq!(manager.active_count(), 1);

        assert!(manager.cancel("msg-1"));
        assert!(!manager.is_active("msg-1"));
        assert_eq!(manager.active_count(), 0);
    }
}
