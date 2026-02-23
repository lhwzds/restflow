//! Typed memory storage wrapper.
//!
//! Provides type-safe access to long-term memory storage by wrapping the
//! byte-level APIs from restflow-storage with Rust types from our models.
//!
//! # Features
//!
//! - **Chunk Storage**: Store and retrieve memory chunks with full metadata
//! - **Session Management**: Organize chunks into named sessions
//! - **Deduplication**: Automatic content hash checking to prevent duplicates
//! - **Search**: Text-based search across memory content
//! - **Statistics**: Track memory usage per agent
//! - **Vector Orphan Cleanup**: HNSW index maintenance for deleted vectors

use crate::models::memory::{
    MemoryChunk, MemorySearchQuery, MemorySearchResult, MemorySession, MemorySource, MemoryStats,
    SearchMode, SemanticMatch, SourceTypeFilter,
};
use anyhow::{Result, anyhow};
use redb::Database;
use regex::Regex;
use restflow_storage::{
    IndexableChunk, MemoryIndex, PutChunkResult, VectorConfig, VectorStats, VectorStorage,
};
use std::sync::Arc;

/// Typed memory storage wrapper around restflow-storage::MemoryStorage.
#[derive(Clone)]
pub struct MemoryStorage {
    inner: restflow_storage::MemoryStorage,
    vectors: Option<Arc<VectorStorage>>,
    index: Option<Arc<MemoryIndex>>,
}

impl MemoryStorage {
    /// Create a new MemoryStorage instance
    pub fn new(db: Arc<Database>) -> Result<Self> {
        let index = Some(Arc::new(MemoryIndex::in_memory()?));
        Self::with_index(db, index)
    }

    /// Create a MemoryStorage instance with a custom text index
    pub fn with_index(db: Arc<Database>, index: Option<Arc<MemoryIndex>>) -> Result<Self> {
        Ok(Self {
            inner: restflow_storage::MemoryStorage::new(db)?,
            vectors: None,
            index,
        })
    }

    /// Create a MemoryStorage instance with vector search enabled
    pub fn with_vectors(db: Arc<Database>, config: VectorConfig) -> Result<Self> {
        let index = Some(Arc::new(MemoryIndex::in_memory()?));
        Ok(Self {
            inner: restflow_storage::MemoryStorage::new(db.clone())?,
            vectors: Some(Arc::new(VectorStorage::new(db, config)?)),
            index,
        })
    }

    /// Check if vector search is enabled
    pub fn has_vector_search(&self) -> bool {
        self.vectors.is_some()
    }

    /// Check if text index is enabled
    pub fn has_text_index(&self) -> bool {
        self.index.is_some()
    }

    // ============== Chunk Operations ==============

    /// Store a memory chunk.
    ///
    /// If a chunk with the same content hash already exists, returns the
    /// existing chunk ID without creating a duplicate.
    pub fn store_chunk(&self, chunk: &MemoryChunk) -> Result<String> {
        let json_bytes = serde_json::to_vec(chunk)?;
        let result = self.inner.put_chunk_if_not_exists(
            &chunk.id,
            &chunk.agent_id,
            chunk.session_id.as_deref(),
            &chunk.content_hash,
            &chunk.tags,
            &json_bytes,
        )?;

        match result {
            PutChunkResult::Created(id) => {
                if let (Some(vectors), Some(embedding)) = (&self.vectors, &chunk.embedding) {
                    vectors.add(&chunk.id, embedding)?;
                }

                if let Some(index) = &self.index {
                    index.index_chunk(&Self::to_indexable_chunk(chunk))?;
                }

                Ok(id)
            }
            PutChunkResult::Existing(id) => Ok(id),
        }
    }

    /// Store a memory chunk with embedding support
    pub fn store_chunk_with_embedding(&self, chunk: &MemoryChunk) -> Result<String> {
        self.store_chunk(chunk)
    }

    /// Get a memory chunk by ID
    pub fn get_chunk(&self, chunk_id: &str) -> Result<Option<MemoryChunk>> {
        if let Some(bytes) = self.inner.get_chunk_raw(chunk_id)? {
            let chunk: MemoryChunk = serde_json::from_slice(&bytes)?;
            Ok(Some(chunk))
        } else {
            Ok(None)
        }
    }

    /// List all chunks for an agent
    pub fn list_chunks(&self, agent_id: &str) -> Result<Vec<MemoryChunk>> {
        let chunks = self.inner.list_chunks_by_agent_raw(agent_id)?;
        let mut result = Vec::new();
        for (_, bytes) in chunks {
            let chunk: MemoryChunk = serde_json::from_slice(&bytes)?;
            result.push(chunk);
        }
        // Sort by created_at descending (most recent first)
        result.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(result)
    }

    /// List all chunks for a session
    pub fn list_chunks_for_session(&self, session_id: &str) -> Result<Vec<MemoryChunk>> {
        let chunks = self.inner.list_chunks_by_session_raw(session_id)?;
        let mut result = Vec::new();
        for (_, bytes) in chunks {
            let chunk: MemoryChunk = serde_json::from_slice(&bytes)?;
            result.push(chunk);
        }
        // Sort by created_at ascending (chronological order within session)
        result.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(result)
    }

    /// Check if any chunks exist for a session without loading them all.
    pub fn has_chunks_for_session(&self, session_id: &str) -> Result<bool> {
        self.inner.has_chunks_for_session(session_id)
    }

