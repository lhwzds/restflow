//! Working Memory - Sliding window for conversation history
//!
//! Provides a bounded buffer for storing conversation messages with automatic
//! eviction of oldest messages when the limit is reached. Designed for runtime
//! use with LLM conversations to prevent context overflow.
//!
//! # Example
//!
//! ```
//! use restflow_ai::memory::WorkingMemory;
//! use restflow_ai::llm::Message;
//!
//! let mut memory = WorkingMemory::new(100);
//! memory.add(Message::user("Hello"));
//! memory.add(Message::assistant("Hi there!"));
//!
//! assert_eq!(memory.len(), 2);
//! let messages = memory.get_messages();
//! ```

use std::collections::VecDeque;

use crate::llm::Message;

/// Default maximum number of messages in working memory
pub const DEFAULT_MAX_MESSAGES: usize = 100;

/// Working memory for conversation history with sliding window
///
/// Stores messages in a bounded buffer that automatically evicts the oldest
/// messages when the limit is reached. This prevents context overflow while
/// maintaining recent conversation history.
///
/// # Design Decisions
///
/// - **No LLM summarization**: When messages are evicted, they are simply discarded.
///   If important context needs to be preserved, use the file externalization tool
///   to save it before it gets evicted.
/// - **System messages preserved**: The first system message is never evicted as it
///   typically contains the core instructions for the agent.
/// - **Configurable limit**: Different tasks may need different history lengths.
#[derive(Debug, Clone)]
pub struct WorkingMemory {
    /// Messages stored in order (oldest first)
    messages: VecDeque<Message>,
    /// Maximum number of messages to retain
    max_messages: usize,
    /// Approximate token count (estimated as chars / 4)
    token_count: usize,
}

impl Default for WorkingMemory {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_MESSAGES)
    }
}

impl WorkingMemory {
    /// Create a new working memory with the specified maximum message count
    ///
    /// # Arguments
    ///
    /// * `max_messages` - Maximum number of messages to retain. When this limit
    ///   is reached, the oldest non-system messages are evicted.
    ///
    /// # Example
    ///
    /// ```
    /// use restflow_ai::memory::WorkingMemory;
    ///
    /// let memory = WorkingMemory::new(50);
    /// assert_eq!(memory.max_messages(), 50);
    /// ```
    pub fn new(max_messages: usize) -> Self {
        Self {
            messages: VecDeque::with_capacity(max_messages),
            max_messages,
            token_count: 0,
        }
    }

    /// Add a message to working memory
    ///
    /// If the memory is at capacity, the oldest non-system message will be
    /// removed to make room for the new message.
    ///
    /// # Arguments
    ///
    /// * `msg` - The message to add
    pub fn add(&mut self, msg: Message) {
        // Estimate tokens (rough approximation: 1 token ≈ 4 chars)
        let msg_tokens = Self::estimate_tokens(&msg);

        // If at capacity, remove oldest non-system message
        while self.messages.len() >= self.max_messages {
            if let Some(removed) = self.remove_oldest_non_system() {
                self.token_count = self
                    .token_count
                    .saturating_sub(Self::estimate_tokens(&removed));
            } else {
                // All messages are system messages - rare edge case
                // Just remove the oldest one
                if let Some(removed) = self.messages.pop_front() {
                    self.token_count = self
                        .token_count
                        .saturating_sub(Self::estimate_tokens(&removed));
                }
                break;
            }
        }

        self.token_count += msg_tokens;
        self.messages.push_back(msg);
    }

    /// Get all messages as a vector
    ///
    /// Returns messages in order from oldest to newest.
    pub fn get_messages(&self) -> Vec<Message> {
        self.messages.iter().cloned().collect()
    }

    /// Get messages as a slice reference
    ///
    /// Returns a pair of slices since VecDeque uses a ring buffer internally.
    /// Use `get_messages()` if you need a contiguous Vec.
    pub fn as_slices(&self) -> (&[Message], &[Message]) {
        self.messages.as_slices()
    }

    /// Clear all messages from memory
    pub fn clear(&mut self) {
        self.messages.clear();
        self.token_count = 0;
    }

    /// Get the number of messages currently stored
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Check if memory is empty
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Get the maximum message limit
    pub fn max_messages(&self) -> usize {
        self.max_messages
    }

    /// Get the approximate token count
    ///
    /// This is an estimate based on character count (chars / 4).
    /// For accurate token counting, use a proper tokenizer.
    pub fn token_count(&self) -> usize {
        self.token_count
    }

    /// Get remaining capacity (messages that can be added before eviction starts)
    pub fn remaining_capacity(&self) -> usize {
        self.max_messages.saturating_sub(self.messages.len())
    }

    /// Check if memory is at capacity
    pub fn is_full(&self) -> bool {
        self.messages.len() >= self.max_messages
    }

    /// Get the last N messages
    ///
    /// Returns up to `n` messages from the end (most recent).
    pub fn last_n(&self, n: usize) -> Vec<Message> {
        let start = self.messages.len().saturating_sub(n);
        self.messages.iter().skip(start).cloned().collect()
    }

    /// Remove oldest non-system message
    ///
    /// Returns the removed message, or None if all messages are system messages.
    fn remove_oldest_non_system(&mut self) -> Option<Message> {
        // Find the index of the first non-system message
        let idx = self
            .messages
            .iter()
            .position(|m| !matches!(m.role, crate::llm::Role::System));

        if let Some(idx) = idx {
            self.messages.remove(idx)
        } else {
            None
        }
    }

