use crate::models::BackgroundAgent;
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
    async fn execute(&self, task: &BackgroundAgent) -> anyhow::Result<bool>;
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
    pub fn new(
        queue: Arc<TaskQueue>,
        executor: Arc<dyn TaskExecutor>,
        config: WorkerPoolConfig,
    ) -> Self {
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
        const WORKER_STOP_TIMEOUT: Duration = Duration::from_secs(10);

        info!("Stopping worker pool");
        let _ = self.shutdown_tx.send(());
        let handles: Vec<_> = self.handles.drain(..).collect();
        for (i, mut handle) in handles.into_iter().enumerate() {
            if tokio::time::timeout(WORKER_STOP_TIMEOUT, &mut handle)
                .await
                .is_err()
            {
                warn!(
                    worker_id = i,
                    "Worker did not stop within {:?}, aborting", WORKER_STOP_TIMEOUT
                );
                handle.abort();
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::BackgroundAgent;
    use crate::performance::{TaskPriority, TaskQueue, TaskQueueConfig};
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Mock executor that tracks how many times it was called and optionally
    /// returns an error for every invocation.
    struct MockExecutor {
        call_count: Arc<AtomicU32>,
        should_fail: bool,
    }

    impl MockExecutor {
        fn new() -> (Self, Arc<AtomicU32>) {
            let counter = Arc::new(AtomicU32::new(0));
            (
                Self {
                    call_count: counter.clone(),
                    should_fail: false,
                },
                counter,
            )
        }

        fn failing() -> (Self, Arc<AtomicU32>) {
            let counter = Arc::new(AtomicU32::new(0));
            (
                Self {
                    call_count: counter.clone(),
                    should_fail: true,
                },
                counter,
            )
        }
    }

    #[async_trait]
    impl TaskExecutor for MockExecutor {
        async fn execute(&self, _task: &BackgroundAgent) -> anyhow::Result<bool> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            if self.should_fail {
                Err(anyhow::anyhow!("mock executor error"))
            } else {
                Ok(true)
            }
        }
    }

    /// Helper to build a minimal BackgroundAgent for testing.
    fn make_task(id: &str) -> BackgroundAgent {
        use crate::models::TaskSchedule;
        BackgroundAgent::new(
            id.to_string(),
            format!("task-{}", id),
            "agent-1".to_string(),
            TaskSchedule::default(),
        )
    }

    /// Helper to build a simple TaskQueue with no persistence.
    fn make_queue() -> Arc<TaskQueue> {
        Arc::new(TaskQueue::new(
            TaskQueueConfig {
                max_concurrent: 10,
                max_queue_size: 100,
                persist_tasks: false,
            },
            None,
        ))
    }

    #[tokio::test]
    async fn start_submit_execute_stop() {
        let queue = make_queue();
        let (executor, call_count) = MockExecutor::new();

        let mut pool = WorkerPool::new(
            queue.clone(),
            Arc::new(executor),
            WorkerPoolConfig {
                worker_count: 2,
                idle_sleep: Duration::from_millis(5),
            },
        );

        pool.start();

        // Submit a task to the queue.
        queue
            .submit(make_task("t1"), TaskPriority::Normal)
            .await
            .expect("submit should succeed");

        // Wait until the executor has been called at least once.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while call_count.load(Ordering::SeqCst) == 0 {
            if tokio::time::Instant::now() > deadline {
                panic!("executor was never called within timeout");
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        assert!(call_count.load(Ordering::SeqCst) >= 1);

        // Stop should complete without hanging.
        pool.stop().await;
    }

    #[tokio::test]
    async fn stop_empty_queue_no_deadlock() {
        let queue = make_queue();
        let (executor, _call_count) = MockExecutor::new();

        let mut pool = WorkerPool::new(
            queue,
            Arc::new(executor),
            WorkerPoolConfig {
                worker_count: 2,
                idle_sleep: Duration::from_millis(5),
            },
        );

        pool.start();

        // No tasks submitted. Stop should return promptly.
        let result = tokio::time::timeout(Duration::from_secs(5), pool.stop()).await;
        assert!(result.is_ok(), "stop() should not deadlock on empty queue");
    }

    #[tokio::test]
    async fn executor_error_increments_failed() {
        let queue = make_queue();
        let (executor, call_count) = MockExecutor::failing();

        let mut pool = WorkerPool::new(
            queue.clone(),
            Arc::new(executor),
            WorkerPoolConfig {
                worker_count: 1,
                idle_sleep: Duration::from_millis(5),
            },
        );

        pool.start();

        // Submit a task that the failing executor will process.
        queue
            .submit(make_task("t-fail"), TaskPriority::Normal)
            .await
            .expect("submit should succeed");

        // Wait until the executor has been called.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while call_count.load(Ordering::SeqCst) == 0 {
            if tokio::time::Instant::now() > deadline {
                panic!("executor was never called within timeout");
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // Allow a brief moment for mark_completed to propagate.
        tokio::time::sleep(Duration::from_millis(50)).await;

        let stats = queue.get_stats();
        assert!(
            stats.failed >= 1,
            "failed count should be >= 1, got {}",
            stats.failed
        );

        pool.stop().await;
    }
}
