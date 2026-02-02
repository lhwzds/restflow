//! Memory storage - byte-level API for long-term memory persistence.
//!
//! Provides low-level storage operations for memory chunks and sessions
//! using the redb embedded database. Supports indexing by agent_id, session_id,
//! content hash (for deduplication), and tags.
//!
//! # Tables
//!
//! - `memory_chunks`: chunk_id -> chunk_data
//! - `memory_sessions`: session_id -> session_data
//! - `memory_agent_index`: agent_id:chunk_id -> chunk_id (for listing by agent)
//! - `memory_session_index`: session_id:chunk_id -> chunk_id (for listing by session)
//! - `memory_hash_index`: content_hash -> chunk_id (for deduplication)
//! - `memory_tag_index`: tag:chunk_id -> chunk_id (for tag filtering)

use anyhow::Result;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::sync::Arc;

const MEMORY_CHUNK_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("memory_chunks");
const MEMORY_SESSION_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("memory_sessions");

/// Index: agent_id:chunk_id -> chunk_id
const AGENT_INDEX_TABLE: TableDefinition<&str, &str> = TableDefinition::new("memory_agent_index");
/// Index: session_id:chunk_id -> chunk_id
const SESSION_INDEX_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("memory_session_index");
/// Index: content_hash -> chunk_id (for deduplication)
const HASH_INDEX_TABLE: TableDefinition<&str, &str> = TableDefinition::new("memory_hash_index");
/// Index: tag:chunk_id -> chunk_id (for tag filtering)
const TAG_INDEX_TABLE: TableDefinition<&str, &str> = TableDefinition::new("memory_tag_index");
/// Index: agent_id:session_id -> session_id (for listing sessions by agent)
const AGENT_SESSION_INDEX_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("memory_agent_session_index");

/// Low-level memory storage with byte-level API
#[derive(Clone)]
pub struct MemoryStorage {
    db: Arc<Database>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PutResult {
    Created(String),
    Existing(String),
}

impl MemoryStorage {
    /// Create a new MemoryStorage instance
    pub fn new(db: Arc<Database>) -> Result<Self> {
        // Initialize all tables
        let write_txn = db.begin_write()?;
        write_txn.open_table(MEMORY_CHUNK_TABLE)?;
        write_txn.open_table(MEMORY_SESSION_TABLE)?;
        write_txn.open_table(AGENT_INDEX_TABLE)?;
        write_txn.open_table(SESSION_INDEX_TABLE)?;
        write_txn.open_table(HASH_INDEX_TABLE)?;
        write_txn.open_table(TAG_INDEX_TABLE)?;
        write_txn.open_table(AGENT_SESSION_INDEX_TABLE)?;
        write_txn.commit()?;

        Ok(Self { db })
    }

    // ============== Memory Chunk Operations ==============

    /// Store a raw memory chunk with all necessary indexes.
    ///
    /// # Arguments
    /// - `chunk_id`: Unique identifier for the chunk
    /// - `agent_id`: Agent this chunk belongs to
    /// - `session_id`: Optional session ID for grouping
    /// - `content_hash`: SHA-256 hash of content for deduplication
    /// - `tags`: List of tags for filtering
    /// - `data`: Serialized chunk data
    pub fn put_chunk_raw(
        &self,
        chunk_id: &str,
        agent_id: &str,
        session_id: Option<&str>,
        content_hash: &str,
        tags: &[String],
        data: &[u8],
    ) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            // Store the chunk data
            let mut chunk_table = write_txn.open_table(MEMORY_CHUNK_TABLE)?;
            chunk_table.insert(chunk_id, data)?;

            // Index by agent_id
            let mut agent_index = write_txn.open_table(AGENT_INDEX_TABLE)?;
            let agent_key = format!("{}:{}", agent_id, chunk_id);
            agent_index.insert(agent_key.as_str(), chunk_id)?;

            // Index by session_id if provided
            if let Some(sid) = session_id {
                let mut session_index = write_txn.open_table(SESSION_INDEX_TABLE)?;
                let session_key = format!("{}:{}", sid, chunk_id);
                session_index.insert(session_key.as_str(), chunk_id)?;
            }