    /// List all chunks with a specific tag
    pub fn list_chunks_by_tag(&self, tag: &str) -> Result<Vec<MemoryChunk>> {
        let chunks = self.inner.list_chunks_by_tag_raw(tag)?;
        let mut result = Vec::new();
        for (_, bytes) in chunks {
            let chunk: MemoryChunk = serde_json::from_slice(&bytes)?;
            result.push(chunk);
        }
        result.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(result)
    }

    /// Check if a chunk with the given content already exists
    pub fn exists_by_content(&self, content: &str) -> Result<Option<String>> {
        use sha2::{Digest, Sha256};
        let hash = hex::encode(Sha256::digest(content.as_bytes()));
        self.inner.find_by_hash(&hash)
    }

    /// Delete a memory chunk
    pub fn delete_chunk(&self, chunk_id: &str) -> Result<bool> {
        // First get the chunk to know its metadata for index cleanup
        if let Some(chunk) = self.get_chunk(chunk_id)? {
            if let Some(vectors) = &self.vectors {
                vectors.delete(chunk_id)?;
            }

            if let Some(index) = &self.index {
                index.remove_chunk(chunk_id)?;
            }

            self.inner.delete_chunk(
                chunk_id,
                &chunk.agent_id,
                chunk.session_id.as_deref(),
                &chunk.content_hash,
                &chunk.tags,
            )
        } else {
            Ok(false)
        }
    }

    /// Delete a chunk and its embedding if present
    pub fn delete_chunk_with_embedding(&self, chunk_id: &str) -> Result<bool> {
        self.delete_chunk(chunk_id)
    }

    /// Delete all chunks for an agent
    pub fn delete_chunks_for_agent(&self, agent_id: &str) -> Result<u32> {
        let chunks = self.list_chunks(agent_id)?;
        let metadata: Vec<_> = chunks
            .iter()
            .map(|chunk| {
                (
                    chunk.id.clone(),
                    chunk.session_id.clone(),
                    chunk.content_hash.clone(),
                    chunk.tags.clone(),
                )
            })
            .collect();

        let deleted = self
            .inner
            .delete_all_chunks_for_agent_with_metadata(agent_id, &metadata)?;

        if let Some(index) = &self.index {
            for chunk in chunks {
                index.remove_chunk(&chunk.id)?;
            }
        }

        Ok(deleted)
    }

    /// Delete chunks older than the given timestamp across all agents.
    ///
    /// Returns the number of deleted chunks.
    pub fn cleanup_old_chunks(&self, older_than_ms: i64) -> Result<usize> {
        let raw_chunks = self.inner.list_all_chunks_raw()?;
        let mut deleted = 0usize;

        for (chunk_id, bytes) in raw_chunks {
            let chunk: MemoryChunk = serde_json::from_slice(&bytes)?;
            if chunk.created_at < older_than_ms && self.delete_chunk(&chunk_id)? {
                deleted += 1;
            }
        }

        Ok(deleted)
    }

    // ============== Session Operations ==============

    /// Create a new memory session
    pub fn create_session(&self, session: &MemorySession) -> Result<String> {
        let json_bytes = serde_json::to_vec(session)?;
        self.inner
            .put_session_raw(&session.id, &session.agent_id, &json_bytes)?;
        Ok(session.id.clone())
    }

    /// Get a memory session by ID
    pub fn get_session(&self, session_id: &str) -> Result<Option<MemorySession>> {
        if let Some(bytes) = self.inner.get_session_raw(session_id)? {
            let session: MemorySession = serde_json::from_slice(&bytes)?;
            Ok(Some(session))
        } else {
            Ok(None)
        }
    }

    /// List all sessions for an agent
    pub fn list_sessions(&self, agent_id: &str) -> Result<Vec<MemorySession>> {
        let sessions = self.inner.list_sessions_by_agent_raw(agent_id)?;
        let mut result = Vec::new();
        for (_, bytes) in sessions {
            let session: MemorySession = serde_json::from_slice(&bytes)?;
            result.push(session);
        }
        // Sort by updated_at descending (most recently updated first)
        result.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(result)
    }

    /// Update a session's metadata
    pub fn update_session(&self, session: &MemorySession) -> Result<()> {
        let json_bytes = serde_json::to_vec(session)?;
        self.inner
            .put_session_raw(&session.id, &session.agent_id, &json_bytes)?;
        Ok(())
    }

    /// Update session statistics based on its chunks
    pub fn refresh_session_stats(&self, session_id: &str) -> Result<Option<MemorySession>> {
        if let Some(mut session) = self.get_session(session_id)? {
            let chunks = self.list_chunks_for_session(session_id)?;

            session.chunk_count = chunks.len() as u32;
            session.total_tokens = chunks.iter().filter_map(|c| c.token_count).sum();
            session = session.touch();

            self.update_session(&session)?;
            Ok(Some(session))
        } else {
            Ok(None)
        }
    }

    /// Delete a session and optionally its chunks
    pub fn delete_session(&self, session_id: &str, delete_chunks: bool) -> Result<bool> {
        if let Some(session) = self.get_session(session_id)? {
            // Delete associated chunks if requested
            if delete_chunks {
                let chunks = self.list_chunks_for_session(session_id)?;
                for chunk in chunks {
                    self.delete_chunk(&chunk.id)?;
                }
            }

            self.inner.delete_session(session_id, &session.agent_id)
        } else {
            Ok(false)
        }
    }

