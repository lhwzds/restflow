//! Memory persistence for agent task execution.
//!
//! This module provides functionality to persist working memory to long-term
//! storage after task completion. Messages from the conversation are chunked
//! and stored for later retrieval and search.
//!
//! # Architecture
//!
//! ```text
//! Task Completion
//!        │
//!        ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                     MemoryPersister                         │
//! ├─────────────────────────────────────────────────────────────┤
//! │  1. Format messages → conversation text                     │
//! │  2. Chunk text → MemoryChunk[]                              │
//! │  3. Create MemorySession for the task execution             │
//! │  4. Store chunks (with deduplication)                       │
//! │  5. Update session stats                                    │
//! └─────────────────────────────────────────────────────────────┘
//!        │
//!        ▼
//!    Long-term Memory (redb)
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use restflow_tauri::agent_task::MemoryPersister;
//!
//! let persister = MemoryPersister::new(memory_storage);
//! let result = persister.persist(
//!     &messages,
//!     "agent-123",
//!     "task-456",
//!     &["automation", "daily-report"],
//! ).await?;
//!
//! println!("Persisted {} chunks in session {}", result.chunk_count, result.session_id);
//! ```

use anyhow::Result;
use chrono::Utc;
use restflow_ai::llm::{Message, Role};
use restflow_core::memory::TextChunker;
use restflow_core::models::memory::{MemorySession, MemorySource};
use restflow_core::storage::MemoryStorage;
use tracing::{debug, info};

/// Result of memory persistence operation.
#[derive(Debug, Clone)]
pub struct PersistResult {
    /// ID of the created memory session
    pub session_id: String,
    /// Number of chunks created
    pub chunk_count: usize,
    /// Number of chunks that were deduplicated (already existed)
    pub deduplicated_count: usize,
    /// Total tokens stored (estimated)
    pub total_tokens: usize,
}

/// Configuration for memory persistence.
#[derive(Debug, Clone)]
pub struct PersistConfig {
    /// Chunk size in characters (~4 chars per token)
    pub chunk_size: usize,
    /// Overlap between chunks in characters
    pub chunk_overlap: usize,
    /// Minimum conversation length to persist (in characters)
    pub min_content_length: usize,
    /// Whether to include system messages in persisted content
    pub include_system_messages: bool,
}

impl Default for PersistConfig {
    fn default() -> Self {
        Self {
            chunk_size: 1600,               // ~400 tokens
            chunk_overlap: 320,             // ~80 tokens overlap
            min_content_length: 100,        // Minimum 100 chars to persist
            include_system_messages: false, // System prompts often repeated, skip by default
        }
    }
}

impl PersistConfig {
    /// Create a config that includes system messages.
    pub fn with_system_messages(mut self) -> Self {
        self.include_system_messages = true;
        self
    }

    /// Set custom chunk size.
    pub fn with_chunk_size(mut self, size: usize) -> Self {
        self.chunk_size = size;
        self
    }

    /// Set custom overlap.
    pub fn with_overlap(mut self, overlap: usize) -> Self {
        self.chunk_overlap = overlap;
        self
    }
}

/// Handles persistence of conversation memory to long-term storage.
///
/// Converts working memory (conversation messages) into chunked long-term
/// memory storage, with automatic deduplication to prevent storing the
/// same content multiple times.
#[derive(Clone)]
pub struct MemoryPersister {
    storage: MemoryStorage,
    config: PersistConfig,
}

impl MemoryPersister {
    /// Create a new MemoryPersister with default configuration.
    pub fn new(storage: MemoryStorage) -> Self {
        Self {
            storage,
            config: PersistConfig::default(),
        }
    }

    /// Create a new MemoryPersister with custom configuration.
    pub fn with_config(storage: MemoryStorage, config: PersistConfig) -> Self {
        Self { storage, config }
    }

