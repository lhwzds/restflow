//! Vector storage using HNSW for approximate nearest neighbor search.
//!
//! Provides low-level vector storage with persistence to ReDB.
//! The HNSW index is kept in memory for fast search, with vectors
//! persisted to the database for durability.
//!
//! # Orphan Vector Handling
//!
//! HNSW indices do not support efficient vector deletion. When `delete()` is called,
//! the vector remains in the in-memory index but is removed from the id_map and
//! reverse_map, effectively marking it as "deleted" for search purposes.
//!
//! Over time, these orphan vectors can accumulate and consume memory. Use
//! `orphan_count()` to monitor and `cleanup_orphans()` to rebuild the index
//! and reclaim memory.

use anyhow::Result;
use hnsw_rs::prelude::*;
use parking_lot::RwLock;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

type VectorIndex = Hnsw<'static, f32, DistCosine>;

const VECTOR_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("memory_vectors");
const VECTOR_META_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("memory_vector_meta");

/// Configuration for vector storage.
#[derive(Debug, Clone)]
pub struct VectorConfig {
    /// Vector dimension (e.g., 1536 for OpenAI text-embedding-3-small)
    pub dimension: usize,
    /// Maximum number of connections per node (16-64 typical)
    pub max_connections: usize,
    /// Search width during construction (200-800 typical)
    pub ef_construction: usize,
    /// Maximum elements to store
    pub max_elements: usize,
}

impl Default for VectorConfig {
    fn default() -> Self {
        Self {
            dimension: 1536,
            max_connections: 16,
            ef_construction: 200,
            max_elements: 100_000,
        }
    }
}

/// Statistics about vector storage state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorStats {
    /// Number of active vectors (in id_map)
    pub active_count: usize,
    /// Number of orphan vectors in the HNSW index
    pub orphan_count: usize,
    /// Total vectors in the HNSW index
    pub total_indexed: usize,
}

/// Low-level vector storage with HNSW index.
pub struct VectorStorage {
    db: Arc<Database>,
    config: VectorConfig,
    /// HNSW index (in-memory, rebuilt on load)
    index: RwLock<VectorIndex>,
    /// chunk_id -> internal vector ID
    id_map: RwLock<HashMap<String, usize>>,
    /// internal vector ID -> chunk_id
    reverse_map: RwLock<HashMap<usize, String>>,
    /// Next available vector ID
    next_id: RwLock<usize>,
    /// Number of deleted vectors (orphan count)
    orphan_count: RwLock<usize>,
}

impl VectorStorage {
    /// Create new vector storage, loading existing vectors from DB.
    pub fn new(db: Arc<Database>, config: VectorConfig) -> Result<Self> {
        let write_txn = db.begin_write()?;
        write_txn.open_table(VECTOR_TABLE)?;
        write_txn.open_table(VECTOR_META_TABLE)?;
        write_txn.commit()?;

        let hnsw: VectorIndex = Hnsw::new(
            config.max_connections,
            config.max_elements,
            16,
            config.ef_construction,
            DistCosine,
        );

        let storage = Self {
            db,
            config,
            index: RwLock::new(hnsw),
            id_map: RwLock::new(HashMap::new()),
            reverse_map: RwLock::new(HashMap::new()),
            next_id: RwLock::new(0),
            orphan_count: RwLock::new(0),
        };

        storage.rebuild_index()?;
        Ok(storage)
    }

    /// Add a vector for a chunk.
    pub fn add(&self, chunk_id: &str, vector: &[f32]) -> Result<()> {
        if vector.len() != self.config.dimension {
            anyhow::bail!(
                "Vector dimension mismatch: expected {}, got {}",
                self.config.dimension,
                vector.len()
            );
        }

        if self.id_map.read().contains_key(chunk_id) {
            return self.update(chunk_id, vector);
        }

        let vector_id = {
            let mut next = self.next_id.write();
            let id = *next;
            *next += 1;
            id
        };

        {
            let index = self.index.write();
            index.insert((vector, vector_id));
        }

        {
            let mut id_map = self.id_map.write();
            let mut reverse = self.reverse_map.write();
            id_map.insert(chunk_id.to_string(), vector_id);
            reverse.insert(vector_id, chunk_id.to_string());
        }

        self.persist_vector(chunk_id, vector)?;
        Ok(())
    }

    /// Update an existing vector.
    pub fn update(&self, chunk_id: &str, vector: &[f32]) -> Result<()> {
        self.delete(chunk_id)?;
        self.add(chunk_id, vector)
    }

