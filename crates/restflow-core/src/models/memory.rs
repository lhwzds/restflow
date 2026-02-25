//! Memory models for long-term agent memory storage.
//!
//! This module defines the data structures for persisting conversation history
//! and agent-generated knowledge to long-term storage. Memory is organized into
//! chunks (for efficient retrieval) and sessions (for organization).
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │                    Long-term Memory                           │
//! │                                                               │
//! │  MemorySession                                                │
//! │  ├── id: "session-abc123"                                    │
//! │  ├── agent_id: "research-agent"                              │
//! │  └── chunks: [MemoryChunk, MemoryChunk, ...]                 │
//! │                                                               │
//! │  MemoryChunk                                                  │
//! │  ├── id: "chunk-xyz789"                                      │
//! │  ├── content: "User asked about Rust async..."               │
//! │  ├── source: TaskExecution { task_id: "task-1" }             │
//! │  └── tags: ["rust", "async", "technical"]                    │
//! └──────────────────────────────────────────────────────────────┘
//! ```

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Source of a memory chunk - where the memory originated from.
///
/// This helps categorize and filter memories based on their origin.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Default)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MemorySource {
    /// Memory from an agent task execution
    TaskExecution {
        /// The task ID that generated this memory
        task_id: String,
    },
    /// Memory from a conversation/chat session
    Conversation {
        /// The session ID where this conversation occurred
        session_id: String,
    },
    /// Manually added note or annotation
    #[default]
    ManualNote,
    /// Memory saved by agent using file memory tools
    AgentGenerated {
        /// Tool that created this memory (e.g., "save_memory")
        tool_name: String,
    },
}

/// A chunk of stored memory content.
///
/// Memory is stored in chunks for efficient retrieval and to support
/// windowed context loading. Each chunk contains a portion of a conversation
/// or document along with metadata for searching and filtering.
///
/// # Example
///
/// ```rust
/// use restflow_core::models::memory::{MemoryChunk, MemorySource};
///
/// let chunk = MemoryChunk::new(
///     "research-agent".to_string(),
///     "User asked about Rust async patterns...".to_string(),
/// )
/// .with_session("session-123".to_string())
/// .with_source(MemorySource::TaskExecution {
///     task_id: "task-abc".to_string(),
/// })
/// .with_tags(vec!["rust".to_string(), "async".to_string()]);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct MemoryChunk {
    /// Unique identifier for this chunk
    pub id: String,

    /// Agent ID this memory belongs to
    pub agent_id: String,

    /// Optional session ID for grouping related chunks
    #[serde(default)]
    pub session_id: Option<String>,

    /// The actual memory content (text)
    pub content: String,

    /// SHA-256 hash of content for deduplication
    pub content_hash: String,

    /// Source of this memory
    #[serde(default)]
    pub source: MemorySource,

    /// Unix timestamp in milliseconds when this chunk was created
    #[ts(type = "number")]
    pub created_at: i64,

    /// Tags for categorization and search filtering
    #[serde(default)]
    pub tags: Vec<String>,

    /// Token count estimate for this chunk (for context window management)
    #[serde(default)]
    pub token_count: Option<u32>,

    /// Vector embedding for semantic search
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub embedding: Option<Vec<f32>>,

    /// Model used to generate the embedding (e.g., "text-embedding-3-small")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub embedding_model: Option<String>,

    /// Embedding dimension (for validation)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub embedding_dim: Option<usize>,
}

impl MemoryChunk {
    /// Create a new memory chunk with required fields.
    ///
    /// Generates a unique ID and content hash automatically.
    pub fn new(agent_id: String, content: String) -> Self {
        use restflow_storage::time_utils;
        use sha2::{Digest, Sha256};

        let id = format!("chunk-{}", uuid::Uuid::new_v4());
        let content_hash = hex::encode(Sha256::digest(content.as_bytes()));
        let created_at = time_utils::now_ms();

        Self {
            id,
            agent_id,
            session_id: None,
            content,
            content_hash,
            source: MemorySource::ManualNote,
            created_at,
            tags: Vec::new(),
            token_count: None,
            embedding: None,
            embedding_model: None,
            embedding_dim: None,
        }
    }

    /// Create a chunk with a specific ID (for deserialization/testing)
    #[must_use]
    pub fn with_id(mut self, id: String) -> Self {
        self.id = id;
        self
    }

    /// Set the session ID
    #[must_use]
    pub fn with_session(mut self, session_id: String) -> Self {
        self.session_id = Some(session_id);
        self
    }