            // Index by content hash for deduplication checking
            let mut hash_index = write_txn.open_table(HASH_INDEX_TABLE)?;
            hash_index.insert(content_hash, chunk_id)?;

            // Index by tags
            let mut tag_index = write_txn.open_table(TAG_INDEX_TABLE)?;
            for tag in tags {
                let tag_key = format!("{}:{}", tag, chunk_id);
                tag_index.insert(tag_key.as_str(), chunk_id)?;
            }
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Store a chunk if it does not already exist for the content hash.
    pub fn put_chunk_if_not_exists(
        &self,
        chunk_id: &str,
        agent_id: &str,
        session_id: Option<&str>,
        content_hash: &str,
        tags: &[String],
        data: &[u8],
    ) -> Result<PutResult> {
        let write_txn = self.db.begin_write()?;
        let result = {
            let existing = {
                let hash_index = write_txn.open_table(HASH_INDEX_TABLE)?;
                hash_index.get(content_hash)?.map(|value| value.value().to_string())
            };

            if let Some(existing_id) = existing {
                PutResult::Existing(existing_id)
            } else {
                let mut chunk_table = write_txn.open_table(MEMORY_CHUNK_TABLE)?;
                chunk_table.insert(chunk_id, data)?;

                let mut agent_index = write_txn.open_table(AGENT_INDEX_TABLE)?;
                let agent_key = format!("{}:{}", agent_id, chunk_id);
                agent_index.insert(agent_key.as_str(), chunk_id)?;

                if let Some(sid) = session_id {
                    let mut session_index = write_txn.open_table(SESSION_INDEX_TABLE)?;
                    let session_key = format!("{}:{}", sid, chunk_id);
                    session_index.insert(session_key.as_str(), chunk_id)?;
                }

                let mut hash_index = write_txn.open_table(HASH_INDEX_TABLE)?;
                hash_index.insert(content_hash, chunk_id)?;

                let mut tag_index = write_txn.open_table(TAG_INDEX_TABLE)?;
                for tag in tags {
                    let tag_key = format!("{}:{}", tag, chunk_id);
                    tag_index.insert(tag_key.as_str(), chunk_id)?;
                }

                PutResult::Created(chunk_id.to_string())
            }
        };
        write_txn.commit()?;
        Ok(result)
    }

    /// Get raw chunk data by ID
    pub fn get_chunk_raw(&self, chunk_id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(MEMORY_CHUNK_TABLE)?;

        if let Some(value) = table.get(chunk_id)? {
            Ok(Some(value.value().to_vec()))
        } else {
            Ok(None)
        }
    }

    /// List all chunks for an agent
    pub fn list_chunks_by_agent_raw(&self, agent_id: &str) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let agent_index = read_txn.open_table(AGENT_INDEX_TABLE)?;
        let chunk_table = read_txn.open_table(MEMORY_CHUNK_TABLE)?;

        let prefix = format!("{}:", agent_id);
        let mut chunks = Vec::new();

        for item in agent_index.iter()? {
            let (key, value) = item?;
            let key_str = key.value();

            if key_str.starts_with(&prefix) {
                let chunk_id = value.value();
                if let Some(chunk_data) = chunk_table.get(chunk_id)? {
                    chunks.push((chunk_id.to_string(), chunk_data.value().to_vec()));
                }
            }
        }

        Ok(chunks)
    }

    /// List all chunks for a session
    pub fn list_chunks_by_session_raw(&self, session_id: &str) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let session_index = read_txn.open_table(SESSION_INDEX_TABLE)?;
        let chunk_table = read_txn.open_table(MEMORY_CHUNK_TABLE)?;

        let prefix = format!("{}:", session_id);
        let mut chunks = Vec::new();

        for item in session_index.iter()? {
            let (key, value) = item?;
            let key_str = key.value();

            if key_str.starts_with(&prefix) {
                let chunk_id = value.value();
                if let Some(chunk_data) = chunk_table.get(chunk_id)? {
                    chunks.push((chunk_id.to_string(), chunk_data.value().to_vec()));
                }
            }
        }

