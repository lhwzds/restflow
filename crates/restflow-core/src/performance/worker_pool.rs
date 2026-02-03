use crate::models::AgentTask;
use crate::performance::TaskQueue;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

/// Worker pool configuration.
#[derive(Clone, Debug)]
pub struct WorkerPoolConfig {
    /// Number of workers.
    pub worker_count: usize,
    /// Idle sleep interval.
    pub idle_sleep: Duration,
}

impl Default for WorkerPoolConfig {
    fn default() -> Self {
        Self {
            worker_count: num_cpus::get() * 2,
            idle_sleep: Duration::from_millis(10),
        }
    }
}

#[async_trait]
pub trait TaskExecutor: Send + Sync + 'static {
    async fn execute(&self, task: &AgentTask) -> anyhow::Result<bool>;
}

/// Worker pool.
pub struct WorkerPool {
    queue: Arc<TaskQueue>,
    executor: Arc<dyn TaskExecutor>,
    config: WorkerPoolConfig,
    shutdown_tx: broadcast::Sender<()>,
    handles: Vec<tokio::task::JoinHandle<()>>,
}

impl WorkerPool {
    pub fn new(queue: Arc<TaskQueue>, executor: Arc<dyn TaskExecutor>, config: WorkerPoolConfig) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        Self {
            queue,
            executor,
            config,
            shutdown_tx,
            handles: Vec::new(),
        }
    }

    /// Start all workers.
    pub fn start(&mut self) {
        info!(count = self.config.worker_count, "Starting worker pool");
        for worker_id in 0..self.config.worker_count {
            let queue = self.queue.clone();
            let executor = self.executor.clone();
            let config = self.config.clone();
            let mut shutdown_rx = self.shutdown_tx.subscribe();
            let handle = tokio::spawn(async move {
                Self::worker_loop(worker_id, queue, executor, config, &mut shutdown_rx).await;
            });
            self.handles.push(handle);
        }
    }

    /// Stop all workers.
    pub async fn stop(&mut self) {
        info!("Stopping worker pool");
        let _ = self.shutdown_tx.send(());
        for handle in self.handles.drain(..) {
            let _ = handle.await;
        }
    }

    async fn worker_loop(
        worker_id: usize,
        queue: Arc<TaskQueue>,
        executor: Arc<dyn TaskExecutor>,
        config: WorkerPoolConfig,
        shutdown_rx: &mut broadcast::Receiver<()>,
    ) {
        info!(worker_id, "Worker started");
        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    info!(worker_id, "Worker shutting down");
                    break;
                }
                _ = Self::process_one(worker_id, &queue, &executor, &config) => {}
            }
        }
    }

    async fn process_one(
        worker_id: usize,
        queue: &TaskQueue,
        executor: &Arc<dyn TaskExecutor>,
        config: &WorkerPoolConfig,
    ) {
        let queued_task = match queue.pop() {
            Some(task) => task,
            None => {
                tokio::time::sleep(config.idle_sleep).await;
                return;
            }
        };

        let _permit = queue.acquire_permit().await;

        let task_id = queued_task.task.id.clone();
        let wait_time = queued_task.submitted_at.elapsed();

        queue.mark_running(&task_id, worker_id, wait_time);

        let result = executor.execute(&queued_task.task).await;

        match result {
            Ok(true) => {
                queue.mark_completed(&task_id, true);
                info!(worker_id, task_id = %task_id, wait_ms = ?wait_time.as_millis(), "Task completed successfully");
            }
            Ok(false) => {
                queue.mark_completed(&task_id, false);
                warn!(worker_id, task_id = %task_id, "Task completed with failure");
            }
            Err(e) => {
                queue.mark_completed(&task_id, false);
                error!(worker_id, task_id = %task_id, error = %e, "Task failed");
            }
        }
    }
}
