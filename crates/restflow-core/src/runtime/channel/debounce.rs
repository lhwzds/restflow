//! Message Debouncer - Combines rapid sequential messages.
//!
//! When users send multiple short messages in quick succession, this module
//! combines them into a single input before processing. This improves UX
//! by treating rapid messages as a single thought.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};
use tokio::time::{Duration, sleep};
use tracing::debug;

/// Pending message buffer for a conversation.
struct PendingBuffer {
    messages: Vec<String>,
    notify: Arc<Notify>,
}

impl PendingBuffer {
    fn new() -> Self {
        Self {
            messages: Vec::new(),
            notify: Arc::new(Notify::new()),
        }
    }
}

/// Message debouncer that combines rapid sequential messages.
///
/// When a message arrives, it starts a timer. If more messages arrive
/// within the timeout window, they are combined. Once the timeout expires
/// without new messages, the combined text is returned.
pub struct MessageDebouncer {
    /// Pending messages per conversation
    pending: Arc<Mutex<HashMap<String, PendingBuffer>>>,
    /// Debounce timeout duration
    timeout: Duration,
}

impl MessageDebouncer {
    /// Create a new MessageDebouncer with the specified timeout.
    pub fn new(timeout: Duration) -> Self {
        Self {
            pending: Arc::new(Mutex::new(HashMap::new())),
            timeout,
        }
    }

    /// Create a debouncer with the default timeout (800ms).
    pub fn default_timeout() -> Self {
        Self::new(Duration::from_millis(800))
    }

    /// Add a message and wait for the debounce window to complete.
    ///
    /// Returns the combined text of all messages received within the window.
    /// Only the first caller for a conversation will receive the combined result;
    /// subsequent callers within the window will receive None.
    pub async fn debounce(&self, conversation_id: &str, text: &str) -> Option<String> {
        let (is_first, notify) = {
            let mut pending = self.pending.lock().await;
            let buffer = pending
                .entry(conversation_id.to_string())
                .or_insert_with(PendingBuffer::new);

            let is_first = buffer.messages.is_empty();
            buffer.messages.push(text.to_string());
            let notify = buffer.notify.clone();

            // Notify any waiting tasks that a new message arrived
            notify.notify_waiters();

            (is_first, notify)
        };

        if !is_first {
            // Not the first message in this batch; the first caller will handle it
            debug!(
                "Debounce: additional message for {}, delegating to first caller",
                conversation_id
            );
            return None;
        }

        // First message - wait for debounce window
        debug!(
            "Debounce: starting window for {} (timeout={:?})",
            conversation_id, self.timeout
        );

        loop {
            // Wait for timeout or new message notification
            tokio::select! {
                _ = sleep(self.timeout) => {
                    // Timeout expired, collect and return messages
                    break;
                }
                _ = notify.notified() => {
                    // New message arrived, reset the timer by continuing the loop
                    debug!("Debounce: timer reset for {}", conversation_id);
                    continue;
                }
            }
        }

        // Collect all pending messages
        let mut pending = self.pending.lock().await;
        if let Some(buffer) = pending.remove(conversation_id) {
            let combined = buffer.messages.join("\n");
            debug!(
                "Debounce: collected {} messages for {}",
                buffer.messages.len(),
                conversation_id
            );
            Some(combined)
        } else {
            None
        }
    }

    /// Get the current timeout duration.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Check if there are pending messages for a conversation.
    pub async fn has_pending(&self, conversation_id: &str) -> bool {
        let pending = self.pending.lock().await;
        pending
            .get(conversation_id)
            .map(|b| !b.messages.is_empty())
            .unwrap_or(false)
    }

    /// Clear pending messages for a conversation (e.g., on error).
    pub async fn clear(&self, conversation_id: &str) {
        let mut pending = self.pending.lock().await;
        pending.remove(conversation_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_single_message() {
        let debouncer = MessageDebouncer::new(Duration::from_millis(50));

        let result = debouncer.debounce("chat-1", "Hello").await;
        assert_eq!(result, Some("Hello".to_string()));
    }

    #[tokio::test]
    async fn test_multiple_messages_combined() {
        let debouncer = Arc::new(MessageDebouncer::new(Duration::from_millis(100)));

        let debouncer1 = debouncer.clone();
        let debouncer2 = debouncer.clone();

        // Spawn two message sends in quick succession
        let handle1 = tokio::spawn(async move { debouncer1.debounce("chat-1", "Hello").await });

        // Small delay to ensure ordering
        sleep(Duration::from_millis(10)).await;

        let handle2 = tokio::spawn(async move { debouncer2.debounce("chat-1", "World").await });

        let result1 = handle1.await.unwrap();
        let result2 = handle2.await.unwrap();

        // First caller should get combined result
        assert!(result1.is_some());
        let combined = result1.unwrap();
        assert!(combined.contains("Hello"));
        assert!(combined.contains("World"));

        // Second caller should get None
        assert!(result2.is_none());
    }

    #[tokio::test]
    async fn test_different_conversations_independent() {
        let debouncer = Arc::new(MessageDebouncer::new(Duration::from_millis(50)));

        let debouncer1 = debouncer.clone();
        let debouncer2 = debouncer.clone();

        let handle1 = tokio::spawn(async move { debouncer1.debounce("chat-1", "Message 1").await });

        let handle2 = tokio::spawn(async move { debouncer2.debounce("chat-2", "Message 2").await });

        let result1 = handle1.await.unwrap();
        let result2 = handle2.await.unwrap();

        // Both should get their respective messages
        assert_eq!(result1, Some("Message 1".to_string()));
        assert_eq!(result2, Some("Message 2".to_string()));
    }

    #[tokio::test]
    async fn test_clear_pending() {
        let debouncer = MessageDebouncer::new(Duration::from_millis(500));

        // Start debouncing but don't wait for it
        let debouncer_clone = debouncer.pending.clone();
        {
            let mut pending = debouncer_clone.lock().await;
            let buffer = pending
                .entry("chat-1".to_string())
                .or_insert_with(PendingBuffer::new);
            buffer.messages.push("test".to_string());
        }

        assert!(debouncer.has_pending("chat-1").await);

        debouncer.clear("chat-1").await;

        assert!(!debouncer.has_pending("chat-1").await);
    }

    #[test]
    fn test_default_timeout() {
        let debouncer = MessageDebouncer::default_timeout();
        assert_eq!(debouncer.timeout(), Duration::from_millis(800));
    }
}