    /// Set the memory source
    #[must_use]
    pub fn with_source(mut self, source: MemorySource) -> Self {
        self.source = source;
        self
    }

    /// Set the tags
    #[must_use]
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Add a single tag
    #[must_use]
    pub fn add_tag(mut self, tag: String) -> Self {
        self.tags.push(tag);
        self
    }

    /// Set the token count
    #[must_use]
    pub fn with_token_count(mut self, count: u32) -> Self {
        self.token_count = Some(count);
        self
    }

    /// Attach an embedding to this chunk
    #[must_use]
    pub fn with_embedding(mut self, embedding: Vec<f32>, model: String) -> Self {
        self.embedding_dim = Some(embedding.len());
        self.embedding = Some(embedding);
        self.embedding_model = Some(model);
        self
    }

    /// Check if this chunk has an embedding
    #[must_use]
    pub fn has_embedding(&self) -> bool {
        self.embedding.is_some()
    }

    /// Set the created_at timestamp
    #[must_use]
    pub fn with_created_at(mut self, timestamp: i64) -> Self {
        self.created_at = timestamp;
        self
    }
}

/// A memory session representing a group of related memory chunks.
///
/// Sessions help organize memories by conversation or task execution.
/// Each session contains metadata and references to its chunks.
///
/// # Example
///
/// ```rust
/// use restflow_core::models::memory::MemorySession;
///
/// let session = MemorySession::new(
///     "research-agent".to_string(),
///     "Research on Rust async".to_string(),
/// );
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct MemorySession {
    /// Unique identifier for this session
    pub id: String,

    /// Agent ID this session belongs to
    pub agent_id: String,

    /// Human-readable name/title for this session
    pub name: String,

    /// Optional description of the session content
    #[serde(default)]
    pub description: Option<String>,

    /// Number of chunks in this session
    #[serde(default)]
    pub chunk_count: u32,

    /// Total token count across all chunks
    #[serde(default)]
    pub total_tokens: u32,

    /// Unix timestamp in milliseconds when this session was created
    #[ts(type = "number")]
    pub created_at: i64,

    /// Unix timestamp in milliseconds when this session was last updated
    #[ts(type = "number")]
    pub updated_at: i64,

    /// Tags for the entire session
    #[serde(default)]
    pub tags: Vec<String>,
}

impl MemorySession {
    /// Create a new memory session.
    pub fn new(agent_id: String, name: String) -> Self {
        use restflow_storage::time_utils;

        let id = format!("session-{}", uuid::Uuid::new_v4());
        let now = time_utils::now_ms();

        Self {
            id,
            agent_id,
            name,
            description: None,
            chunk_count: 0,
            total_tokens: 0,
            created_at: now,
            updated_at: now,
            tags: Vec::new(),
        }
    }

    /// Create a memory session with a deterministic ID based on source identity.
    ///
    /// The ID is derived from `sha256(source_key)` so the same source always
    /// produces the same session ID, even if the bound agent changes later.
    /// This enables stable upsert semantics for session-centric memory.
    pub fn new_deterministic(agent_id: String, source_id: &str, name: String) -> Self {
        use restflow_storage::time_utils;
        use sha2::{Digest, Sha256};

        let hash = hex::encode(Sha256::digest(source_id.as_bytes()));
        let id = format!("session-{}", &hash[..16]);
        let now = time_utils::now_ms();

        Self {
            id,
            agent_id,
            name,
            description: None,
            chunk_count: 0,
            total_tokens: 0,
            created_at: now,
            updated_at: now,
            tags: Vec::new(),
        }
    }

    /// Create a session with a specific ID (for deserialization/testing)
    #[must_use]
    pub fn with_id(mut self, id: String) -> Self {
        self.id = id;
        self
    }

    /// Set the description
    #[must_use]
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    /// Set the tags
    #[must_use]
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Update chunk count and total tokens
    #[must_use]
    pub fn update_stats(mut self, chunk_count: u32, total_tokens: u32) -> Self {
        self.chunk_count = chunk_count;
        self.total_tokens = total_tokens;
        self
    }

    /// Update the updated_at timestamp to now
    #[must_use]
    pub fn touch(mut self) -> Self {
        use restflow_storage::time_utils;

        self.updated_at = time_utils::now_ms();
        self
    }
}

/// Query parameters for semantic vector search.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SemanticSearchQuery {
    pub agent_id: String,
    pub query_text: String,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    #[serde(default)]
    pub min_score: Option<f32>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub session_id: Option<String>,
}

fn default_top_k() -> usize {
    10
}

