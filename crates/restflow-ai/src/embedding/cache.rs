use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::RwLock;

/// In-memory cache for embeddings to avoid redundant API calls
pub struct EmbeddingCache {
    cache: RwLock<HashMap<String, Vec<f32>>>,
    max_entries: usize,
}

impl EmbeddingCache {
    pub fn new(max_entries: usize) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            max_entries,
        }
    }

    fn cache_key(text: &str, model: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(model.as_bytes());
        hasher.update(b":");
        hasher.update(text.as_bytes());
        hex::encode(hasher.finalize())
    }

    pub fn get(&self, text: &str, model: &str) -> Option<Vec<f32>> {
        let key = Self::cache_key(text, model);
        self.cache.read().ok()?.get(&key).cloned()
    }

    pub fn put(&self, text: &str, model: &str, embedding: Vec<f32>) {
        let key = Self::cache_key(text, model);
        if let Ok(mut cache) = self.cache.write() {
            if cache.len() >= self.max_entries {
                let keys_to_remove: Vec<_> = cache.keys().take(self.max_entries / 2).cloned().collect();
                for k in keys_to_remove {
                    cache.remove(&k);
                }
            }
            cache.insert(key, embedding);
        }
    }
}
