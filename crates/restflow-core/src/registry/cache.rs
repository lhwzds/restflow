use std::time::{Duration, Instant};

pub(crate) struct CacheEntry<T> {
    pub(crate) data: T,
    expires_at: Instant,
}

impl<T> CacheEntry<T> {
    pub(crate) fn new(data: T, ttl: Duration) -> Self {
        Self {
            data,
            expires_at: Instant::now() + ttl,
        }
    }

    pub(crate) fn is_expired(&self) -> bool {
        Instant::now() > self.expires_at
    }
}