    // ============== Search Operations ==============

    /// Search memory chunks based on a query
    pub fn search(&self, query: &MemorySearchQuery) -> Result<MemorySearchResult> {
        if let (Some(index), Some(search_text)) = (&self.index, &query.query)
            && query.search_mode == SearchMode::Keyword
        {
            return self.search_indexed(index, query, search_text);
        }

        // Get all chunks for the agent
        let mut chunks = self.list_chunks(&query.agent_id)?;

        // Apply filters
        chunks = self.apply_filters(chunks, query)?;

        // Apply text search if query is provided
        if let Some(ref search_text) = query.query {
            chunks = self.apply_text_search(chunks, search_text, &query.search_mode)?;
        }

        // Calculate total before pagination
        let total_count = chunks.len() as u32;
        let has_more = total_count > query.offset + query.limit;

        // Apply pagination
        let offset = query.offset as usize;
        let limit = query.limit as usize;
        let paginated: Vec<_> = chunks.into_iter().skip(offset).take(limit).collect();

        Ok(MemorySearchResult {
            chunks: paginated,
            total_count,
            has_more,
        })
    }

    /// Rebuild full-text index from persisted chunks.
    pub fn rebuild_text_index(&self) -> Result<usize> {
        let Some(index) = &self.index else {
            return Ok(0);
        };

        let chunks = self.inner.list_all_chunks_raw()?;
        let mut docs = Vec::with_capacity(chunks.len());
        for (_, bytes) in chunks {
            let chunk: MemoryChunk = serde_json::from_slice(&bytes)?;
            docs.push(Self::to_indexable_chunk(&chunk));
        }

        index.rebuild(docs)
    }

    /// Rebuild full-text index only when the index is empty.
    pub fn rebuild_text_index_if_empty(&self) -> Result<usize> {
        let Some(index) = &self.index else {
            return Ok(0);
        };

        if index.doc_count()? > 0 {
            return Ok(0);
        }

        self.rebuild_text_index()
    }