/// Match result for semantic search.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SemanticMatch {
    pub chunk: MemoryChunk,
    /// Cosine distance (0 = identical, 2 = opposite)
    pub distance: f32,
    /// Similarity score (1 = identical, 0 = orthogonal, -1 = opposite)
    pub similarity: f32,
}

/// Semantic search response payload.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SemanticSearchResult {
    pub matches: Vec<SemanticMatch>,
    pub query_embedding_model: String,
    pub search_time_ms: u64,
}

/// Query parameters for hybrid semantic + keyword search.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HybridSearchQuery {
    pub agent_id: String,
    pub query_text: String,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    /// Weight for semantic search (0.0-1.0); rest goes to text search
    #[serde(default = "default_semantic_weight")]
    pub semantic_weight: f32,
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_semantic_weight() -> f32 {
    0.7
}

/// Search query for finding memory chunks.
///
/// Supports keyword search, tag filtering, time range filtering,
/// and source type filtering.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MemorySearchQuery {
    /// Agent ID to search within (required)
    pub agent_id: String,

    /// Text query for content search (keyword, phrase, or regex)
    #[serde(default)]
    pub query: Option<String>,

    /// Search mode for the query
    #[serde(default)]
    pub search_mode: SearchMode,

    /// Filter by session ID
    #[serde(default)]
    pub session_id: Option<String>,

    /// Filter by tags (chunks must have ALL specified tags)
    #[serde(default)]
    pub tags: Vec<String>,

    /// Filter by source type
    #[serde(default)]
    pub source_type: Option<SourceTypeFilter>,

    /// Start of time range (unix timestamp in ms)
    #[ts(type = "number | null")]
    #[serde(default)]
    pub from_time: Option<i64>,

    /// End of time range (unix timestamp in ms)
    #[ts(type = "number | null")]
    #[serde(default)]
    pub to_time: Option<i64>,

    /// Maximum number of results to return
    #[serde(default = "default_limit")]
    pub limit: u32,

    /// Offset for pagination
    #[serde(default)]
    pub offset: u32,
}

fn default_limit() -> u32 {
    50
}

impl Default for MemorySearchQuery {
    fn default() -> Self {
        Self {
            agent_id: String::new(),
            query: None,
            search_mode: SearchMode::default(),
            session_id: None,
            tags: Vec::new(),
            source_type: None,
            from_time: None,
            to_time: None,
            limit: default_limit(),
            offset: 0,
        }
    }
}

impl MemorySearchQuery {
    /// Create a new search query for an agent
    pub fn new(agent_id: String) -> Self {
        Self {
            agent_id,
            ..Default::default()
        }
    }

    /// Set the text query
    #[must_use]
    pub fn with_query(mut self, query: String) -> Self {
        self.query = Some(query);
        self
    }

    /// Set the search mode
    #[must_use]
    pub fn with_mode(mut self, mode: SearchMode) -> Self {
        self.search_mode = mode;
        self
    }

    /// Filter by session
    pub fn in_session(mut self, session_id: String) -> Self {
        self.session_id = Some(session_id);
        self
    }

    /// Filter by tags
    #[must_use]
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Filter by source type
    pub fn from_source(mut self, source_type: SourceTypeFilter) -> Self {
        self.source_type = Some(source_type);
        self
    }

    /// Set time range
    pub fn in_range(mut self, from: Option<i64>, to: Option<i64>) -> Self {
        self.from_time = from;
        self.to_time = to;
        self
    }

    /// Set pagination
    pub fn paginate(mut self, limit: u32, offset: u32) -> Self {
        self.limit = limit;
        self.offset = offset;
        self
    }
}

/// Unified search query combining memory and chat session filters.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct UnifiedSearchQuery {
    /// Base memory search query.
    #[serde(flatten)]
    pub base: MemorySearchQuery,

    /// Whether to include chat sessions in search.
    #[serde(default)]
    pub include_sessions: bool,

    /// Filter to specific session IDs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub session_ids: Vec<String>,

    /// Filter to sessions with a specific agent ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_agent_id: Option<String>,
}

impl UnifiedSearchQuery {
    /// Create a new unified search query.
    pub fn new(base: MemorySearchQuery) -> Self {
        Self {
            base,
            include_sessions: true,
            session_ids: Vec::new(),
            session_agent_id: None,
        }
    }

    /// Enable or disable session search.
    #[must_use]
    pub fn with_sessions(mut self, include_sessions: bool) -> Self {
        self.include_sessions = include_sessions;
        self
    }

