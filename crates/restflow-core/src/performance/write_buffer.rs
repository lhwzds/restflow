use anyhow::Result;
use parking_lot::Mutex;
use redb::{Database, WriteTransaction};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Notify;
use tracing::{debug, error};

/// Trait for buffered write operations that can be applied in a batch.
pub trait WriteOperation: Send + 'static {
    fn apply(self: Box<Self>, txn: &WriteTransaction) -> Result<()>;
}

/// Wrapper for a closure-based write operation.
pub struct WriteOperationFn<F>(pub F);

impl<F> WriteOperation for WriteOperationFn<F>
where
    F: FnOnce(&WriteTransaction) -> Result<()> + Send + 'static,
{
    fn apply(self: Box<Self>, txn: &WriteTransaction) -> Result<()> {
        (self.0)(txn)
    }
}

/// Write buffer configuration.
#[derive(Clone, Debug)]
pub struct WriteBufferConfig {
    /// Maximum buffered operations before auto flush.
    pub max_entries: usize,
    /// Maximum delay before auto flush.
    pub max_delay: Duration,
    /// Whether the buffer is enabled.
    pub enabled: bool,
}

impl Default for WriteBufferConfig {
    fn default() -> Self {
        Self {
            max_entries: 100,
            max_delay: Duration::from_millis(50),
            enabled: true,
        }
    }
}

/// Batch write buffer for redb transactions.
pub struct WriteBuffer {
    db: Arc<Database>,
    config: WriteBufferConfig,
    buffer: Mutex<Vec<Box<dyn WriteOperation>>>,
    last_flush: Mutex<Instant>,
    flush_notify: Notify,
}

impl WriteBuffer {
    pub fn new(db: Arc<Database>, config: WriteBufferConfig) -> Arc<Self> {
        let buffer = Arc::new(Self {
            db,
            config: config.clone(),
            buffer: Mutex::new(Vec::with_capacity(config.max_entries)),
            last_flush: Mutex::new(Instant::now()),
            flush_notify: Notify::new(),
        });

        if config.enabled {
            let buffer_clone = buffer.clone();
            tokio::spawn(async move {
                buffer_clone.flush_loop().await;
            });
        }

        buffer
    }

    /// Buffer a write operation (non-blocking).
    pub fn write(&self, op: Box<dyn WriteOperation>) {
        if !self.config.enabled {
            self.write_immediate(op);
            return;
        }

        let should_flush = {
            let mut buffer = self.buffer.lock();
            buffer.push(op);
            buffer.len() >= self.config.max_entries
        };

        if should_flush {
            self.flush_notify.notify_one();
        }
    }

    /// Buffer a write operation and wait for flush.
    pub async fn write_sync(&self, op: Box<dyn WriteOperation>) {
        self.write(op);
        self.flush().await;
    }

    /// Force flush buffered operations.
    pub async fn flush(&self) {
        let ops = {
            let mut buffer = self.buffer.lock();
            if buffer.is_empty() {
                return;
            }
            std::mem::take(&mut *buffer)
        };

        if let Err(e) = self.flush_batch(ops).await {
            error!(error = %e, "Failed to flush write buffer");
        }

        *self.last_flush.lock() = Instant::now();
    }

    async fn flush_batch(&self, ops: Vec<Box<dyn WriteOperation>>) -> Result<()> {
        let count = ops.len();
        let start = Instant::now();
        let db = self.db.clone();

        tokio::task::spawn_blocking(move || {
            let txn = db.begin_write()?;
            for op in ops {
                op.apply(&txn)?;
            }
            txn.commit()?;
            Result::Ok(())
        })
        .await??;

        debug!(count, elapsed_ms = ?start.elapsed().as_millis(), "Flushed write buffer");
        Ok(())
    }

    fn write_immediate(&self, op: Box<dyn WriteOperation>) {
        let db = self.db.clone();
        let _ = std::thread::spawn(move || {
            if let Ok(txn) = db.begin_write() {
                let _ = op.apply(&txn);
                let _ = txn.commit();
            }
        });
    }

    async fn flush_loop(&self) {
        loop {
            tokio::select! {
                _ = self.flush_notify.notified() => {
                    self.flush().await;
                }
                _ = tokio::time::sleep(self.config.max_delay) => {
                    let elapsed = self.last_flush.lock().elapsed();
                    if elapsed >= self.config.max_delay {
                        let has_data = !self.buffer.lock().is_empty();
                        if has_data {
                            self.flush().await;
                        }
                    }
                }
            }
        }
    }
}
