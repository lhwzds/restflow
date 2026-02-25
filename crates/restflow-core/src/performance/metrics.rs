use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Performance metrics collector.
#[derive(Default)]
pub struct Metrics {
    pub db_read_count: AtomicU64,
    pub db_write_count: AtomicU64,
    pub db_read_time_us: AtomicU64,
    pub db_write_time_us: AtomicU64,
    pub cache_hits: AtomicU64,
    pub cache_misses: AtomicU64,
    pub queue_submitted: AtomicU64,
    pub queue_completed: AtomicU64,
    pub queue_failed: AtomicU64,
    pub ipc_requests: AtomicU64,
    pub ipc_errors: AtomicU64,
}

impl Metrics {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn record_db_read(&self, duration: std::time::Duration) {
        self.db_read_count.fetch_add(1, Ordering::Relaxed);
        self.db_read_time_us
            .fetch_add(duration.as_micros() as u64, Ordering::Relaxed);
    }

    pub fn record_db_write(&self, duration: std::time::Duration) {
        self.db_write_count.fetch_add(1, Ordering::Relaxed);
        self.db_write_time_us
            .fetch_add(duration.as_micros() as u64, Ordering::Relaxed);
    }

    pub fn record_cache_hit(&self) {
        self.cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_cache_miss(&self) {
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            db_read_count: self.db_read_count.load(Ordering::Relaxed),
            db_write_count: self.db_write_count.load(Ordering::Relaxed),
            db_avg_read_us: self.avg_read_time(),
            db_avg_write_us: self.avg_write_time(),
            cache_hit_rate: self.cache_hit_rate(),
            queue_submitted: self.queue_submitted.load(Ordering::Relaxed),
            queue_completed: self.queue_completed.load(Ordering::Relaxed),
            queue_failed: self.queue_failed.load(Ordering::Relaxed),
        }
    }

    fn avg_read_time(&self) -> u64 {
        let count = self.db_read_count.load(Ordering::Relaxed);
        let time = self.db_read_time_us.load(Ordering::Relaxed);
        if count > 0 { time / count } else { 0 }
    }

    fn avg_write_time(&self) -> u64 {
        let count = self.db_write_count.load(Ordering::Relaxed);
        let time = self.db_write_time_us.load(Ordering::Relaxed);
        if count > 0 { time / count } else { 0 }
    }

    fn cache_hit_rate(&self) -> f64 {
        let hits = self.cache_hits.load(Ordering::Relaxed) as f64;
        let misses = self.cache_misses.load(Ordering::Relaxed) as f64;
        let total = hits + misses;
        if total > 0.0 { hits / total } else { 0.0 }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MetricsSnapshot {
    pub db_read_count: u64,
    pub db_write_count: u64,
    pub db_avg_read_us: u64,
    pub db_avg_write_us: u64,
    pub cache_hit_rate: f64,
    pub queue_submitted: u64,
    pub queue_completed: u64,
    pub queue_failed: u64,
}
