use crate::models::BackgroundAgent;
use anyhow::Result;
use async_trait::async_trait;
use crossbeam_queue::SegQueue;
use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

/// Queue priority levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// Task queued for execution.
#[derive(Debug, Clone)]
pub struct QueuedTask {
    pub task: BackgroundAgent,
    pub submitted_at: Instant,
    pub priority: TaskPriority,
}

/// Running task details.
pub struct RunningTaskInfo {
    pub task_id: String,
    pub started_at: Instant,
    pub worker_id: usize,
}

/// Queue statistics counters.
#[derive(Debug, Default)]
pub struct QueueStats {
    pub pending_count: AtomicUsize,
    pub running_count: AtomicUsize,
    pub completed_count: AtomicU64,
    pub failed_count: AtomicU64,
    pub total_wait_time_ms: AtomicU64,
    pub total_exec_time_ms: AtomicU64,
}

/// Storage interface for persisting queued tasks.
#[async_trait]
pub trait TaskQueueStorage: Send + Sync {
    async fn save_task(&self, task: &BackgroundAgent) -> Result<()>;
}

/// Queue configuration.
#[derive(Clone, Debug)]
pub struct TaskQueueConfig {
    /// Maximum concurrent tasks.
    pub max_concurrent: usize,
    /// Maximum queued tasks.
    pub max_queue_size: usize,
    /// Whether to persist tasks.
    pub persist_tasks: bool,
}

impl Default for TaskQueueConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 50,
            max_queue_size: 10_000,
            persist_tasks: true,
        }
    }
}

/// High performance task queue.
pub struct TaskQueue {
    critical: SegQueue<QueuedTask>,
    high: SegQueue<QueuedTask>,
    normal: SegQueue<QueuedTask>,
    low: SegQueue<QueuedTask>,
    running: DashMap<String, RunningTaskInfo>,
    semaphore: Arc<Semaphore>,
    config: TaskQueueConfig,
    stats: Arc<QueueStats>,
    storage: Option<Arc<dyn TaskQueueStorage>>,
}

impl TaskQueue {
    pub fn new(config: TaskQueueConfig, storage: Option<Arc<dyn TaskQueueStorage>>) -> Self {
        Self {
            critical: SegQueue::new(),
            high: SegQueue::new(),
            normal: SegQueue::new(),
            low: SegQueue::new(),
            running: DashMap::new(),
            semaphore: Arc::new(Semaphore::new(config.max_concurrent)),
            config,
            stats: Arc::new(QueueStats::default()),
            storage,
        }
    }

    /// Submit a task to the queue.
    pub async fn submit(
        &self,
        task: BackgroundAgent,
        priority: TaskPriority,
    ) -> Result<(), QueueError> {
        let pending = self.stats.pending_count.load(Ordering::Relaxed);
        if pending >= self.config.max_queue_size {
            return Err(QueueError::QueueFull);
        }

        let queued = QueuedTask {
            task: task.clone(),
            submitted_at: Instant::now(),
            priority,
        };

        if self.config.persist_tasks
            && let Some(storage) = &self.storage
        {
            let storage = storage.clone();
            let task = task.clone();
            tokio::spawn(async move {
                let _ = storage.save_task(&task).await;
            });
        }

        match priority {
            TaskPriority::Critical => self.critical.push(queued),
            TaskPriority::High => self.high.push(queued),
            TaskPriority::Normal => self.normal.push(queued),
            TaskPriority::Low => self.low.push(queued),
        }

        self.stats.pending_count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Pop a task from the queue by priority.
    pub fn pop(&self) -> Option<QueuedTask> {
        self.critical
            .pop()
            .or_else(|| self.high.pop())
            .or_else(|| self.normal.pop())
            .or_else(|| self.low.pop())
    }

    /// Acquire a concurrency permit.
    pub async fn acquire_permit(&self) -> tokio::sync::SemaphorePermit<'_> {
        self.semaphore.acquire().await.expect("semaphore closed")
    }

    /// Mark a task as running.
    pub fn mark_running(&self, task_id: &str, worker_id: usize, wait_time: Duration) {
        self.running.insert(
            task_id.to_string(),
            RunningTaskInfo {
                task_id: task_id.to_string(),
                started_at: Instant::now(),
                worker_id,
            },
        );
        self.stats.pending_count.fetch_sub(1, Ordering::Relaxed);
        self.stats.running_count.fetch_add(1, Ordering::Relaxed);
        self.stats
            .total_wait_time_ms
            .fetch_add(wait_time.as_millis() as u64, Ordering::Relaxed);
    }

