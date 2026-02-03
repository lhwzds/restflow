use crate::models::AgentTask;
use anyhow::Result;
use async_trait::async_trait;
use crossbeam_queue::SegQueue;
use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
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
    pub task: AgentTask,
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
    async fn save_task(&self, task: &AgentTask) -> Result<()>;
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
    pub async fn submit(&self, task: AgentTask, priority: TaskPriority) -> Result<(), QueueError> {
        let pending = self.stats.pending_count.load(Ordering::Relaxed);
        if pending >= self.config.max_queue_size {
            return Err(QueueError::QueueFull);
        }

        let queued = QueuedTask {
            task: task.clone(),
            submitted_at: Instant::now(),
            priority,
        };

        if self.config.persist_tasks {
            if let Some(storage) = &self.storage {
                let storage = storage.clone();
                let task = task.clone();
                tokio::spawn(async move {
                    let _ = storage.save_task(&task).await;
                });
            }
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
        if count > 0 {
            total / count
        } else {
            0
        }
    }

    fn calculate_avg_wait_time(&self) -> u64 {
        let total = self.stats.total_wait_time_ms.load(Ordering::Relaxed);
        let count = self.stats.completed_count.load(Ordering::Relaxed)
            + self.stats.failed_count.load(Ordering::Relaxed);
        if count > 0 {
            total / count
        } else {
            0
        }
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