    /// Estimate token count for a message
    ///
    /// Uses a simple heuristic: 1 token ≈ 4 characters.
    fn estimate_tokens(msg: &Message) -> usize {
        let content_len = msg.content.len();
        let tool_call_len = msg
            .tool_calls
            .as_ref()
            .map(|calls| {
                calls
                    .iter()
                    .map(|c| c.name.len() + c.arguments.to_string().len())
                    .sum::<usize>()
            })
            .unwrap_or(0);

        (content_len + tool_call_len) / 4 + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{Message, Role, ToolCall};
    use serde_json::json;

    #[test]
    fn test_new_memory() {
        let memory = WorkingMemory::new(50);
        assert_eq!(memory.max_messages(), 50);
        assert_eq!(memory.len(), 0);
        assert!(memory.is_empty());
        assert!(!memory.is_full());
    }

    #[test]
    fn test_default_memory() {
        let memory = WorkingMemory::default();
        assert_eq!(memory.max_messages(), DEFAULT_MAX_MESSAGES);
    }

    #[test]
    fn test_add_and_get_messages() {
        let mut memory = WorkingMemory::new(100);

        memory.add(Message::user("Hello"));
        memory.add(Message::assistant("Hi there!"));

        assert_eq!(memory.len(), 2);
        let messages = memory.get_messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, Role::User);
        assert_eq!(messages[0].content, "Hello");
        assert_eq!(messages[1].role, Role::Assistant);
        assert_eq!(messages[1].content, "Hi there!");
    }

    #[test]
    fn test_sliding_window_eviction() {
        let mut memory = WorkingMemory::new(3);

        memory.add(Message::user("Message 1"));
        memory.add(Message::user("Message 2"));
        memory.add(Message::user("Message 3"));

        assert_eq!(memory.len(), 3);
        assert!(memory.is_full());

        // Add one more - should evict the oldest
        memory.add(Message::user("Message 4"));

        assert_eq!(memory.len(), 3);
        let messages = memory.get_messages();
        assert_eq!(messages[0].content, "Message 2");
        assert_eq!(messages[1].content, "Message 3");
        assert_eq!(messages[2].content, "Message 4");
    }

    #[test]
    fn test_system_message_preserved() {
        let mut memory = WorkingMemory::new(3);

        memory.add(Message::system("You are a helpful assistant"));
        memory.add(Message::user("Hello"));
        memory.add(Message::assistant("Hi!"));

        // At capacity, add one more
        memory.add(Message::user("How are you?"));

        // System message should be preserved, user message evicted
        let messages = memory.get_messages();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, Role::System);
        assert_eq!(messages[0].content, "You are a helpful assistant");
    }

    #[test]
    fn test_clear() {
        let mut memory = WorkingMemory::new(100);

        memory.add(Message::user("Hello"));
        memory.add(Message::assistant("Hi!"));
        assert_eq!(memory.len(), 2);

        memory.clear();

        assert_eq!(memory.len(), 0);
        assert!(memory.is_empty());
        assert_eq!(memory.token_count(), 0);
    }

    #[test]
    fn test_token_count_estimation() {
        let mut memory = WorkingMemory::new(100);

        // "Hello" = 5 chars ≈ 2 tokens (5/4 + 1)
        memory.add(Message::user("Hello"));
        assert!(memory.token_count() > 0);

        let initial_count = memory.token_count();

        // Add longer message
        memory.add(Message::assistant("This is a longer response message"));
        assert!(memory.token_count() > initial_count);
    }

    #[test]
    fn test_last_n() {
        let mut memory = WorkingMemory::new(100);

        memory.add(Message::user("One"));
        memory.add(Message::user("Two"));
        memory.add(Message::user("Three"));
        memory.add(Message::user("Four"));

        let last_two = memory.last_n(2);
        assert_eq!(last_two.len(), 2);
        assert_eq!(last_two[0].content, "Three");
        assert_eq!(last_two[1].content, "Four");

        // Request more than available
        let last_ten = memory.last_n(10);
        assert_eq!(last_ten.len(), 4);
    }

    #[test]
    fn test_remaining_capacity() {
        let mut memory = WorkingMemory::new(5);

        assert_eq!(memory.remaining_capacity(), 5);

        memory.add(Message::user("One"));
        memory.add(Message::user("Two"));

        assert_eq!(memory.remaining_capacity(), 3);
    }

    #[test]
    fn test_tool_call_token_estimation() {
        let mut memory = WorkingMemory::new(100);

        let tool_calls = vec![ToolCall {
            id: "call_123".to_string(),
            name: "search".to_string(),
            arguments: json!({"query": "rust programming"}),
        }];

        memory.add(Message::assistant_with_tool_calls(
            Some("Let me search that for you".to_string()),
            tool_calls,
        ));

        // Token count should include tool call info
        assert!(memory.token_count() > 5);
    }

    #[test]
    fn test_multiple_evictions() {
        let mut memory = WorkingMemory::new(2);

        memory.add(Message::system("System"));
        memory.add(Message::user("User 1"));

        // Add multiple messages, triggering multiple evictions
        memory.add(Message::user("User 2"));
        memory.add(Message::user("User 3"));
        memory.add(Message::user("User 4"));

        let messages = memory.get_messages();
        assert_eq!(messages.len(), 2);

        // System should be preserved
        assert_eq!(messages[0].role, Role::System);
        // Most recent user message
        assert_eq!(messages[1].content, "User 4");
    }

    #[test]
    fn test_as_slices() {
        let mut memory = WorkingMemory::new(100);

        memory.add(Message::user("One"));
        memory.add(Message::user("Two"));

        let (slice1, slice2) = memory.as_slices();
        let total = slice1.len() + slice2.len();
        assert_eq!(total, 2);
    }
}