    /// Delete a vector.
    ///
    /// Note: HNSW does not support efficient deletion, so the vector remains
    /// in the in-memory index but is removed from the id_map. This creates
    /// an "orphan" vector that will be filtered out during search.
    /// Use `cleanup_orphans()` periodically to rebuild the index and reclaim memory.
    pub fn delete(&self, chunk_id: &str) -> Result<bool> {
        let vector_id = {
            let id_map = self.id_map.read();
            match id_map.get(chunk_id) {
                Some(&id) => id,
                None => return Ok(false),
            }
        };

        {
            let mut id_map = self.id_map.write();
            let mut reverse = self.reverse_map.write();
            id_map.remove(chunk_id);
            reverse.remove(&vector_id);
        }

        // Track orphan count (vector still in HNSW index but not in maps)
        {
            let mut orphan = self.orphan_count.write();
            *orphan += 1;
        }

        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(VECTOR_TABLE)?;
            table.remove(chunk_id)?;
        }
        write_txn.commit()?;

        Ok(true)
    }

    /// Search for similar vectors.
    pub fn search(
        &self,
        query: &[f32],
        top_k: usize,
        ef_search: usize,
    ) -> Result<Vec<(String, f32)>> {
        if query.len() != self.config.dimension {
            anyhow::bail!(
                "Query dimension mismatch: expected {}, got {}",
                self.config.dimension,
                query.len()
            );
        }

        let index = self.index.read();
        let reverse = self.reverse_map.read();
        let results = index.search(query, top_k, ef_search);
        Ok(results
            .into_iter()
            .filter_map(|item| {
                let chunk_id = reverse.get(&item.d_id)?;
                Some((chunk_id.clone(), item.distance))
            })
            .collect())
    }

    /// Search with filtering (only return IDs in allowed set).
    pub fn search_filtered(
        &self,
        query: &[f32],
        top_k: usize,
        ef_search: usize,
        allowed_ids: &[String],
    ) -> Result<Vec<(String, f32)>> {
        if query.len() != self.config.dimension {
            anyhow::bail!(
                "Query dimension mismatch: expected {}, got {}",
                self.config.dimension,
                query.len()
            );
        }

        let allowed_set: HashSet<&String> = allowed_ids.iter().collect();
        let id_map = self.id_map.read();
        let allowed_vector_ids: Vec<usize> = allowed_ids
            .iter()
            .filter_map(|chunk_id| id_map.get(chunk_id).copied())
            .collect();

        if allowed_vector_ids.is_empty() {
            return Ok(Vec::new());
        }

        let index = self.index.read();
        let reverse = self.reverse_map.read();
        // Search for more results than needed, then filter
        let search_k = top_k * 10; // Over-fetch to account for filtering
        let results = index.search(query, search_k, ef_search);

        Ok(results
            .into_iter()
            .filter_map(|item| {
                let chunk_id = reverse.get(&item.d_id)?;
                if allowed_set.contains(chunk_id) {
                    Some((chunk_id.clone(), item.distance))
                } else {
                    None
                }
            })
            .take(top_k)
            .collect())
    }

    /// Check if a chunk has a vector.
    pub fn has_vector(&self, chunk_id: &str) -> bool {
        self.id_map.read().contains_key(chunk_id)
    }

    /// Get active vector count (excludes orphans).
    pub fn count(&self) -> usize {
        self.id_map.read().len()
    }

    /// Get the number of orphan vectors in the index.
    ///
    /// Orphans are vectors that have been "deleted" but still exist in the
    /// HNSW index. They are filtered out during search but consume memory.
    pub fn orphan_count(&self) -> usize {
        *self.orphan_count.read()
    }

    /// Get statistics about the vector storage state.
    pub fn stats(&self) -> VectorStats {
        let active_count = self.id_map.read().len();
        let orphan_count = *self.orphan_count.read();
        let total_indexed = active_count + orphan_count;
        VectorStats {
            active_count,
            orphan_count,
            total_indexed,
        }
    }

    /// Clean up orphan vectors by rebuilding the index.
    ///
    /// This recreates the HNSW index with only active vectors, reclaiming
    /// memory from deleted vectors. This is an expensive operation and should
    /// be called periodically based on orphan count thresholds.
    ///
    /// Returns the number of orphans cleaned up.
    pub fn cleanup_orphans(&self) -> Result<usize> {
        let orphans_before = self.orphan_count();
        if orphans_before == 0 {
            return Ok(0);
        }

        self.rebuild_index()?;
        Ok(orphans_before)
    }

    fn persist_vector(&self, chunk_id: &str, vector: &[f32]) -> Result<()> {
        let bytes = bincode::serde::encode_to_vec(vector, bincode::config::standard())?;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(VECTOR_TABLE)?;
            table.insert(chunk_id, bytes.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn rebuild_index(&self) -> Result<()> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(VECTOR_TABLE)?;
        let mut vectors: Vec<(String, Vec<f32>)> = Vec::new();
        for item in table.iter()? {
            let (key, value) = item?;
            let chunk_id = key.value().to_string();
            let (vector, _): (Vec<f32>, usize) =
                bincode::serde::decode_from_slice(value.value(), bincode::config::standard())?;
            vectors.push((chunk_id, vector));
        }
        drop(read_txn);

        let mut index = self.index.write();
        let mut id_map = self.id_map.write();
        let mut reverse = self.reverse_map.write();
        let mut next_id = self.next_id.write();
        let mut orphan_count = self.orphan_count.write();

        *index = Hnsw::new(
            self.config.max_connections,
            self.config.max_elements,
            16,
            self.config.ef_construction,
            DistCosine,
        );

        id_map.clear();
        reverse.clear();
        *next_id = 0;
        *orphan_count = 0;

        for (chunk_id, vector) in vectors {
            let vector_id = *next_id;
            *next_id += 1;
            index.insert((vector.as_slice(), vector_id));
            id_map.insert(chunk_id.clone(), vector_id);
            reverse.insert(vector_id, chunk_id);
        }

        tracing::info!("Rebuilt vector index with {} vectors", id_map.len());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_storage(dim: usize) -> VectorStorage {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let config = VectorConfig {
            dimension: dim,
            max_connections: 8,
            ef_construction: 100,
            max_elements: 1000,
        };
        VectorStorage::new(db, config).unwrap()
    }

    #[test]
    fn test_add_and_search() {
        let storage = create_test_storage(4);
        storage.add("chunk-1", &[1.0, 0.0, 0.0, 0.0]).unwrap();
        storage.add("chunk-2", &[0.0, 1.0, 0.0, 0.0]).unwrap();
        storage.add("chunk-3", &[0.9, 0.1, 0.0, 0.0]).unwrap();

        let results = storage.search(&[1.0, 0.0, 0.0, 0.0], 2, 50).unwrap();
        assert!(!results.is_empty());
        let returned: Vec<&str> = results.iter().map(|item| item.0.as_str()).collect();
        assert!(returned.contains(&"chunk-1"));
    }

    #[test]
    fn test_dimension_validation() {
        let storage = create_test_storage(4);
        let result = storage.add("chunk-1", &[1.0, 0.0, 0.0]);
        assert!(result.is_err());
    }

    #[test]
    fn test_delete() {
        let storage = create_test_storage(4);
        storage.add("chunk-1", &[1.0, 0.0, 0.0, 0.0]).unwrap();
        assert!(storage.has_vector("chunk-1"));
        storage.delete("chunk-1").unwrap();
        assert!(!storage.has_vector("chunk-1"));
    }

    #[test]
    fn test_count() {
        let storage = create_test_storage(4);
        assert_eq!(storage.count(), 0);
        storage.add("chunk-1", &[1.0, 0.0, 0.0, 0.0]).unwrap();
        storage.add("chunk-2", &[0.0, 1.0, 0.0, 0.0]).unwrap();
        assert_eq!(storage.count(), 2);
    }

    #[test]
    fn test_orphan_tracking() {
        let storage = create_test_storage(4);

        // Initially no orphans
        assert_eq!(storage.orphan_count(), 0);
        let stats = storage.stats();
        assert_eq!(stats.active_count, 0);
        assert_eq!(stats.orphan_count, 0);

        // Add vectors
        storage.add("chunk-1", &[1.0, 0.0, 0.0, 0.0]).unwrap();
        storage.add("chunk-2", &[0.0, 1.0, 0.0, 0.0]).unwrap();
        assert_eq!(storage.count(), 2);
        assert_eq!(storage.orphan_count(), 0);

        // Delete one - creates orphan
        storage.delete("chunk-1").unwrap();
        assert_eq!(storage.count(), 1);
        assert_eq!(storage.orphan_count(), 1);

        let stats = storage.stats();
        assert_eq!(stats.active_count, 1);
        assert_eq!(stats.orphan_count, 1);
        assert_eq!(stats.total_indexed, 2);
    }

    #[test]
    fn test_cleanup_orphans() {
        let storage = create_test_storage(4);

        // Add and delete multiple vectors
        for i in 0..10 {
            let chunk_id = format!("chunk-{}", i);
            storage.add(&chunk_id, &[i as f32, 0.0, 0.0, 0.0]).unwrap();
        }
        assert_eq!(storage.count(), 10);
        assert_eq!(storage.orphan_count(), 0);

        // Delete 5 vectors
        for i in 0..5 {
            let chunk_id = format!("chunk-{}", i);
            storage.delete(&chunk_id).unwrap();
        }
        assert_eq!(storage.count(), 5);
        assert_eq!(storage.orphan_count(), 5);

        // Cleanup orphans
        let cleaned = storage.cleanup_orphans().unwrap();
        assert_eq!(cleaned, 5);
        assert_eq!(storage.orphan_count(), 0);
        assert_eq!(storage.count(), 5);

        let stats = storage.stats();
        assert_eq!(stats.active_count, 5);
        assert_eq!(stats.orphan_count, 0);
        assert_eq!(stats.total_indexed, 5);
    }

    #[test]
    fn test_cleanup_orphans_when_none() {
        let storage = create_test_storage(4);
        storage.add("chunk-1", &[1.0, 0.0, 0.0, 0.0]).unwrap();

        // Cleanup when no orphans should return 0
        let cleaned = storage.cleanup_orphans().unwrap();
        assert_eq!(cleaned, 0);
        assert_eq!(storage.count(), 1);
    }

    #[test]
    fn test_deleted_vector_not_in_search() {
        let storage = create_test_storage(4);

        // Add vectors
        storage.add("chunk-1", &[1.0, 0.0, 0.0, 0.0]).unwrap();
        storage.add("chunk-2", &[0.99, 0.01, 0.0, 0.0]).unwrap();

        // Search should return at least one active vector
        let results = storage.search(&[1.0, 0.0, 0.0, 0.0], 10, 50).unwrap();
        assert!(!results.is_empty());
        let before_delete_ids: Vec<_> = results.iter().map(|(id, _)| id.as_str()).collect();
        assert!(before_delete_ids.contains(&"chunk-1"));

        // Delete one
        storage.delete("chunk-2").unwrap();

        // Search should only return active vector (orphan filtered out)
        let results = storage.search(&[1.0, 0.0, 0.0, 0.0], 10, 50).unwrap();
        assert!(!results.is_empty());
        let after_delete_ids: Vec<_> = results.iter().map(|(id, _)| id.as_str()).collect();
        assert!(after_delete_ids.contains(&"chunk-1"));
        assert!(!after_delete_ids.contains(&"chunk-2"));
    }

    #[test]
    fn test_rebuild_preserves_vectors() {
        let storage = create_test_storage(4);

        // Add vectors
        storage.add("chunk-1", &[1.0, 0.0, 0.0, 0.0]).unwrap();
        storage.add("chunk-2", &[0.0, 1.0, 0.0, 0.0]).unwrap();

        // Create orphans
        storage.add("chunk-3", &[0.0, 0.0, 1.0, 0.0]).unwrap();
        storage.delete("chunk-3").unwrap();

        // Cleanup (rebuilds index)
        let cleaned = storage.cleanup_orphans().unwrap();
        assert_eq!(cleaned, 1);
        assert_eq!(storage.count(), 2);

        // Active vectors should still be searchable after rebuild.
        let search_chunk_1 = storage.search(&[1.0, 0.0, 0.0, 0.0], 10, 100).unwrap();
        let ids_chunk_1: Vec<_> = search_chunk_1.iter().map(|(id, _)| id.as_str()).collect();
        assert!(ids_chunk_1.contains(&"chunk-1"));

        let search_chunk_2 = storage.search(&[0.0, 1.0, 0.0, 0.0], 10, 100).unwrap();
        let ids_chunk_2: Vec<_> = search_chunk_2.iter().map(|(id, _)| id.as_str()).collect();
        assert!(ids_chunk_2.contains(&"chunk-2"));

        // Deleted vectors should not reappear after rebuild.
        assert!(!ids_chunk_1.contains(&"chunk-3"));
        assert!(!ids_chunk_2.contains(&"chunk-3"));
    }
}