    /// Restrict search to specific session IDs.
    #[must_use]
    pub fn with_session_ids(mut self, session_ids: Vec<String>) -> Self {
        self.session_ids = session_ids;
        self
    }

    /// Override agent ID used when searching sessions.
    #[must_use]
    pub fn with_session_agent_id(mut self, agent_id: Option<String>) -> Self {
        self.session_agent_id = agent_id;
        self
    }
}

/// Search mode for memory queries.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default, PartialEq)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum SearchMode {
    /// Simple keyword search (case-insensitive contains)
    #[default]
    Keyword,
    /// Exact phrase search
    Phrase,
    /// Regular expression search
    Regex,
}

/// Filter for memory source types.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum SourceTypeFilter {
    /// Only task execution memories
    TaskExecution,
    /// Only conversation memories
    Conversation,
    /// Only manual notes
    ManualNote,
    /// Only agent-generated memories
    AgentGenerated,
}

/// Result of a memory search operation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MemorySearchResult {
    /// Matching chunks
    pub chunks: Vec<MemoryChunk>,

    /// Total number of matching results (for pagination)
    pub total_count: u32,

    /// Whether there are more results available
    pub has_more: bool,
}

/// Statistics about an agent's memory storage.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct MemoryStats {
    /// Agent ID
    pub agent_id: String,

    /// Total number of sessions
    pub session_count: u32,

    /// Total number of chunks
    pub chunk_count: u32,

    /// Total tokens across all chunks
    pub total_tokens: u32,

    /// Oldest memory timestamp
    #[ts(type = "number | null")]
    pub oldest_memory: Option<i64>,

    /// Newest memory timestamp
    #[ts(type = "number | null")]
    pub newest_memory: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_chunk_new() {
        let chunk = MemoryChunk::new(
            "test-agent".to_string(),
            "Test content for memory".to_string(),
        );

        assert!(chunk.id.starts_with("chunk-"));
        assert_eq!(chunk.agent_id, "test-agent");
        assert_eq!(chunk.content, "Test content for memory");
        assert!(!chunk.content_hash.is_empty());
        assert!(chunk.session_id.is_none());
        assert!(chunk.tags.is_empty());
        assert!(chunk.created_at > 0);
    }

    #[test]
    fn test_memory_chunk_builder() {
        let chunk = MemoryChunk::new("agent-1".to_string(), "Content".to_string())
            .with_session("session-1".to_string())
            .with_source(MemorySource::TaskExecution {
                task_id: "task-1".to_string(),
            })
            .with_tags(vec!["tag1".to_string(), "tag2".to_string()])
            .with_token_count(100);

        assert_eq!(chunk.session_id, Some("session-1".to_string()));
        assert_eq!(
            chunk.source,
            MemorySource::TaskExecution {
                task_id: "task-1".to_string()
            }
        );
        assert_eq!(chunk.tags, vec!["tag1", "tag2"]);
        assert_eq!(chunk.token_count, Some(100));
    }

    #[test]
    fn test_memory_chunk_content_hash_consistency() {
        // Same content should produce same hash
        let chunk1 = MemoryChunk::new("agent".to_string(), "Same content".to_string());
        let chunk2 = MemoryChunk::new("agent".to_string(), "Same content".to_string());

        assert_eq!(chunk1.content_hash, chunk2.content_hash);

        // Different content should produce different hash
        let chunk3 = MemoryChunk::new("agent".to_string(), "Different content".to_string());
        assert_ne!(chunk1.content_hash, chunk3.content_hash);
    }

    #[test]
    fn test_memory_session_new() {
        let session = MemorySession::new("test-agent".to_string(), "Test Session".to_string());

        assert!(session.id.starts_with("session-"));
        assert_eq!(session.agent_id, "test-agent");
        assert_eq!(session.name, "Test Session");
        assert_eq!(session.chunk_count, 0);
        assert_eq!(session.total_tokens, 0);
        assert!(session.created_at > 0);
        assert_eq!(session.created_at, session.updated_at);
    }

    #[test]
    fn test_memory_session_builder() {
        let session = MemorySession::new("agent".to_string(), "Session".to_string())
            .with_description("A test session".to_string())
            .with_tags(vec!["important".to_string()])
            .update_stats(5, 1000);

        assert_eq!(session.description, Some("A test session".to_string()));
        assert_eq!(session.tags, vec!["important"]);
        assert_eq!(session.chunk_count, 5);
        assert_eq!(session.total_tokens, 1000);
    }

    #[test]
    fn test_memory_source_serialization() {
        let task_source = MemorySource::TaskExecution {
            task_id: "task-123".to_string(),
        };
        let json = serde_json::to_string(&task_source).unwrap();
        assert!(json.contains("task_execution"));
        assert!(json.contains("task-123"));

        let conv_source = MemorySource::Conversation {
            session_id: "sess-456".to_string(),
        };
        let json = serde_json::to_string(&conv_source).unwrap();
        assert!(json.contains("conversation"));
        assert!(json.contains("sess-456"));

        let manual_source = MemorySource::ManualNote;
        let json = serde_json::to_string(&manual_source).unwrap();
        assert!(json.contains("manual_note"));
    }

    #[test]
    fn test_memory_source_deserialization() {
        let json = r#"{"type":"task_execution","task_id":"task-abc"}"#;
        let source: MemorySource = serde_json::from_str(json).unwrap();
        assert_eq!(
            source,
            MemorySource::TaskExecution {
                task_id: "task-abc".to_string()
            }
        );

        let json = r#"{"type":"manual_note"}"#;
        let source: MemorySource = serde_json::from_str(json).unwrap();
        assert_eq!(source, MemorySource::ManualNote);
    }

    #[test]
    fn test_memory_chunk_serialization() {
        let chunk = MemoryChunk::new("agent".to_string(), "Test content".to_string())
            .with_id("chunk-test".to_string())
            .with_tags(vec!["tag".to_string()]);

        let json = serde_json::to_string(&chunk).unwrap();
        let parsed: MemoryChunk = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, "chunk-test");
        assert_eq!(parsed.agent_id, "agent");
        assert_eq!(parsed.content, "Test content");
        assert_eq!(parsed.tags, vec!["tag"]);
    }

    #[test]
    fn test_memory_session_serialization() {
        let session = MemorySession::new("agent".to_string(), "Test".to_string())
            .with_id("session-test".to_string())
            .with_description("Description".to_string());

        let json = serde_json::to_string(&session).unwrap();
        let parsed: MemorySession = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, "session-test");
        assert_eq!(parsed.name, "Test");
        assert_eq!(parsed.description, Some("Description".to_string()));
    }

    #[test]
    fn test_search_query_builder() {
        let query = MemorySearchQuery::new("agent".to_string())
            .with_query("rust async".to_string())
            .with_mode(SearchMode::Phrase)
            .with_tags(vec!["technical".to_string()])
            .from_source(SourceTypeFilter::TaskExecution)
            .in_range(Some(1000), Some(2000))
            .paginate(10, 20);

        assert_eq!(query.agent_id, "agent");
        assert_eq!(query.query, Some("rust async".to_string()));
        assert_eq!(query.search_mode, SearchMode::Phrase);
        assert_eq!(query.tags, vec!["technical"]);
        assert_eq!(query.source_type, Some(SourceTypeFilter::TaskExecution));
        assert_eq!(query.from_time, Some(1000));
        assert_eq!(query.to_time, Some(2000));
        assert_eq!(query.limit, 10);
        assert_eq!(query.offset, 20);
    }

    #[test]
    fn test_search_mode_serialization() {
        assert_eq!(
            serde_json::to_string(&SearchMode::Keyword).unwrap(),
            "\"keyword\""
        );
        assert_eq!(
            serde_json::to_string(&SearchMode::Phrase).unwrap(),
            "\"phrase\""
        );
        assert_eq!(
            serde_json::to_string(&SearchMode::Regex).unwrap(),
            "\"regex\""
        );
    }

    #[test]
    fn test_deterministic_session_id() {
        let s1 = MemorySession::new_deterministic(
            "agent-x".to_string(),
            "source-123",
            "Name".to_string(),
        );
        let s2 = MemorySession::new_deterministic(
            "agent-x".to_string(),
            "source-123",
            "Name".to_string(),
        );
        assert_eq!(s1.id, s2.id, "same inputs must produce same ID");
        assert!(s1.id.starts_with("session-"));
        assert_eq!(s1.id.len(), "session-".len() + 16);

        let s3 = MemorySession::new_deterministic(
            "agent-y".to_string(),
            "source-123",
            "Name".to_string(),
        );
        assert_eq!(
            s1.id, s3.id,
            "same source key must stay stable even when agent changes"
        );
    }

    #[test]
    fn test_memory_stats_default() {
        let stats = MemoryStats::default();
        assert!(stats.agent_id.is_empty());
        assert_eq!(stats.session_count, 0);
        assert_eq!(stats.chunk_count, 0);
        assert_eq!(stats.total_tokens, 0);
        assert!(stats.oldest_memory.is_none());
        assert!(stats.newest_memory.is_none());
    }
}