    /// Mark a task as completed.
    pub fn mark_completed(&self, task_id: &str, success: bool) {
        if let Some((_, info)) = self.running.remove(task_id) {
            let exec_time = info.started_at.elapsed().as_millis() as u64;
            self.stats
                .total_exec_time_ms
                .fetch_add(exec_time, Ordering::Relaxed);
            self.stats.running_count.fetch_sub(1, Ordering::Relaxed);
            if success {
                self.stats.completed_count.fetch_add(1, Ordering::Relaxed);
            } else {
                self.stats.failed_count.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Snapshot queue stats.
    pub fn get_stats(&self) -> QueueStatsSnapshot {
        QueueStatsSnapshot {
            pending: self.stats.pending_count.load(Ordering::Relaxed),
            running: self.stats.running_count.load(Ordering::Relaxed),
            completed: self.stats.completed_count.load(Ordering::Relaxed),
            failed: self.stats.failed_count.load(Ordering::Relaxed),
            avg_exec_time_ms: self.calculate_avg_exec_time(),
            avg_wait_time_ms: self.calculate_avg_wait_time(),
        }
    }

    fn calculate_avg_exec_time(&self) -> u64 {
        let total = self.stats.total_exec_time_ms.load(Ordering::Relaxed);
        let count = self.stats.completed_count.load(Ordering::Relaxed)
            + self.stats.failed_count.load(Ordering::Relaxed);
        if count > 0 { total / count } else { 0 }
    }

    fn calculate_avg_wait_time(&self) -> u64 {
        let total = self.stats.total_wait_time_ms.load(Ordering::Relaxed);
        let count = self.stats.completed_count.load(Ordering::Relaxed)
            + self.stats.failed_count.load(Ordering::Relaxed);
        if count > 0 { total / count } else { 0 }
    }
}

#[derive(Debug)]
pub enum QueueError {
    QueueFull,
    TaskNotFound,
    AlreadyRunning,
}

#[derive(Debug, Clone)]
pub struct QueueStatsSnapshot {
    pub pending: usize,
    pub running: usize,
    pub completed: u64,
    pub failed: u64,
    pub avg_exec_time_ms: u64,
    pub avg_wait_time_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::background_agent::TaskSchedule;

    /// Helper to create a test queue with no persistence.
    fn test_queue(max_queue_size: usize) -> TaskQueue {
        let config = TaskQueueConfig {
            max_concurrent: 10,
            max_queue_size,
            persist_tasks: false,
        };
        TaskQueue::new(config, None)
    }

    /// Helper to create a minimal BackgroundAgent for testing.
    fn test_agent(id: &str) -> BackgroundAgent {
        BackgroundAgent::new(
            id.to_string(),
            format!("task-{id}"),
            "agent-1".to_string(),
            TaskSchedule::default(),
        )
    }

    #[tokio::test]
    async fn submit_and_pop() {
        let queue = test_queue(100);
        let agent = test_agent("t1");
        queue
            .submit(agent.clone(), TaskPriority::Normal)
            .await
            .unwrap();

        let popped = queue.pop();
        assert!(popped.is_some(), "should pop the submitted task");
        let popped = popped.unwrap();
        assert_eq!(popped.task.id, "t1");
        assert_eq!(popped.priority, TaskPriority::Normal);
    }

    #[tokio::test]
    async fn priority_ordering() {
        let queue = test_queue(100);

        // Submit in non-priority order
        queue
            .submit(test_agent("low"), TaskPriority::Low)
            .await
            .unwrap();
        queue
            .submit(test_agent("normal"), TaskPriority::Normal)
            .await
            .unwrap();
        queue
            .submit(test_agent("high"), TaskPriority::High)
            .await
            .unwrap();
        queue
            .submit(test_agent("critical"), TaskPriority::Critical)
            .await
            .unwrap();

        // Pop should return in priority order: Critical > High > Normal > Low
        assert_eq!(queue.pop().unwrap().task.id, "critical");
        assert_eq!(queue.pop().unwrap().task.id, "high");
        assert_eq!(queue.pop().unwrap().task.id, "normal");
        assert_eq!(queue.pop().unwrap().task.id, "low");
        assert!(queue.pop().is_none(), "queue should be empty");
    }

    #[test]
    fn pop_empty_queue() {
        let queue = test_queue(100);
        assert!(
            queue.pop().is_none(),
            "pop on empty queue should return None"
        );
    }

    #[tokio::test]
    async fn mark_running_updates_stats() {
        let queue = test_queue(100);
        queue
            .submit(test_agent("t1"), TaskPriority::Normal)
            .await
            .unwrap();

        let stats_before = queue.get_stats();
        assert_eq!(stats_before.pending, 1);
        assert_eq!(stats_before.running, 0);

        let _popped = queue.pop().unwrap();
        queue.mark_running("t1", 0, Duration::from_millis(10));

        let stats_after = queue.get_stats();
        assert_eq!(stats_after.running, 1);
        // pending_count was decremented by mark_running
        assert_eq!(stats_after.pending, 0);
    }

    #[tokio::test]
    async fn mark_completed_updates_stats() {
        let queue = test_queue(100);
        queue
            .submit(test_agent("t1"), TaskPriority::Normal)
            .await
            .unwrap();
        let _popped = queue.pop().unwrap();
        queue.mark_running("t1", 0, Duration::from_millis(5));

        let stats_before = queue.get_stats();
        assert_eq!(stats_before.running, 1);
        assert_eq!(stats_before.completed, 0);

        queue.mark_completed("t1", true);

        let stats_after = queue.get_stats();
        assert_eq!(stats_after.running, 0);
        assert_eq!(stats_after.completed, 1);
        assert_eq!(stats_after.failed, 0);
    }

    #[tokio::test]
    async fn queue_full() {
        let queue = test_queue(2);
        queue
            .submit(test_agent("t1"), TaskPriority::Normal)
            .await
            .unwrap();
        queue
            .submit(test_agent("t2"), TaskPriority::Normal)
            .await
            .unwrap();

        let result = queue.submit(test_agent("t3"), TaskPriority::Normal).await;
        assert!(
            result.is_err(),
            "submitting beyond max_queue_size should fail"
        );
        match result.unwrap_err() {
            QueueError::QueueFull => {} // expected
            other => panic!("expected QueueFull, got: {:?}", other),
        }
    }

    #[test]
    fn fresh_queue_stats_all_zero() {
        let queue = test_queue(100);
        let stats = queue.get_stats();
        assert_eq!(stats.pending, 0);
        assert_eq!(stats.running, 0);
        assert_eq!(stats.completed, 0);
        assert_eq!(stats.failed, 0);
        assert_eq!(stats.avg_exec_time_ms, 0);
        assert_eq!(stats.avg_wait_time_ms, 0);
    }
}