    /// Persist conversation messages to long-term memory.
    ///
    /// This method:
    /// 1. Formats the messages into a readable conversation transcript
    /// 2. Creates a memory session to group the chunks
    /// 3. Chunks the content and stores each chunk
    /// 4. Handles deduplication (chunks with same content hash are skipped)
    ///
    /// # Arguments
    ///
    /// * `messages` - The conversation messages to persist
    /// * `agent_id` - The agent that ran the conversation
    /// * `task_id` - The task that triggered the execution
    /// * `task_name` - Human-readable task name for the session
    /// * `tags` - Optional tags to associate with the memory
    ///
    /// # Returns
    ///
    /// A `PersistResult` containing statistics about the operation.
    pub fn persist(
        &self,
        messages: &[Message],
        agent_id: &str,
        task_id: &str,
        task_name: &str,
        tags: &[String],
    ) -> Result<PersistResult> {
        // Format messages into conversation text
        let conversation_text = self.format_conversation(messages);

        // Check minimum content length
        if conversation_text.len() < self.config.min_content_length {
            debug!(
                "Conversation too short ({} chars), skipping persistence",
                conversation_text.len()
            );
            return Ok(PersistResult {
                session_id: String::new(),
                chunk_count: 0,
                deduplicated_count: 0,
                total_tokens: 0,
            });
        }

        // Create memory session
        let session_name = format!("Task: {}", task_name);
        let session_desc = format!(
            "Conversation from task execution at {}",
            Utc::now().format("%Y-%m-%d %H:%M UTC")
        );

        let session = MemorySession::new(agent_id.to_string(), session_name)
            .with_description(session_desc)
            .with_tags(tags.to_vec());

        // Store session first
        self.storage.create_session(&session)?;
        let session_id = session.id.clone();

        info!(
            "Created memory session '{}' for task '{}' (agent: {})",
            session_id, task_name, agent_id
        );

        // Create chunker with config
        let chunker = TextChunker::default()
            .with_chunk_size(self.config.chunk_size)
            .with_overlap(self.config.chunk_overlap);

        // Chunk the conversation
        let source = MemorySource::TaskExecution {
            task_id: task_id.to_string(),
        };
        let chunks = chunker.chunk(&conversation_text, agent_id, Some(&session_id), source);

        let _total_chunks = chunks.len();
        let mut stored_count = 0;
        let mut dedup_count = 0;
        let mut total_tokens = 0;

        // Store each chunk
        for mut chunk in chunks {
            // Add tags to chunk
            chunk.tags.extend(tags.iter().cloned());

            // Estimate tokens (token_count is Option<u32>)
            if let Some(tokens) = chunk.token_count {
                total_tokens += tokens as usize;
            }

            // Store chunk (returns existing ID if duplicate)
            let stored_id = self.storage.store_chunk(&chunk)?;

            if stored_id == chunk.id {
                stored_count += 1;
                debug!(
                    "Stored chunk {} ({} tokens)",
                    chunk.id,
                    chunk.token_count.unwrap_or(0)
                );
            } else {
                dedup_count += 1;
                debug!("Chunk deduplicated - existing chunk {} found", stored_id);
            }
        }

        // Refresh session stats to update chunk count and token totals
        self.storage.refresh_session_stats(&session_id)?;

        info!(
            "Persisted {} chunks ({} deduplicated) for session '{}', ~{} tokens",
            stored_count, dedup_count, session_id, total_tokens
        );

        Ok(PersistResult {
            session_id,
            chunk_count: stored_count,
            deduplicated_count: dedup_count,
            total_tokens,
        })
    }