        Ok(chunks)
    }

    /// List all chunks with a specific tag
    pub fn list_chunks_by_tag_raw(&self, tag: &str) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let tag_index = read_txn.open_table(TAG_INDEX_TABLE)?;
        let chunk_table = read_txn.open_table(MEMORY_CHUNK_TABLE)?;

        let prefix = format!("{}:", tag);
        let mut chunks = Vec::new();

        for item in tag_index.iter()? {
            let (key, value) = item?;
            let key_str = key.value();

            if key_str.starts_with(&prefix) {
                let chunk_id = value.value();
                if let Some(chunk_data) = chunk_table.get(chunk_id)? {
                    chunks.push((chunk_id.to_string(), chunk_data.value().to_vec()));
                }
            }
        }

        Ok(chunks)
    }

    /// Check if a chunk with the given content hash already exists.
    /// Returns the existing chunk ID if found.
    pub fn find_by_hash(&self, content_hash: &str) -> Result<Option<String>> {
        let read_txn = self.db.begin_read()?;
        let hash_index = read_txn.open_table(HASH_INDEX_TABLE)?;

        if let Some(value) = hash_index.get(content_hash)? {
            Ok(Some(value.value().to_string()))
        } else {
            Ok(None)
        }
    }

    /// Delete a chunk and all its index entries.
    ///
    /// Note: Requires the metadata to be known for proper index cleanup.
    pub fn delete_chunk(
        &self,
        chunk_id: &str,
        agent_id: &str,
        session_id: Option<&str>,
        content_hash: &str,
        tags: &[String],
    ) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let existed = {
            // Delete the chunk
            let mut chunk_table = write_txn.open_table(MEMORY_CHUNK_TABLE)?;
            let existed = chunk_table.remove(chunk_id)?.is_some();

            // Remove from agent index
            let mut agent_index = write_txn.open_table(AGENT_INDEX_TABLE)?;
            let agent_key = format!("{}:{}", agent_id, chunk_id);
            agent_index.remove(agent_key.as_str())?;

            // Remove from session index if applicable
            if let Some(sid) = session_id {
                let mut session_index = write_txn.open_table(SESSION_INDEX_TABLE)?;
                let session_key = format!("{}:{}", sid, chunk_id);
                session_index.remove(session_key.as_str())?;
            }

            // Remove from hash index
            let mut hash_index = write_txn.open_table(HASH_INDEX_TABLE)?;
            hash_index.remove(content_hash)?;

            // Remove from tag indexes
            let mut tag_index = write_txn.open_table(TAG_INDEX_TABLE)?;
            for tag in tags {
                let tag_key = format!("{}:{}", tag, chunk_id);
                tag_index.remove(tag_key.as_str())?;
            }

            existed
        };
        write_txn.commit()?;
        Ok(existed)
    }

    /// Count chunks for an agent
    pub fn count_chunks_by_agent(&self, agent_id: &str) -> Result<u32> {
        let read_txn = self.db.begin_read()?;
        let agent_index = read_txn.open_table(AGENT_INDEX_TABLE)?;

        let prefix = format!("{}:", agent_id);
        let mut count = 0u32;

        for item in agent_index.iter()? {
            let (key, _) = item?;
            if key.value().starts_with(&prefix) {
                count += 1;
            }
        }

        Ok(count)
    }

    // ============== Memory Session Operations ==============

    /// Store a raw memory session
    pub fn put_session_raw(&self, session_id: &str, agent_id: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut session_table = write_txn.open_table(MEMORY_SESSION_TABLE)?;
            session_table.insert(session_id, data)?;

            // Index by agent_id
            let mut agent_session_index = write_txn.open_table(AGENT_SESSION_INDEX_TABLE)?;
            let index_key = format!("{}:{}", agent_id, session_id);
            agent_session_index.insert(index_key.as_str(), session_id)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get raw session data by ID
    pub fn get_session_raw(&self, session_id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(MEMORY_SESSION_TABLE)?;

        if let Some(value) = table.get(session_id)? {
            Ok(Some(value.value().to_vec()))
        } else {
            Ok(None)
        }
    }

    /// List all sessions for an agent
    pub fn list_sessions_by_agent_raw(&self, agent_id: &str) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let agent_session_index = read_txn.open_table(AGENT_SESSION_INDEX_TABLE)?;
        let session_table = read_txn.open_table(MEMORY_SESSION_TABLE)?;

        let prefix = format!("{}:", agent_id);
        let mut sessions = Vec::new();

        for item in agent_session_index.iter()? {
            let (key, value) = item?;
            let key_str = key.value();

            if key_str.starts_with(&prefix) {
                let session_id = value.value();
                if let Some(session_data) = session_table.get(session_id)? {
                    sessions.push((session_id.to_string(), session_data.value().to_vec()));
                }
            }
        }

        Ok(sessions)
    }

    /// Delete a session (does not delete associated chunks)
    pub fn delete_session(&self, session_id: &str, agent_id: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let existed = {
            let mut session_table = write_txn.open_table(MEMORY_SESSION_TABLE)?;
            let existed = session_table.remove(session_id)?.is_some();

            // Remove from agent session index
            let mut agent_session_index = write_txn.open_table(AGENT_SESSION_INDEX_TABLE)?;
            let index_key = format!("{}:{}", agent_id, session_id);
            agent_session_index.remove(index_key.as_str())?;

            existed
        };
        write_txn.commit()?;
        Ok(existed)
    }

    /// Count sessions for an agent
    pub fn count_sessions_by_agent(&self, agent_id: &str) -> Result<u32> {
        let read_txn = self.db.begin_read()?;
        let agent_session_index = read_txn.open_table(AGENT_SESSION_INDEX_TABLE)?;

        let prefix = format!("{}:", agent_id);
        let mut count = 0u32;

        for item in agent_session_index.iter()? {
            let (key, _) = item?;
            if key.value().starts_with(&prefix) {
                count += 1;
            }
        }

        Ok(count)
    }

    // ============== Bulk Operations ==============

    /// Delete all chunks for an agent with full index cleanup.
    pub fn delete_all_chunks_for_agent_with_metadata(
        &self,
        agent_id: &str,
        chunk_metadata: &[(String, Option<String>, String, Vec<String>)],
    ) -> Result<u32> {
        if chunk_metadata.is_empty() {
            return Ok(0);
        }

        let write_txn = self.db.begin_write()?;
        let count = {
            let mut chunk_table = write_txn.open_table(MEMORY_CHUNK_TABLE)?;
            let mut agent_index = write_txn.open_table(AGENT_INDEX_TABLE)?;
            let mut session_index = write_txn.open_table(SESSION_INDEX_TABLE)?;
            let mut hash_index = write_txn.open_table(HASH_INDEX_TABLE)?;
            let mut tag_index = write_txn.open_table(TAG_INDEX_TABLE)?;
            let mut deleted = 0u32;

            for (chunk_id, session_id, content_hash, tags) in chunk_metadata {
                chunk_table.remove(chunk_id.as_str())?;
                let agent_key = format!("{}:{}", agent_id, chunk_id);
                agent_index.remove(agent_key.as_str())?;

                if let Some(sid) = session_id {
                    let session_key = format!("{}:{}", sid, chunk_id);
                    session_index.remove(session_key.as_str())?;
                }

                hash_index.remove(content_hash.as_str())?;

                for tag in tags {
                    let tag_key = format!("{}:{}", tag, chunk_id);
                    tag_index.remove(tag_key.as_str())?;
                }

                deleted += 1;
            }

            deleted
        };
        write_txn.commit()?;
        Ok(count)
    }

    /// Delete all chunks for an agent
    pub fn delete_all_chunks_for_agent(&self, agent_id: &str) -> Result<u32> {
        // First collect all chunk IDs for this agent
        let chunks = self.list_chunks_by_agent_raw(agent_id)?;
        let count = chunks.len() as u32;

        if count == 0 {
            return Ok(0);
        }

        // We need to deserialize chunks to get their metadata for index cleanup.
        // This is a limitation of the byte-level API - the typed layer handles this better.
        // For now, we'll just delete from the primary table and agent index.
        // Full index cleanup requires the typed layer.

        let write_txn = self.db.begin_write()?;
        {
            let mut chunk_table = write_txn.open_table(MEMORY_CHUNK_TABLE)?;
            let mut agent_index = write_txn.open_table(AGENT_INDEX_TABLE)?;

            for (chunk_id, _) in &chunks {
                chunk_table.remove(chunk_id.as_str())?;
                let agent_key = format!("{}:{}", agent_id, chunk_id);
                agent_index.remove(agent_key.as_str())?;
            }
        }
        write_txn.commit()?;

        Ok(count)
    }

    /// Delete all sessions for an agent
    pub fn delete_all_sessions_for_agent(&self, agent_id: &str) -> Result<u32> {
        let sessions = self.list_sessions_by_agent_raw(agent_id)?;
        let count = sessions.len() as u32;

        if count == 0 {
            return Ok(0);
        }

        let write_txn = self.db.begin_write()?;
        {
            let mut session_table = write_txn.open_table(MEMORY_SESSION_TABLE)?;
            let mut agent_session_index = write_txn.open_table(AGENT_SESSION_INDEX_TABLE)?;

            for (session_id, _) in &sessions {
                session_table.remove(session_id.as_str())?;
                let index_key = format!("{}:{}", agent_id, session_id);
                agent_session_index.remove(index_key.as_str())?;
            }
        }
        write_txn.commit()?;

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_storage() -> MemoryStorage {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        MemoryStorage::new(db).unwrap()
    }

    #[test]
    fn test_put_and_get_chunk_raw() {
        let storage = create_test_storage();

        let data = b"test chunk data";
        storage
            .put_chunk_raw(
                "chunk-001",
                "agent-001",
                Some("session-001"),
                "hash123",
                &["tag1".to_string(), "tag2".to_string()],
                data,
            )
            .unwrap();

        let retrieved = storage.get_chunk_raw("chunk-001").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), data);
    }

    #[test]
    fn test_get_nonexistent_chunk() {
        let storage = create_test_storage();

        let result = storage.get_chunk_raw("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_list_chunks_by_agent() {
        let storage = create_test_storage();

        // Add chunks for agent-001
        storage
            .put_chunk_raw("chunk-001", "agent-001", None, "hash1", &[], b"data1")
            .unwrap();
        storage
            .put_chunk_raw("chunk-002", "agent-001", None, "hash2", &[], b"data2")
            .unwrap();

        // Add chunk for agent-002
        storage
            .put_chunk_raw("chunk-003", "agent-002", None, "hash3", &[], b"data3")
            .unwrap();

        let chunks_agent1 = storage.list_chunks_by_agent_raw("agent-001").unwrap();
        assert_eq!(chunks_agent1.len(), 2);

        let chunks_agent2 = storage.list_chunks_by_agent_raw("agent-002").unwrap();
        assert_eq!(chunks_agent2.len(), 1);

        let chunks_agent3 = storage.list_chunks_by_agent_raw("agent-003").unwrap();
        assert_eq!(chunks_agent3.len(), 0);
    }

    #[test]
    fn test_list_chunks_by_session() {
        let storage = create_test_storage();

        // Add chunks for session-001
        storage
            .put_chunk_raw(
                "chunk-001",
                "agent-001",
                Some("session-001"),
                "hash1",
                &[],
                b"data1",
            )
            .unwrap();
        storage
            .put_chunk_raw(
                "chunk-002",
                "agent-001",
                Some("session-001"),
                "hash2",
                &[],
                b"data2",
            )
            .unwrap();

        // Add chunk for session-002
        storage
            .put_chunk_raw(
                "chunk-003",
                "agent-001",
                Some("session-002"),
                "hash3",
                &[],
                b"data3",
            )
            .unwrap();

        let chunks_session1 = storage.list_chunks_by_session_raw("session-001").unwrap();
        assert_eq!(chunks_session1.len(), 2);

        let chunks_session2 = storage.list_chunks_by_session_raw("session-002").unwrap();
        assert_eq!(chunks_session2.len(), 1);
    }

    #[test]
    fn test_list_chunks_by_tag() {
        let storage = create_test_storage();

        storage
            .put_chunk_raw(
                "chunk-001",
                "agent-001",
                None,
                "hash1",
                &["rust".to_string(), "async".to_string()],
                b"data1",
            )
            .unwrap();
        storage
            .put_chunk_raw(
                "chunk-002",
                "agent-001",
                None,
                "hash2",
                &["rust".to_string()],
                b"data2",
            )
            .unwrap();
        storage
            .put_chunk_raw(
                "chunk-003",
                "agent-001",
                None,
                "hash3",
                &["python".to_string()],
                b"data3",
            )
            .unwrap();

        let rust_chunks = storage.list_chunks_by_tag_raw("rust").unwrap();
        assert_eq!(rust_chunks.len(), 2);

        let async_chunks = storage.list_chunks_by_tag_raw("async").unwrap();
        assert_eq!(async_chunks.len(), 1);

        let python_chunks = storage.list_chunks_by_tag_raw("python").unwrap();
        assert_eq!(python_chunks.len(), 1);
    }

    #[test]
    fn test_find_by_hash_deduplication() {
        let storage = create_test_storage();

        // Store first chunk
        storage
            .put_chunk_raw("chunk-001", "agent-001", None, "unique-hash", &[], b"data")
            .unwrap();

        // Check for existing hash
        let existing = storage.find_by_hash("unique-hash").unwrap();
        assert!(existing.is_some());
        assert_eq!(existing.unwrap(), "chunk-001");

        // Check for non-existing hash
        let not_found = storage.find_by_hash("other-hash").unwrap();
        assert!(not_found.is_none());
    }

    #[test]
    fn test_delete_chunk() {
        let storage = create_test_storage();

        storage
            .put_chunk_raw(
                "chunk-001",
                "agent-001",
                Some("session-001"),
                "hash123",
                &["tag1".to_string()],
                b"data",
            )
            .unwrap();

        let deleted = storage
            .delete_chunk(
                "chunk-001",
                "agent-001",
                Some("session-001"),
                "hash123",
                &["tag1".to_string()],
            )
            .unwrap();
        assert!(deleted);

        // Chunk should be gone
        let retrieved = storage.get_chunk_raw("chunk-001").unwrap();
        assert!(retrieved.is_none());

        // Agent index should be empty
        let chunks = storage.list_chunks_by_agent_raw("agent-001").unwrap();
        assert!(chunks.is_empty());

        // Session index should be empty
        let session_chunks = storage.list_chunks_by_session_raw("session-001").unwrap();
        assert!(session_chunks.is_empty());

        // Hash index should be empty
        let hash_result = storage.find_by_hash("hash123").unwrap();
        assert!(hash_result.is_none());

        // Tag index should be empty
        let tag_chunks = storage.list_chunks_by_tag_raw("tag1").unwrap();
        assert!(tag_chunks.is_empty());
    }

    #[test]
    fn test_count_chunks_by_agent() {
        let storage = create_test_storage();

        storage
            .put_chunk_raw("chunk-001", "agent-001", None, "hash1", &[], b"data1")
            .unwrap();
        storage
            .put_chunk_raw("chunk-002", "agent-001", None, "hash2", &[], b"data2")
            .unwrap();
        storage
            .put_chunk_raw("chunk-003", "agent-002", None, "hash3", &[], b"data3")
            .unwrap();

        assert_eq!(storage.count_chunks_by_agent("agent-001").unwrap(), 2);
        assert_eq!(storage.count_chunks_by_agent("agent-002").unwrap(), 1);
        assert_eq!(storage.count_chunks_by_agent("agent-003").unwrap(), 0);
    }

    #[test]
    fn test_put_and_get_session_raw() {
        let storage = create_test_storage();

        let data = b"session data";
        storage
            .put_session_raw("session-001", "agent-001", data)
            .unwrap();

        let retrieved = storage.get_session_raw("session-001").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), data);
    }

    #[test]
    fn test_list_sessions_by_agent() {
        let storage = create_test_storage();

        storage
            .put_session_raw("session-001", "agent-001", b"data1")
            .unwrap();
        storage
            .put_session_raw("session-002", "agent-001", b"data2")
            .unwrap();
        storage
            .put_session_raw("session-003", "agent-002", b"data3")
            .unwrap();

        let sessions_agent1 = storage.list_sessions_by_agent_raw("agent-001").unwrap();
        assert_eq!(sessions_agent1.len(), 2);

        let sessions_agent2 = storage.list_sessions_by_agent_raw("agent-002").unwrap();
        assert_eq!(sessions_agent2.len(), 1);
    }

    #[test]
    fn test_delete_session() {
        let storage = create_test_storage();

        storage
            .put_session_raw("session-001", "agent-001", b"data")
            .unwrap();

        let deleted = storage.delete_session("session-001", "agent-001").unwrap();
        assert!(deleted);

        let retrieved = storage.get_session_raw("session-001").unwrap();
        assert!(retrieved.is_none());

        let sessions = storage.list_sessions_by_agent_raw("agent-001").unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn test_count_sessions_by_agent() {
        let storage = create_test_storage();

        storage
            .put_session_raw("session-001", "agent-001", b"data1")
            .unwrap();
        storage
            .put_session_raw("session-002", "agent-001", b"data2")
            .unwrap();
        storage
            .put_session_raw("session-003", "agent-002", b"data3")
            .unwrap();

        assert_eq!(storage.count_sessions_by_agent("agent-001").unwrap(), 2);
        assert_eq!(storage.count_sessions_by_agent("agent-002").unwrap(), 1);
        assert_eq!(storage.count_sessions_by_agent("agent-003").unwrap(), 0);
    }

    #[test]
    fn test_delete_all_chunks_for_agent() {
        let storage = create_test_storage();

        storage
            .put_chunk_raw("chunk-001", "agent-001", None, "hash1", &[], b"data1")
            .unwrap();
        storage
            .put_chunk_raw("chunk-002", "agent-001", None, "hash2", &[], b"data2")
            .unwrap();
        storage
            .put_chunk_raw("chunk-003", "agent-002", None, "hash3", &[], b"data3")
            .unwrap();

        let deleted = storage.delete_all_chunks_for_agent("agent-001").unwrap();
        assert_eq!(deleted, 2);

        let chunks_agent1 = storage.list_chunks_by_agent_raw("agent-001").unwrap();
        assert!(chunks_agent1.is_empty());

        // agent-002 chunks should still exist
        let chunks_agent2 = storage.list_chunks_by_agent_raw("agent-002").unwrap();
        assert_eq!(chunks_agent2.len(), 1);
    }

    #[test]
    fn test_delete_all_sessions_for_agent() {
        let storage = create_test_storage();

        storage
            .put_session_raw("session-001", "agent-001", b"data1")
            .unwrap();
        storage
            .put_session_raw("session-002", "agent-001", b"data2")
            .unwrap();
        storage
            .put_session_raw("session-003", "agent-002", b"data3")
            .unwrap();

        let deleted = storage.delete_all_sessions_for_agent("agent-001").unwrap();
        assert_eq!(deleted, 2);

        let sessions_agent1 = storage.list_sessions_by_agent_raw("agent-001").unwrap();
        assert!(sessions_agent1.is_empty());

        // agent-002 sessions should still exist
        let sessions_agent2 = storage.list_sessions_by_agent_raw("agent-002").unwrap();
        assert_eq!(sessions_agent2.len(), 1);
    }

    #[test]
    fn test_update_chunk() {
        let storage = create_test_storage();

        storage
            .put_chunk_raw("chunk-001", "agent-001", None, "hash1", &[], b"original")
            .unwrap();
        storage
            .put_chunk_raw("chunk-001", "agent-001", None, "hash1", &[], b"updated")
            .unwrap();

        let retrieved = storage.get_chunk_raw("chunk-001").unwrap();
        assert_eq!(retrieved.unwrap(), b"updated");
    }

    #[test]
    fn test_update_session() {
        let storage = create_test_storage();

        storage
            .put_session_raw("session-001", "agent-001", b"original")
            .unwrap();
        storage
            .put_session_raw("session-001", "agent-001", b"updated")
            .unwrap();

        let retrieved = storage.get_session_raw("session-001").unwrap();
        assert_eq!(retrieved.unwrap(), b"updated");
    }
}
