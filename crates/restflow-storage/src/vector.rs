//! Process-local vector storage.

use crate::simple_storage::namespace_for_db;
use anyhow::Result;
use redb::Database;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

type VectorMap = HashMap<String, Vec<f32>>;
type VectorStores = HashMap<usize, VectorMap>;

#[derive(Debug, Clone)]
pub struct VectorConfig {
    pub dimension: usize,
    pub max_connections: usize,
    pub ef_construction: usize,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorStats {
    pub active_count: usize,
    pub orphan_count: usize,
    pub total_indexed: usize,
}

fn stores() -> &'static Mutex<VectorStores> {
    static STORES: OnceLock<Mutex<VectorStores>> = OnceLock::new();
    STORES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn orphan_counts() -> &'static Mutex<HashMap<usize, usize>> {
    static ORPHANS: OnceLock<Mutex<HashMap<usize, usize>>> = OnceLock::new();
    ORPHANS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Clone)]
pub struct VectorStorage {
    namespace: usize,
    config: VectorConfig,
}

impl VectorStorage {
    pub fn new(db: Arc<Database>, config: VectorConfig) -> Result<Self> {
        Ok(Self {
            namespace: namespace_for_db(&db),
            config,
        })
    }

    fn validate_dimension(&self, vector: &[f32]) -> Result<()> {
        if vector.len() != self.config.dimension {
            anyhow::bail!(
                "Vector dimension mismatch: expected {}, got {}",
                self.config.dimension,
                vector.len()
            );
        }
        Ok(())
    }

    fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
        let mut dot = 0.0_f32;
        let mut norm_a = 0.0_f32;
        let mut norm_b = 0.0_f32;
        for (left, right) in a.iter().zip(b) {
            dot += left * right;
            norm_a += left * left;
            norm_b += right * right;
        }
        if norm_a == 0.0 || norm_b == 0.0 {
            return 1.0;
        }
        1.0 - dot / (norm_a.sqrt() * norm_b.sqrt())
    }

    pub fn add(&self, chunk_id: &str, vector: &[f32]) -> Result<()> {
        self.validate_dimension(vector)?;
        let mut stores = stores().lock().expect("vector store lock poisoned");
        stores
            .entry(self.namespace)
            .or_default()
            .insert(chunk_id.to_string(), vector.to_vec());
        Ok(())
    }

    pub fn update(&self, chunk_id: &str, vector: &[f32]) -> Result<()> {
        self.add(chunk_id, vector)
    }

    pub fn delete(&self, chunk_id: &str) -> Result<bool> {
        let mut stores = stores().lock().expect("vector store lock poisoned");
        let deleted = stores
            .get_mut(&self.namespace)
            .is_some_and(|store| store.remove(chunk_id).is_some());
        if deleted {
            let mut orphans = orphan_counts()
                .lock()
                .expect("vector orphan store lock poisoned");
            *orphans.entry(self.namespace).or_default() += 1;
        }
        Ok(deleted)
    }

    pub fn search(&self, query: &[f32], k: usize) -> Result<Vec<(String, f32)>> {
        self.validate_dimension(query)?;
        let stores = stores().lock().expect("vector store lock poisoned");
        let Some(store) = stores.get(&self.namespace) else {
            return Ok(Vec::new());
        };
        let mut rows = store
            .iter()
            .map(|(chunk_id, vector)| {
                (
                    chunk_id.clone(),
                    Self::cosine_distance(query, vector).clamp(0.0, 2.0),
                )
            })
            .collect::<Vec<_>>();
        rows.sort_by(|left, right| left.1.partial_cmp(&right.1).unwrap_or(Ordering::Equal));
        rows.truncate(k);
        Ok(rows)
    }

    pub fn search_filtered(
        &self,
        query: &[f32],
        k: usize,
        _ef_search: usize,
        allowed_chunk_ids: &[String],
    ) -> Result<Vec<(String, f32)>> {
        self.validate_dimension(query)?;
        if allowed_chunk_ids.is_empty() {
            return Ok(Vec::new());
        }
        let stores = stores().lock().expect("vector store lock poisoned");
        let Some(store) = stores.get(&self.namespace) else {
            return Ok(Vec::new());
        };
        let allowed = allowed_chunk_ids.iter().collect::<HashSet<_>>();
        let mut rows = store
            .iter()
            .filter(|(chunk_id, _)| allowed.contains(chunk_id))
            .map(|(chunk_id, vector)| {
                (
                    chunk_id.clone(),
                    Self::cosine_distance(query, vector).clamp(0.0, 2.0),
                )
            })
            .collect::<Vec<_>>();
        rows.sort_by(|left, right| left.1.partial_cmp(&right.1).unwrap_or(Ordering::Equal));
        rows.truncate(k);
        Ok(rows)
    }

    pub fn has_vector(&self, chunk_id: &str) -> bool {
        stores()
            .lock()
            .expect("vector store lock poisoned")
            .get(&self.namespace)
            .is_some_and(|store| store.contains_key(chunk_id))
    }

    pub fn count(&self) -> usize {
        stores()
            .lock()
            .expect("vector store lock poisoned")
            .get(&self.namespace)
            .map(HashMap::len)
            .unwrap_or_default()
    }

    pub fn orphan_count(&self) -> usize {
        orphan_counts()
            .lock()
            .expect("vector orphan store lock poisoned")
            .get(&self.namespace)
            .copied()
            .unwrap_or_default()
    }

    pub fn stats(&self) -> VectorStats {
        let active_count = self.count();
        let orphan_count = self.orphan_count();
        VectorStats {
            active_count,
            orphan_count,
            total_indexed: active_count + orphan_count,
        }
    }

    pub fn cleanup_orphans(&self) -> Result<usize> {
        let mut orphans = orphan_counts()
            .lock()
            .expect("vector orphan store lock poisoned");
        Ok(orphans.remove(&self.namespace).unwrap_or_default())
    }
}
