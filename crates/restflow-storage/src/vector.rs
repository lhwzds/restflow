//! Vector storage using HNSW for approximate nearest neighbor search.
//!
//! Provides low-level vector storage with persistence to ReDB.
//! The HNSW index is kept in memory for fast search, with vectors
//! persisted to the database for durability.

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

    /// Get vector count.
    pub fn count(&self) -> usize {
        self.id_map.read().len()
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
}