    /// Semantic vector search
    pub fn semantic_search(
        &self,
        agent_id: &str,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<SemanticMatch>> {
        let vectors = self
            .vectors
            .as_ref()
            .ok_or_else(|| anyhow!("Vector search not enabled"))?;

        let agent_chunks = self.list_chunks(agent_id)?;
        let chunk_ids: Vec<_> = agent_chunks.iter().map(|c| c.id.clone()).collect();
        let results = vectors.search_filtered(query_embedding, top_k, 100, &chunk_ids)?;

        let mut matches = Vec::new();
        for (chunk_id, distance) in results {
            if let Some(chunk) = self.get_chunk(&chunk_id)? {
                matches.push(SemanticMatch {
                    chunk,
                    distance,
                    similarity: 1.0 - (distance / 2.0),
                });
            }
        }

        Ok(matches)
    }

    /// Hybrid semantic + keyword search using reciprocal rank fusion
    pub fn hybrid_search(
        &self,
        agent_id: &str,
        query_embedding: &[f32],
        query_text: &str,
        top_k: usize,
        semantic_weight: f32,
    ) -> Result<Vec<SemanticMatch>> {
        use std::collections::HashMap;

        let semantic = self.semantic_search(agent_id, query_embedding, top_k * 2)?;
        let text_query = MemorySearchQuery::new(agent_id.to_string())
            .with_query(query_text.to_string())
            .paginate((top_k * 2) as u32, 0);
        let text_results = self.search(&text_query)?;

        let mut scores: HashMap<String, f32> = HashMap::new();
        let k = 60.0;

        for (i, m) in semantic.iter().enumerate() {
            let rrf = semantic_weight / (k + i as f32 + 1.0);
            *scores.entry(m.chunk.id.clone()).or_insert(0.0) += rrf;
        }

        for (i, chunk) in text_results.chunks.iter().enumerate() {
            let rrf = (1.0 - semantic_weight) / (k + i as f32 + 1.0);
            *scores.entry(chunk.id.clone()).or_insert(0.0) += rrf;
        }

        let mut ranked: Vec<_> = scores.into_iter().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut matches = Vec::new();
        for (chunk_id, score) in ranked.into_iter().take(top_k) {
            if let Some(chunk) = self.get_chunk(&chunk_id)? {
                matches.push(SemanticMatch {
                    chunk,
                    distance: 0.0,
                    similarity: score,
                });
            }
        }

        Ok(matches)
    }

    /// Apply non-text filters to chunks
    fn apply_filters(
        &self,
        chunks: Vec<MemoryChunk>,
        query: &MemorySearchQuery,
    ) -> Result<Vec<MemoryChunk>> {
        let mut filtered = chunks;

        // Filter by session
        if let Some(ref session_id) = query.session_id {
            filtered.retain(|c| c.session_id.as_ref() == Some(session_id));
        }

        // Filter by tags (all tags must be present)
        if !query.tags.is_empty() {
            filtered.retain(|c| query.tags.iter().all(|tag| c.tags.contains(tag)));
        }

        // Filter by source type
        if let Some(ref source_type) = query.source_type {
            filtered.retain(|c| matches_source_type(&c.source, source_type));
        }

        // Filter by time range
        if let Some(from_time) = query.from_time {
            filtered.retain(|c| c.created_at >= from_time);
        }
        if let Some(to_time) = query.to_time {
            filtered.retain(|c| c.created_at <= to_time);
        }

        Ok(filtered)
    }

    /// Apply text search to chunks
    fn apply_text_search(
        &self,
        chunks: Vec<MemoryChunk>,
        search_text: &str,
        mode: &SearchMode,
    ) -> Result<Vec<MemoryChunk>> {
        match mode {
            SearchMode::Keyword => {
                // Case-insensitive keyword search (all keywords must be present)
                let search_lower = search_text.to_lowercase();
                let keywords: Vec<&str> = search_lower.split_whitespace().collect();
                Ok(chunks
                    .into_iter()
                    .filter(|c| {
                        let content_lower = c.content.to_lowercase();
                        keywords.iter().all(|kw| content_lower.contains(kw))
                    })
                    .collect())
            }
            SearchMode::Phrase => {
                // Exact phrase match (case-insensitive)
                let phrase_lower = search_text.to_lowercase();
                Ok(chunks
                    .into_iter()
                    .filter(|c| c.content.to_lowercase().contains(&phrase_lower))
                    .collect())
            }
            SearchMode::Regex => {
                // Regular expression search
                let regex = Regex::new(search_text)?;
                Ok(chunks
                    .into_iter()
                    .filter(|c| regex.is_match(&c.content))
                    .collect())
            }
        }
    }

    fn search_indexed(
        &self,
        index: &MemoryIndex,
        query: &MemorySearchQuery,
        search_text: &str,
    ) -> Result<MemorySearchResult> {
        let requested = (query.offset + query.limit) as usize;
        let candidate_limit = requested.max(100).saturating_mul(5).min(5_000);
        let hits = index.search(search_text, &query.agent_id, candidate_limit)?;

        if hits.is_empty() {
            return Ok(MemorySearchResult {
                chunks: Vec::new(),
                total_count: 0,
                has_more: false,
            });
        }

        let mut chunks = Vec::with_capacity(hits.len());
        for hit in hits {
            if let Some(chunk) = self.get_chunk(&hit.chunk_id)? {
                chunks.push(chunk);
            }
        }

        chunks = self.apply_filters(chunks, query)?;

        let total_count = chunks.len() as u32;
        let has_more = total_count > query.offset + query.limit;
        let offset = query.offset as usize;
        let limit = query.limit as usize;
        let paginated: Vec<_> = chunks.into_iter().skip(offset).take(limit).collect();

        Ok(MemorySearchResult {
            chunks: paginated,
            total_count,
            has_more,
        })
    }

    fn to_indexable_chunk(chunk: &MemoryChunk) -> IndexableChunk {
        IndexableChunk {
            id: chunk.id.clone(),
            agent_id: chunk.agent_id.clone(),
            content: chunk.content.clone(),
            tags: chunk.tags.clone(),
            created_at: chunk.created_at,
        }
    }

    // ============== Statistics ==============

    /// Get memory statistics for an agent
    pub fn get_stats(&self, agent_id: &str) -> Result<MemoryStats> {
        let chunks = self.list_chunks(agent_id)?;
        let sessions = self.list_sessions(agent_id)?;

        let chunk_count = chunks.len() as u32;
        let session_count = sessions.len() as u32;
        let total_tokens = chunks.iter().filter_map(|c| c.token_count).sum();

        let oldest_memory = chunks.iter().map(|c| c.created_at).min();
        let newest_memory = chunks.iter().map(|c| c.created_at).max();

        Ok(MemoryStats {
            agent_id: agent_id.to_string(),
            session_count,
            chunk_count,
            total_tokens,
            oldest_memory,
            newest_memory,
        })
    }

    // ============== Vector Orphan Cleanup ==============

    /// Get statistics about the vector storage.
    ///
    /// Returns `None` if vector search is not enabled.
    ///
    /// # Example
    /// ```ignore
    /// if let Some(stats) = storage.vector_stats()? {
    ///     println!("Active: {}, Orphans: {}", stats.active_count, stats.orphan_count);
    ///     if stats.orphan_count > 100 {
    ///         storage.cleanup_vector_orphans()?;
    ///     }
    /// }
    /// ```
    pub fn vector_stats(&self) -> Result<Option<VectorStats>> {
        Ok(self.vectors.as_ref().map(|v| v.stats()))
    }

    /// Clean up orphan vectors in the HNSW index.
    ///
    /// HNSW indices do not support efficient vector deletion. When vectors are
    /// deleted, they remain in the index but are filtered out during search.
    /// This method rebuilds the index to reclaim memory from deleted vectors.
    ///
    /// Returns `None` if vector search is not enabled, otherwise returns the
    /// number of orphan vectors cleaned up.
    ///
    /// # When to Call
    ///
    /// Call this method when:
    /// - `vector_stats()` shows a high orphan count
    /// - Memory usage is a concern
    /// - After bulk delete operations
    ///
    /// # Performance
    ///
    /// This is an expensive operation that rebuilds the entire HNSW index.
    /// Consider calling it during off-peak hours or when the orphan count
    /// exceeds a threshold (e.g., > 10% of active vectors).
    pub fn cleanup_vector_orphans(&self) -> Result<Option<usize>> {
        if let Some(vectors) = &self.vectors {
            let cleaned = vectors.cleanup_orphans()?;
            Ok(Some(cleaned))
        } else {
            Ok(None)
        }
    }
}

/// Check if a MemorySource matches a SourceTypeFilter
fn matches_source_type(source: &MemorySource, filter: &SourceTypeFilter) -> bool {
    matches!(
        (source, filter),
        (
            MemorySource::TaskExecution { .. },
            SourceTypeFilter::TaskExecution
        ) | (
            MemorySource::Conversation { .. },
            SourceTypeFilter::Conversation
        ) | (MemorySource::ManualNote, SourceTypeFilter::ManualNote)
            | (
                MemorySource::AgentGenerated { .. },
                SourceTypeFilter::AgentGenerated
            )
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_storage::time_utils;
    use tempfile::tempdir;

    fn create_test_storage() -> MemoryStorage {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        MemoryStorage::new(db).unwrap()
    }

    #[test]
    fn test_store_and_get_chunk() {
        let storage = create_test_storage();

        let chunk = MemoryChunk::new("agent-001".to_string(), "Test content".to_string())
            .with_tags(vec!["tag1".to_string()]);

        let id = storage.store_chunk(&chunk).unwrap();
        assert_eq!(id, chunk.id);

        let retrieved = storage.get_chunk(&id).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.content, "Test content");
        assert_eq!(retrieved.agent_id, "agent-001");
    }

    #[test]
    fn test_deduplication() {
        let storage = create_test_storage();

        let chunk1 = MemoryChunk::new("agent-001".to_string(), "Duplicate content".to_string());
        let chunk2 = MemoryChunk::new("agent-001".to_string(), "Duplicate content".to_string());

        let id1 = storage.store_chunk(&chunk1).unwrap();
        let id2 = storage.store_chunk(&chunk2).unwrap();

        // Should return the same ID due to deduplication
        assert_eq!(id1, id2);

        // Only one chunk should exist
        let chunks = storage.list_chunks("agent-001").unwrap();
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_list_chunks() {
        let storage = create_test_storage();

        let chunk1 = MemoryChunk::new("agent-001".to_string(), "Content 1".to_string());
        let chunk2 = MemoryChunk::new("agent-001".to_string(), "Content 2".to_string());
        let chunk3 = MemoryChunk::new("agent-002".to_string(), "Content 3".to_string());

        storage.store_chunk(&chunk1).unwrap();
        storage.store_chunk(&chunk2).unwrap();
        storage.store_chunk(&chunk3).unwrap();

        let chunks_agent1 = storage.list_chunks("agent-001").unwrap();
        assert_eq!(chunks_agent1.len(), 2);

        let chunks_agent2 = storage.list_chunks("agent-002").unwrap();
        assert_eq!(chunks_agent2.len(), 1);
    }

    #[test]
    fn test_list_chunks_for_session() {
        let storage = create_test_storage();

        let chunk1 = MemoryChunk::new("agent-001".to_string(), "Content 1".to_string())
            .with_session("session-001".to_string());
        let chunk2 = MemoryChunk::new("agent-001".to_string(), "Content 2".to_string())
            .with_session("session-001".to_string());
        let chunk3 = MemoryChunk::new("agent-001".to_string(), "Content 3".to_string())
            .with_session("session-002".to_string());

        storage.store_chunk(&chunk1).unwrap();
        storage.store_chunk(&chunk2).unwrap();
        storage.store_chunk(&chunk3).unwrap();

        let session1_chunks = storage.list_chunks_for_session("session-001").unwrap();
        assert_eq!(session1_chunks.len(), 2);

        let session2_chunks = storage.list_chunks_for_session("session-002").unwrap();
        assert_eq!(session2_chunks.len(), 1);
    }

    #[test]
    fn test_list_chunks_by_tag() {
        let storage = create_test_storage();

        let chunk1 = MemoryChunk::new("agent-001".to_string(), "Rust content".to_string())
            .with_tags(vec!["rust".to_string(), "async".to_string()]);
        let chunk2 = MemoryChunk::new("agent-001".to_string(), "Python content".to_string())
            .with_tags(vec!["python".to_string()]);
        let chunk3 = MemoryChunk::new("agent-001".to_string(), "More Rust".to_string())
            .with_tags(vec!["rust".to_string()]);

        storage.store_chunk(&chunk1).unwrap();
        storage.store_chunk(&chunk2).unwrap();
        storage.store_chunk(&chunk3).unwrap();

        let rust_chunks = storage.list_chunks_by_tag("rust").unwrap();
        assert_eq!(rust_chunks.len(), 2);

        let async_chunks = storage.list_chunks_by_tag("async").unwrap();
        assert_eq!(async_chunks.len(), 1);
    }

    #[test]
    fn test_delete_chunk() {
        let storage = create_test_storage();

        let chunk = MemoryChunk::new("agent-001".to_string(), "To delete".to_string())
            .with_tags(vec!["tag1".to_string()]);
        let id = storage.store_chunk(&chunk).unwrap();

        let deleted = storage.delete_chunk(&id).unwrap();
        assert!(deleted);

        let retrieved = storage.get_chunk(&id).unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_create_and_get_session() {
        let storage = create_test_storage();

        let session = MemorySession::new("agent-001".to_string(), "Test Session".to_string())
            .with_description("A test session".to_string());

        let id = storage.create_session(&session).unwrap();
        assert_eq!(id, session.id);

        let retrieved = storage.get_session(&id).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.name, "Test Session");
        assert_eq!(retrieved.description, Some("A test session".to_string()));
    }

    #[test]
    fn test_list_sessions() {
        let storage = create_test_storage();

        let session1 = MemorySession::new("agent-001".to_string(), "Session 1".to_string());
        let session2 = MemorySession::new("agent-001".to_string(), "Session 2".to_string());
        let session3 = MemorySession::new("agent-002".to_string(), "Session 3".to_string());

        storage.create_session(&session1).unwrap();
        storage.create_session(&session2).unwrap();
        storage.create_session(&session3).unwrap();

        let sessions_agent1 = storage.list_sessions("agent-001").unwrap();
        assert_eq!(sessions_agent1.len(), 2);

        let sessions_agent2 = storage.list_sessions("agent-002").unwrap();
        assert_eq!(sessions_agent2.len(), 1);
    }

    #[test]
    fn test_delete_session_with_chunks() {
        let storage = create_test_storage();

        let session = MemorySession::new("agent-001".to_string(), "Session".to_string());
        storage.create_session(&session).unwrap();

        let chunk = MemoryChunk::new("agent-001".to_string(), "Content".to_string())
            .with_session(session.id.clone());
        storage.store_chunk(&chunk).unwrap();

        // Delete session with chunks
        let deleted = storage.delete_session(&session.id, true).unwrap();
        assert!(deleted);

        // Session should be gone
        let retrieved = storage.get_session(&session.id).unwrap();
        assert!(retrieved.is_none());

        // Chunks should also be gone
        let chunks = storage.list_chunks_for_session(&session.id).unwrap();
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_search_keyword() {
        let storage = create_test_storage();

        let chunk1 = MemoryChunk::new(
            "agent-001".to_string(),
            "Rust is a systems programming language".to_string(),
        );
        let chunk2 = MemoryChunk::new(
            "agent-001".to_string(),
            "Python is great for scripting".to_string(),
        );
        let chunk3 = MemoryChunk::new(
            "agent-001".to_string(),
            "Rust async patterns are useful".to_string(),
        );

        storage.store_chunk(&chunk1).unwrap();
        storage.store_chunk(&chunk2).unwrap();
        storage.store_chunk(&chunk3).unwrap();

        let query = MemorySearchQuery::new("agent-001".to_string())
            .with_query("rust".to_string())
            .with_mode(SearchMode::Keyword);

        let results = storage.search(&query).unwrap();
        assert_eq!(results.chunks.len(), 2);
        assert_eq!(results.total_count, 2);
    }

    #[test]
    fn test_indexed_search_respects_deletion() {
        let storage = create_test_storage();
        assert!(storage.has_text_index());

        let chunk = MemoryChunk::new("agent-001".to_string(), "Rust tokio runtime".to_string());
        storage.store_chunk(&chunk).unwrap();

        let query = MemorySearchQuery::new("agent-001".to_string())
            .with_query("rust tokio".to_string())
            .with_mode(SearchMode::Keyword);
        let before_delete = storage.search(&query).unwrap();
        assert_eq!(before_delete.total_count, 1);

        storage.delete_chunk(&chunk.id).unwrap();
        let after_delete = storage.search(&query).unwrap();
        assert_eq!(after_delete.total_count, 0);
    }

    #[test]
    fn test_rebuild_text_index_if_empty() {
        let storage = create_test_storage();
        let chunk = MemoryChunk::new("agent-001".to_string(), "index rebuild content".to_string());
        storage.store_chunk(&chunk).unwrap();

        let rebuilt = storage.rebuild_text_index_if_empty().unwrap();
        assert_eq!(rebuilt, 0);

        let rebuilt_force = storage.rebuild_text_index().unwrap();
        assert_eq!(rebuilt_force, 1);

        let query = MemorySearchQuery::new("agent-001".to_string())
            .with_query("rebuild".to_string())
            .with_mode(SearchMode::Keyword);
        let result = storage.search(&query).unwrap();
        assert_eq!(result.total_count, 1);
    }

    #[test]
    fn test_search_phrase() {
        let storage = create_test_storage();

        let chunk1 = MemoryChunk::new(
            "agent-001".to_string(),
            "Learning rust programming is fun".to_string(),
        );
        let chunk2 = MemoryChunk::new(
            "agent-001".to_string(),
            "Rust programming language".to_string(),
        );

        storage.store_chunk(&chunk1).unwrap();
        storage.store_chunk(&chunk2).unwrap();

        let query = MemorySearchQuery::new("agent-001".to_string())
            .with_query("rust programming".to_string())
            .with_mode(SearchMode::Phrase);

        let results = storage.search(&query).unwrap();
        assert_eq!(results.chunks.len(), 2);
    }

    #[test]
    fn test_search_regex() {
        let storage = create_test_storage();

        let chunk1 = MemoryChunk::new("agent-001".to_string(), "error: code 404".to_string());
        let chunk2 = MemoryChunk::new("agent-001".to_string(), "error: code 500".to_string());
        let chunk3 = MemoryChunk::new("agent-001".to_string(), "success: code 200".to_string());

        storage.store_chunk(&chunk1).unwrap();
        storage.store_chunk(&chunk2).unwrap();
        storage.store_chunk(&chunk3).unwrap();

        let query = MemorySearchQuery::new("agent-001".to_string())
            .with_query(r"error: code \d+".to_string())
            .with_mode(SearchMode::Regex);

        let results = storage.search(&query).unwrap();
        assert_eq!(results.chunks.len(), 2);
    }

    #[test]
    fn test_search_with_tag_filter() {
        let storage = create_test_storage();

        let chunk1 = MemoryChunk::new("agent-001".to_string(), "Rust content".to_string())
            .with_tags(vec!["rust".to_string(), "important".to_string()]);
        let chunk2 = MemoryChunk::new("agent-001".to_string(), "Python content".to_string())
            .with_tags(vec!["python".to_string(), "important".to_string()]);

        storage.store_chunk(&chunk1).unwrap();
        storage.store_chunk(&chunk2).unwrap();

        let query = MemorySearchQuery::new("agent-001".to_string())
            .with_tags(vec!["important".to_string(), "rust".to_string()]);

        let results = storage.search(&query).unwrap();
        assert_eq!(results.chunks.len(), 1);
        assert!(results.chunks[0].content.contains("Rust"));
    }

    #[test]
    fn test_search_with_source_filter() {
        let storage = create_test_storage();

        let chunk1 = MemoryChunk::new("agent-001".to_string(), "Task output".to_string())
            .with_source(MemorySource::TaskExecution {
                task_id: "task-1".to_string(),
            });
        let chunk2 = MemoryChunk::new("agent-001".to_string(), "Manual note".to_string())
            .with_source(MemorySource::ManualNote);

        storage.store_chunk(&chunk1).unwrap();
        storage.store_chunk(&chunk2).unwrap();

        let query = MemorySearchQuery::new("agent-001".to_string())
            .from_source(SourceTypeFilter::TaskExecution);

        let results = storage.search(&query).unwrap();
        assert_eq!(results.chunks.len(), 1);
        assert!(results.chunks[0].content.contains("Task"));
    }

    #[test]
    fn test_search_pagination() {
        let storage = create_test_storage();

        // Create 10 chunks
        for i in 0..10 {
            let chunk = MemoryChunk::new("agent-001".to_string(), format!("Content {}", i));
            storage.store_chunk(&chunk).unwrap();
        }

        let query = MemorySearchQuery::new("agent-001".to_string()).paginate(3, 0);

        let results = storage.search(&query).unwrap();
        assert_eq!(results.chunks.len(), 3);
        assert_eq!(results.total_count, 10);
        assert!(results.has_more);

        // Get next page
        let query = MemorySearchQuery::new("agent-001".to_string()).paginate(3, 3);
        let results = storage.search(&query).unwrap();
        assert_eq!(results.chunks.len(), 3);
        assert!(results.has_more);

        // Get last page
        let query = MemorySearchQuery::new("agent-001".to_string()).paginate(3, 9);
        let results = storage.search(&query).unwrap();
        assert_eq!(results.chunks.len(), 1);
        assert!(!results.has_more);
    }

    #[test]
    fn test_get_stats() {
        let storage = create_test_storage();

        let session = MemorySession::new("agent-001".to_string(), "Session".to_string());
        storage.create_session(&session).unwrap();

        let chunk1 = MemoryChunk::new("agent-001".to_string(), "Content 1".to_string())
            .with_token_count(100);
        let chunk2 = MemoryChunk::new("agent-001".to_string(), "Content 2".to_string())
            .with_token_count(150);

        storage.store_chunk(&chunk1).unwrap();
        storage.store_chunk(&chunk2).unwrap();

        let stats = storage.get_stats("agent-001").unwrap();
        assert_eq!(stats.agent_id, "agent-001");
        assert_eq!(stats.session_count, 1);
        assert_eq!(stats.chunk_count, 2);
        assert_eq!(stats.total_tokens, 250);
        assert!(stats.oldest_memory.is_some());
        assert!(stats.newest_memory.is_some());
    }

    #[test]
    fn test_refresh_session_stats() {
        let storage = create_test_storage();

        let session = MemorySession::new("agent-001".to_string(), "Session".to_string());
        storage.create_session(&session).unwrap();

        let chunk1 = MemoryChunk::new("agent-001".to_string(), "Content 1".to_string())
            .with_session(session.id.clone())
            .with_token_count(100);
        let chunk2 = MemoryChunk::new("agent-001".to_string(), "Content 2".to_string())
            .with_session(session.id.clone())
            .with_token_count(200);

        storage.store_chunk(&chunk1).unwrap();
        storage.store_chunk(&chunk2).unwrap();

        let updated = storage.refresh_session_stats(&session.id).unwrap();
        assert!(updated.is_some());
        let updated = updated.unwrap();
        assert_eq!(updated.chunk_count, 2);
        assert_eq!(updated.total_tokens, 300);
    }

    #[test]
    fn test_exists_by_content() {
        let storage = create_test_storage();

        let chunk = MemoryChunk::new("agent-001".to_string(), "Unique content".to_string());
        storage.store_chunk(&chunk).unwrap();

        let exists = storage.exists_by_content("Unique content").unwrap();
        assert!(exists.is_some());
        assert_eq!(exists.unwrap(), chunk.id);

        let not_exists = storage.exists_by_content("Different content").unwrap();
        assert!(not_exists.is_none());
    }

    #[test]
    fn test_delete_chunks_for_agent() {
        let storage = create_test_storage();

        let chunk1 = MemoryChunk::new("agent-001".to_string(), "Content 1".to_string())
            .with_tags(vec!["tag".to_string()]);
        let chunk2 = MemoryChunk::new("agent-001".to_string(), "Content 2".to_string());
        let chunk3 = MemoryChunk::new("agent-002".to_string(), "Content 3".to_string());

        storage.store_chunk(&chunk1).unwrap();
        storage.store_chunk(&chunk2).unwrap();
        storage.store_chunk(&chunk3).unwrap();

        let deleted = storage.delete_chunks_for_agent("agent-001").unwrap();
        assert_eq!(deleted, 2);

        let chunks_agent1 = storage.list_chunks("agent-001").unwrap();
        assert!(chunks_agent1.is_empty());

        // agent-002 chunks should still exist
        let chunks_agent2 = storage.list_chunks("agent-002").unwrap();
        assert_eq!(chunks_agent2.len(), 1);
    }

    #[test]
    fn test_cleanup_old_chunks() {
        let storage = create_test_storage();
        let now = chrono::Utc::now().timestamp_millis();

        let old_chunk = MemoryChunk::new("agent-1".to_string(), "old".to_string())
            .with_created_at(now - (120 * 24 * 60 * 60 * 1000));
        let recent_chunk = MemoryChunk::new("agent-1".to_string(), "recent".to_string())
            .with_created_at(now - (2 * 24 * 60 * 60 * 1000));

        storage.store_chunk(&old_chunk).unwrap();
        storage.store_chunk(&recent_chunk).unwrap();

        let cutoff = now - (90 * 24 * 60 * 60 * 1000);
        let deleted = storage.cleanup_old_chunks(cutoff).unwrap();
        assert_eq!(deleted, 1);
        assert!(storage.get_chunk(&old_chunk.id).unwrap().is_none());
        assert!(storage.get_chunk(&recent_chunk.id).unwrap().is_some());
    }

    #[test]
    fn test_search_time_range() {
        let storage = create_test_storage();

        let now = time_utils::now_ms();

        let chunk1 = MemoryChunk::new("agent-001".to_string(), "Old content".to_string())
            .with_created_at(now - 10000);
        let chunk2 = MemoryChunk::new("agent-001".to_string(), "New content".to_string())
            .with_created_at(now);

        storage.store_chunk(&chunk1).unwrap();
        storage.store_chunk(&chunk2).unwrap();

        // Search for recent chunks only
        let query =
            MemorySearchQuery::new("agent-001".to_string()).in_range(Some(now - 5000), None);

        let results = storage.search(&query).unwrap();
        assert_eq!(results.chunks.len(), 1);
        assert!(results.chunks[0].content.contains("New"));
    }

    #[test]
    fn test_vector_stats_without_vectors() {
        let storage = create_test_storage();
        let stats = storage.vector_stats().unwrap();
        assert!(stats.is_none());
    }

    #[test]
    fn test_cleanup_vector_orphans_without_vectors() {
        let storage = create_test_storage();
        let cleaned = storage.cleanup_vector_orphans().unwrap();
        assert!(cleaned.is_none());
    }

    /// Test concurrent chunk storage with deduplication at the typed layer.
    /// All threads storing the same content should get the same chunk ID.
    #[test]
    fn test_concurrent_typed_chunk_dedup() {
        use std::collections::HashSet;
        use std::thread;

        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = Arc::new(MemoryStorage::new(db).unwrap());

        let duplicate_content = "duplicate content for typed test";
        let num_threads = 10;

        let handles: Vec<_> = (0..num_threads)
            .map(|_| {
                let s = Arc::clone(&storage);
                let content = duplicate_content.to_string();
                thread::spawn(move || {
                    let chunk = MemoryChunk::new("agent-001".to_string(), content);
                    s.store_chunk(&chunk).unwrap()
                })
            })
            .collect();

        let ids: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        // All threads should return the same chunk ID
        let unique_ids: HashSet<_> = ids.iter().cloned().collect();
        assert_eq!(
            unique_ids.len(),
            1,
            "All threads should get the same chunk ID due to deduplication"
        );

        // Only one chunk should exist in storage
        let chunks = storage.list_chunks("agent-001").unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].content, duplicate_content);
    }

    /// Test concurrent delete_chunks_for_agent is safe.
    #[test]
    fn test_concurrent_delete_chunks_for_agent() {
        use std::thread;

        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = Arc::new(MemoryStorage::new(db).unwrap());

        // Create chunks
        for i in 0..20 {
            let chunk = MemoryChunk::new("agent-001".to_string(), format!("Content {}", i))
                .with_tags(vec!["tag".to_string()]);
            storage.store_chunk(&chunk).unwrap();
        }

        // Concurrent deletion attempts
        let handles: Vec<_> = (0..5)
            .map(|_| {
                let s = Arc::clone(&storage);
                thread::spawn(move || s.delete_chunks_for_agent("agent-001"))
            })
            .collect();

        for h in handles {
            // All should succeed (idempotent)
            let _ = h.join().unwrap();
        }

        // No chunks should remain
        let chunks = storage.list_chunks("agent-001").unwrap();
        assert!(chunks.is_empty());
    }
}