    /// Format conversation messages into readable text.
    ///
    /// Each message is formatted as:
    /// ```text
    /// [Role]: Content
    /// ```
    ///
    /// Tool calls and results are formatted with appropriate labels.
    pub fn format_conversation(&self, messages: &[Message]) -> String {
        let mut lines = Vec::new();

        for msg in messages {
            // Skip system messages if configured
            if !self.config.include_system_messages && matches!(msg.role, Role::System) {
                continue;
            }

            let role_str = match msg.role {
                Role::System => "System",
                Role::User => "User",
                Role::Assistant => "Assistant",
                Role::Tool => "Tool Result",
            };

            // Format main content
            if !msg.content.is_empty() {
                lines.push(format!("[{}]: {}", role_str, msg.content));
            }

            // Format tool calls if present
            if let Some(tool_calls) = &msg.tool_calls {
                for tc in tool_calls {
                    lines.push(format!(
                        "[Assistant Tool Call]: {}({})",
                        tc.name, tc.arguments
                    ));
                }
            }
        }

        lines.join("\n\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_ai::llm::{Message, ToolCall};
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn create_test_storage() -> (MemoryStorage, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(redb::Database::create(db_path).unwrap());
        (MemoryStorage::new(db).unwrap(), temp_dir)
    }

    fn create_test_messages() -> Vec<Message> {
        vec![
            Message::system("You are a helpful assistant"),
            Message::user("What is the weather today?"),
            Message::assistant("Let me check the weather for you."),
            Message::user("Thanks!"),
            Message::assistant("The weather today is sunny with a high of 75°F."),
        ]
    }

    #[test]
    fn test_persist_config_default() {
        let config = PersistConfig::default();
        assert_eq!(config.chunk_size, 1600);
        assert_eq!(config.chunk_overlap, 320);
        assert_eq!(config.min_content_length, 100);
        assert!(!config.include_system_messages);
    }

    #[test]
    fn test_persist_config_builder() {
        let config = PersistConfig::default()
            .with_system_messages()
            .with_chunk_size(2000)
            .with_overlap(400);

        assert!(config.include_system_messages);
        assert_eq!(config.chunk_size, 2000);
        assert_eq!(config.chunk_overlap, 400);
    }

    #[test]
    fn test_format_conversation_basic() {
        let (storage, _temp_dir) = create_test_storage();
        let persister = MemoryPersister::new(storage);

        let messages = vec![Message::user("Hello"), Message::assistant("Hi there!")];

        let text = persister.format_conversation(&messages);
        assert!(text.contains("[User]: Hello"));
        assert!(text.contains("[Assistant]: Hi there!"));
    }

    #[test]
    fn test_format_conversation_excludes_system_by_default() {
        let (storage, _temp_dir) = create_test_storage();
        let persister = MemoryPersister::new(storage);

        let messages = create_test_messages();
        let text = persister.format_conversation(&messages);

        // System message should NOT be included by default
        assert!(!text.contains("[System]"));
        assert!(text.contains("[User]"));
        assert!(text.contains("[Assistant]"));
    }

    #[test]
    fn test_format_conversation_includes_system_when_configured() {
        let (storage, _temp_dir) = create_test_storage();
        let config = PersistConfig::default().with_system_messages();
        let persister = MemoryPersister::with_config(storage, config);

        let messages = create_test_messages();
        let text = persister.format_conversation(&messages);

        // System message SHOULD be included
        assert!(text.contains("[System]: You are a helpful assistant"));
    }

    #[test]
    fn test_format_conversation_with_tool_calls() {
        let (storage, _temp_dir) = create_test_storage();
        let persister = MemoryPersister::new(storage);

        let tool_calls = vec![ToolCall {
            id: "call_1".to_string(),
            name: "weather".to_string(),
            arguments: json!({"city": "Seattle"}),
        }];

        let messages = vec![
            Message::user("What's the weather?"),
            Message::assistant_with_tool_calls(Some("Let me check.".to_string()), tool_calls),
            Message::tool_result("call_1".to_string(), "Sunny, 75°F".to_string()),
        ];

        let text = persister.format_conversation(&messages);
        assert!(text.contains("[User]: What's the weather?"));
        assert!(text.contains("[Assistant]: Let me check."));
        assert!(text.contains("[Assistant Tool Call]: weather("));
        assert!(text.contains("[Tool Result]: Sunny, 75°F"));
    }

    #[test]
    fn test_persist_creates_session_and_chunks() {
        let (storage, _temp_dir) = create_test_storage();
        let persister = MemoryPersister::new(storage.clone());

        let messages = create_test_messages();
        let result = persister
            .persist(
                &messages,
                "agent-1",
                "task-1",
                "Daily Report",
                &["daily".to_string()],
            )
            .unwrap();

        // Should have created a session
        assert!(!result.session_id.is_empty());

        // Should have at least one chunk
        assert!(result.chunk_count > 0 || result.deduplicated_count > 0);

        // Verify session exists
        let session = storage.get_session(&result.session_id).unwrap();
        assert!(session.is_some());
        let session = session.unwrap();
        assert!(session.name.contains("Daily Report"));
        assert_eq!(session.agent_id, "agent-1");
    }

    #[test]
    fn test_persist_skips_short_content() {
        let (storage, _temp_dir) = create_test_storage();
        let persister = MemoryPersister::new(storage);

        // Very short conversation
        let messages = vec![Message::user("Hi"), Message::assistant("Hello")];

        let result = persister
            .persist(&messages, "agent-1", "task-1", "Test", &[])
            .unwrap();

        // Should be skipped due to minimum content length
        assert!(result.session_id.is_empty());
        assert_eq!(result.chunk_count, 0);
    }

    #[test]
    fn test_persist_deduplication() {
        let (storage, _temp_dir) = create_test_storage();
        let persister = MemoryPersister::new(storage);

        let messages = create_test_messages();

        // Persist the same conversation twice
        let result1 = persister
            .persist(&messages, "agent-1", "task-1", "Test 1", &[])
            .unwrap();

        let result2 = persister
            .persist(&messages, "agent-1", "task-2", "Test 2", &[])
            .unwrap();

        // First persist should store chunks
        assert!(result1.chunk_count > 0);

        // Second persist should deduplicate (same content)
        assert!(result2.deduplicated_count > 0);
    }

    #[test]
    fn test_persist_adds_tags() {
        let (storage, _temp_dir) = create_test_storage();
        let persister = MemoryPersister::new(storage.clone());

        let messages = create_test_messages();
        let tags = vec!["automation".to_string(), "weather".to_string()];

        let result = persister
            .persist(&messages, "agent-1", "task-1", "Test", &tags)
            .unwrap();

        // Verify chunks have the tags
        let chunks = storage.list_chunks_for_session(&result.session_id).unwrap();
        if !chunks.is_empty() {
            let chunk = &chunks[0];
            assert!(chunk.tags.contains(&"automation".to_string()));
            assert!(chunk.tags.contains(&"weather".to_string()));
        }
    }

    #[test]
    fn test_persist_result_fields() {
        let result = PersistResult {
            session_id: "session-123".to_string(),
            chunk_count: 5,
            deduplicated_count: 2,
            total_tokens: 1500,
        };

        assert_eq!(result.session_id, "session-123");
        assert_eq!(result.chunk_count, 5);
        assert_eq!(result.deduplicated_count, 2);
        assert_eq!(result.total_tokens, 1500);
    }

    #[test]
    fn test_long_conversation_chunking() {
        let (storage, _temp_dir) = create_test_storage();
        // Use smaller chunk size for testing
        let config = PersistConfig::default()
            .with_chunk_size(200)
            .with_overlap(40);
        let persister = MemoryPersister::with_config(storage.clone(), config);

        // Create a longer conversation that will require multiple chunks
        let mut messages = Vec::new();
        for i in 0..20 {
            messages.push(Message::user(format!(
                "This is message number {} with some extra content to make it longer.",
                i
            )));
            messages.push(Message::assistant(format!("Response {} - I understand your message and here is my detailed reply with additional information.", i)));
        }

        let result = persister
            .persist(&messages, "agent-1", "task-1", "Long Conversation", &[])
            .unwrap();

        // Should have created multiple chunks
        assert!(
            result.chunk_count > 1,
            "Expected multiple chunks for long conversation"
        );

        // Verify chunks were stored
        let chunks = storage.list_chunks_for_session(&result.session_id).unwrap();
        assert_eq!(chunks.len(), result.chunk_count);
    }
}
