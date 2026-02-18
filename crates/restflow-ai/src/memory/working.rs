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

use crate::error::Result;
use crate::llm::{LlmClient, Message};

use super::compaction::{CompactionConfig, CompactionResult, ContextCompactor};

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
    /// Optional compaction configuration
    compaction_config: Option<CompactionConfig>,
    /// Last compaction result
    last_compaction: Option<CompactionResult>,
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
        let max_messages = max_messages.max(1);
        Self {
            messages: VecDeque::with_capacity(max_messages),
            max_messages,
            token_count: 0,
            compaction_config: None,
            last_compaction: None,
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
            // Special case: max_messages == 1 cannot maintain capacity
            // without losing the only slot. Always evict when at capacity=1.
            if self.max_messages == 1 {
                // With max_messages == 1, we must evict the existing message
                // to maintain the capacity invariant. This is true regardless
                // of whether the incoming message is system or non-system.
                if let Some(removed) = self.messages.pop_front() {
                    self.token_count = self
                        .token_count
                        .saturating_sub(Self::estimate_tokens(&removed));
                }
                break;
            }

            if let Some(removed) = self.remove_oldest_non_system() {
                self.token_count = self
                    .token_count
                    .saturating_sub(Self::estimate_tokens(&removed));
            } else {
                // All messages are system messages - rare edge case
                // When max_messages > 1, we can safely remove one system message
                // to make room.
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

    /// Enable context compaction with the provided configuration.
    pub fn enable_compaction(&mut self, config: CompactionConfig) {
        self.compaction_config = Some(config);
    }

    /// Get the last compaction result, if any.
    pub fn last_compaction(&self) -> Option<&CompactionResult> {
        self.last_compaction.as_ref()
    }

    /// Check and perform auto-compaction if needed.
    pub async fn auto_compact_if_needed(
        &mut self,
        llm: &dyn LlmClient,
        context_window: usize,
    ) -> Result<Option<CompactionResult>> {
        let Some(config) = &self.compaction_config else {
            return Ok(None);
        };

        if !config.auto_compact {
            return Ok(None);
        }

        let compactor = ContextCompactor::new(config.clone());
        let messages = self.get_messages();
        if !compactor.needs_compaction(&messages, context_window) {
            return Ok(None);
        }

        let result = compactor.compact(messages, llm).await?;
        self.replace_history(&result.new_history);
        self.last_compaction = Some(result.clone());
        Ok(Some(result))
    }

    /// Manually trigger compaction using the current configuration.
    pub async fn compact(&mut self, llm: &dyn LlmClient) -> Result<CompactionResult> {
        let config = self.compaction_config.clone().unwrap_or_default();
        let compactor = ContextCompactor::new(config);
        let messages = self.get_messages();
        let result = compactor.compact(messages, llm).await?;
        self.replace_history(&result.new_history);
        self.last_compaction = Some(result.clone());
        Ok(result)
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
        self.last_compaction = None;
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

    fn replace_history(&mut self, messages: &[Message]) {
        self.messages.clear();
        for msg in messages {
            self.messages.push_back(msg.clone());
        }
        self.recalculate_tokens();
    }

    fn recalculate_tokens(&mut self) {
        self.token_count = self.messages.iter().map(Self::estimate_tokens).sum();
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
    fn test_zero_capacity_clamped_to_one() {
        let mut memory = WorkingMemory::new(0);
        assert_eq!(memory.max_messages(), 1);

        memory.add(Message::user("Hello"));
        assert_eq!(memory.len(), 1);

        // Adding another message evicts the first
        memory.add(Message::user("World"));
        assert_eq!(memory.len(), 1);
        assert_eq!(memory.get_messages()[0].content, "World");
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

    #[test]
    fn test_max_messages_one_system_then_user() {
        // This is the specific bug case from the issue:
        // max_messages == 1, add system, then add user
        // The system message should be evicted to make room for user
        let mut memory = WorkingMemory::new(1);

        memory.add(Message::system("policy"));
        assert_eq!(memory.len(), 1);
        assert_eq!(memory.get_messages()[0].role, Role::System);

        memory.add(Message::user("hello"));
        assert_eq!(memory.len(), 1);
        // User message should now be present (system was evicted)
        assert_eq!(memory.get_messages()[0].role, Role::User);
        assert_eq!(memory.get_messages()[0].content, "hello");
    }

    #[test]
    fn test_max_messages_one_user_then_system() {
        // Edge case: add user first, then system
        let mut memory = WorkingMemory::new(1);

        memory.add(Message::user("hello"));
        assert_eq!(memory.len(), 1);
        assert_eq!(memory.get_messages()[0].role, Role::User);

        // Adding system should replace user
        memory.add(Message::system("policy"));
        assert_eq!(memory.len(), 1);
        assert_eq!(memory.get_messages()[0].role, Role::System);
        assert_eq!(memory.get_messages()[0].content, "policy");
    }

    #[test]
    fn test_max_messages_one_two_systems() {
        // Edge case: add two system messages with max=1
        let mut memory = WorkingMemory::new(1);

        memory.add(Message::system("policy1"));
        assert_eq!(memory.len(), 1);

        memory.add(Message::system("policy2"));
        assert_eq!(memory.len(), 1);
        // Second system replaces first
        assert_eq!(memory.get_messages()[0].content, "policy2");
    }
}
