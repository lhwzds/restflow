use dashmap::DashMap;
use std::hash::Hash;
use std::sync::Arc;
use std::time::{Duration, Instant};

struct CacheEntry<V> {
    value: V,
    created_at: Instant,
    access_count: u64,
}

/// Cache configuration.
#[derive(Clone, Debug)]
pub struct CacheConfig {
    /// Maximum entries.
    pub max_entries: usize,
    /// Entry TTL.
    pub ttl: Duration,
    /// Whether the cache is enabled.
    pub enabled: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 10_000,
            ttl: Duration::from_secs(60),
            enabled: true,
        }
    }
}

/// Basic cache with TTL and eviction.
pub struct Cache<K, V> {
    data: DashMap<K, CacheEntry<V>>,
    config: CacheConfig,
}

impl<K, V> Cache<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    pub fn new(config: CacheConfig) -> Arc<Self> {
        let cache = Arc::new(Self {
            data: DashMap::new(),
            config: config.clone(),
        });

        if config.enabled {
            let cache_clone = cache.clone();
            tokio::spawn(async move {
                cache_clone.cleanup_loop().await;
            });
        }

        cache
    }

    pub fn get(&self, key: &K) -> Option<V> {
        if !self.config.enabled {
            return None;
        }
        self.data.get_mut(key).and_then(|mut entry| {
            if entry.created_at.elapsed() > self.config.ttl {
                None
            } else {
                entry.access_count += 1;
                Some(entry.value.clone())
            }
        })
    }

    pub fn set(&self, key: K, value: V) {
        if !self.config.enabled {
            return;
        }

        if self.data.len() >= self.config.max_entries {
            self.evict_one();
        }

        self.data.insert(
            key,
            CacheEntry {
                value,
                created_at: Instant::now(),
                access_count: 0,
            },
        );
    }

    pub fn remove(&self, key: &K) {
        self.data.remove(key);
    }

    pub fn clear(&self) {
        self.data.clear();
    }

    fn evict_one(&self) {
        let mut min_key: Option<K> = None;
        let mut min_count = u64::MAX;
        for entry in self.data.iter() {
            if entry.access_count < min_count {
                min_count = entry.access_count;
                min_key = Some(entry.key().clone());
            }
        }

        if let Some(key) = min_key {
            self.data.remove(&key);
        }
    }

    async fn cleanup_loop(&self) {
        let interval = self.config.ttl / 2;
        loop {
            tokio::time::sleep(interval).await;
            self.cleanup_expired();
        }
    }

    fn cleanup_expired(&self) {
        self.data
            .retain(|_, entry| entry.created_at.elapsed() <= self.config.ttl);
    }
}

/// Cached wrapper for storage-like operations.
pub struct CachedStorage {
    pub agents: Arc<Cache<String, Vec<u8>>>,
    pub skills: Arc<Cache<String, Vec<u8>>>,
    pub tasks: Arc<Cache<String, Vec<u8>>>,
}

impl CachedStorage {
    pub fn new(config: CacheConfig) -> Self {
        Self {
            agents: Cache::new(config.clone()),
            skills: Cache::new(config.clone()),
            tasks: Cache::new(config),
        }
    }
}
