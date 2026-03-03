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
    pub started_count: AtomicU64,
    pub completed_count: AtomicU64,
    pub failed_count: AtomicU64,
    pub total_wait_time_ms: AtomicU64,
    pub total_exec_time_ms: AtomicU64,
    pub snapshot_version: AtomicU64,
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
    fair_cursor: AtomicUsize,
}

impl TaskQueue {
    // Weighted schedule for minimal fairness while preserving priority bias.
    // Critical/High are favored, but Normal/Low are guaranteed service when queued.
    const FAIR_SCHEDULE: [TaskPriority; 8] = [
        TaskPriority::Critical,
        TaskPriority::High,
        TaskPriority::Critical,
        TaskPriority::Normal,
        TaskPriority::High,
        TaskPriority::Critical,
        TaskPriority::Low,
        TaskPriority::Normal,
    ];

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
            fair_cursor: AtomicUsize::new(0),
        }
    }

    /// Submit a task to the queue.
    pub async fn submit(
        &self,
        task: BackgroundAgent,
        priority: TaskPriority,
    ) -> Result<(), QueueError> {
        self.stats
            .pending_count
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |pending| {
                if pending >= self.config.max_queue_size {
                    None
                } else {
                    Some(pending + 1)
                }
            })
            .map_err(|_| QueueError::QueueFull)?;

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

        Ok(())
    }

    /// Pop a task from the queue by priority.
    pub fn pop(&self) -> Option<QueuedTask> {
        let start = self.fair_cursor.fetch_add(1, Ordering::Relaxed);
        for offset in 0..Self::FAIR_SCHEDULE.len() {
            let idx = (start + offset) % Self::FAIR_SCHEDULE.len();
            if let Some(task) = self.pop_by_priority(Self::FAIR_SCHEDULE[idx]) {
                return Some(task);
            }
        }
        None
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
        self.begin_stats_update();
        self.decrement_pending_safely();
        self.stats.running_count.fetch_add(1, Ordering::AcqRel);
        self.stats.started_count.fetch_add(1, Ordering::AcqRel);
        self.stats
            .total_wait_time_ms
            .fetch_add(wait_time.as_millis() as u64, Ordering::AcqRel);
        self.end_stats_update();
    }

    /// Mark a task as completed.
    pub fn mark_completed(&self, task_id: &str, success: bool) {
        if let Some((_, info)) = self.running.remove(task_id) {
            let exec_time = info.started_at.elapsed().as_millis() as u64;
            self.begin_stats_update();
            self.stats
                .total_exec_time_ms
                .fetch_add(exec_time, Ordering::AcqRel);
            self.decrement_running_safely();
            if success {
                self.stats.completed_count.fetch_add(1, Ordering::AcqRel);
            } else {
                self.stats.failed_count.fetch_add(1, Ordering::AcqRel);
            }
            self.end_stats_update();
        }
    }

    /// Snapshot queue stats.
    pub fn get_stats(&self) -> QueueStatsSnapshot {
        loop {
            let version_before = self.stats.snapshot_version.load(Ordering::Acquire);
            if version_before % 2 != 0 {
                std::hint::spin_loop();
                continue;
            }

            let pending = self.stats.pending_count.load(Ordering::Acquire);
            let running = self.stats.running_count.load(Ordering::Acquire);
            let started = self.stats.started_count.load(Ordering::Acquire);
            let completed = self.stats.completed_count.load(Ordering::Acquire);
            let failed = self.stats.failed_count.load(Ordering::Acquire);
            let total_wait_time_ms = self.stats.total_wait_time_ms.load(Ordering::Acquire);
            let total_exec_time_ms = self.stats.total_exec_time_ms.load(Ordering::Acquire);

            let version_after = self.stats.snapshot_version.load(Ordering::Acquire);
            if version_before != version_after {
                continue;
            }

            let finished = completed + failed;
            return QueueStatsSnapshot {
                pending,
                running,
                completed,
                failed,
                avg_exec_time_ms: Self::calculate_avg(total_exec_time_ms, finished),
                // Wait time is measured when entering running state, so divide by started tasks.
                avg_wait_time_ms: Self::calculate_avg(total_wait_time_ms, started),
            };
        }
    }

    fn calculate_avg(total_ms: u64, count: u64) -> u64 {
        if count > 0 { total_ms / count } else { 0 }
    }

    fn pop_by_priority(&self, priority: TaskPriority) -> Option<QueuedTask> {
        match priority {
            TaskPriority::Critical => self.critical.pop(),
            TaskPriority::High => self.high.pop(),
            TaskPriority::Normal => self.normal.pop(),
            TaskPriority::Low => self.low.pop(),
        }
    }

    fn begin_stats_update(&self) {
        self.stats.snapshot_version.fetch_add(1, Ordering::AcqRel);
    }

    fn end_stats_update(&self) {
        self.stats.snapshot_version.fetch_add(1, Ordering::Release);
    }

    fn decrement_pending_safely(&self) {
        let _ =
            self.stats
                .pending_count
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |pending| {
                    Some(pending.saturating_sub(1))
                });
    }

    fn decrement_running_safely(&self) {
        let _ =
            self.stats
                .running_count
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |running| {
                    Some(running.saturating_sub(1))
                });
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
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use tokio::sync::Barrier;

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

        let mut popped_ids = Vec::new();
        while let Some(task) = queue.pop() {
            popped_ids.push(task.task.id);
        }
        assert_eq!(popped_ids.len(), 4);
        assert_eq!(popped_ids[0], "critical");
        assert_eq!(popped_ids[1], "high");
        assert!(popped_ids.contains(&"normal".to_string()));
        assert!(popped_ids.contains(&"low".to_string()));
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

    #[tokio::test]
    async fn concurrent_submit_respects_capacity() {
        let max_queue_size = 5;
        let total_submitters = 50;
        let queue = Arc::new(test_queue(max_queue_size));
        let barrier = Arc::new(Barrier::new(total_submitters));

        let mut join_set = tokio::task::JoinSet::new();
        for i in 0..total_submitters {
            let queue = Arc::clone(&queue);
            let barrier = Arc::clone(&barrier);
            join_set.spawn(async move {
                let id = format!("t{i}");
                barrier.wait().await;
                queue.submit(test_agent(&id), TaskPriority::Normal).await
            });
        }

        let mut accepted = 0;
        let mut rejected = 0;
        while let Some(joined) = join_set.join_next().await {
            match joined.expect("submit task should not panic") {
                Ok(()) => accepted += 1,
                Err(QueueError::QueueFull) => rejected += 1,
                Err(other) => panic!("unexpected submit error: {:?}", other),
            }
        }

        assert_eq!(accepted, max_queue_size);
        assert_eq!(rejected, total_submitters - max_queue_size);
        assert_eq!(queue.get_stats().pending, max_queue_size);
    }

    #[tokio::test]
    async fn concurrent_submit_pop_complete_preserves_final_stats_invariants() {
        let total_tasks = 240usize;
        let producer_count = 8usize;
        let consumer_count = 6usize;
        let queue = Arc::new(test_queue(total_tasks + 32));

        let next_task_id = Arc::new(AtomicUsize::new(0));
        let submit_done = Arc::new(AtomicBool::new(false));
        let succeeded = Arc::new(AtomicUsize::new(0));
        let failed = Arc::new(AtomicUsize::new(0));

        let mut producer_set = tokio::task::JoinSet::new();
        for _ in 0..producer_count {
            let queue = Arc::clone(&queue);
            let next_task_id = Arc::clone(&next_task_id);
            producer_set.spawn(async move {
                loop {
                    let current = next_task_id.fetch_add(1, Ordering::AcqRel);
                    if current >= total_tasks {
                        break;
                    }
                    let task_id = format!("task-{current}");
                    queue
                        .submit(test_agent(&task_id), TaskPriority::Normal)
                        .await
                        .expect("queue should have enough capacity");
                }
            });
        }

        let mut consumer_set = tokio::task::JoinSet::new();
        for worker_id in 0..consumer_count {
            let queue = Arc::clone(&queue);
            let submit_done = Arc::clone(&submit_done);
            let succeeded = Arc::clone(&succeeded);
            let failed = Arc::clone(&failed);
            consumer_set.spawn(async move {
                loop {
                    if let Some(task) = queue.pop() {
                        let numeric_id: usize = task.task.id["task-".len()..]
                            .parse()
                            .expect("test task id format should be numeric");
                        queue.mark_running(&task.task.id, worker_id, task.submitted_at.elapsed());
                        // Deterministic success/failure split for invariant checks.
                        let success = numeric_id % 4 != 0;
                        queue.mark_completed(&task.task.id, success);
                        if success {
                            succeeded.fetch_add(1, Ordering::AcqRel);
                        } else {
                            failed.fetch_add(1, Ordering::AcqRel);
                        }
                    } else if submit_done.load(Ordering::Acquire) {
                        // Producers finished and queue appears drained.
                        break;
                    } else {
                        tokio::task::yield_now().await;
                    }
                }
            });
        }

        while let Some(joined) = producer_set.join_next().await {
            joined.expect("producer should not panic");
        }
        submit_done.store(true, Ordering::Release);

        while let Some(joined) = consumer_set.join_next().await {
            joined.expect("consumer should not panic");
        }

        let expected_success = succeeded.load(Ordering::Acquire) as u64;
        let expected_failed = failed.load(Ordering::Acquire) as u64;
        assert_eq!(
            expected_success + expected_failed,
            total_tasks as u64,
            "all submitted tasks should be completed or failed exactly once"
        );

        let stats = queue.get_stats();
        assert_eq!(stats.pending, 0, "pending should be drained");
        assert_eq!(stats.running, 0, "running should be drained");
        assert_eq!(stats.completed, expected_success);
        assert_eq!(stats.failed, expected_failed);
        assert_eq!(
            stats.completed + stats.failed,
            total_tasks as u64,
            "final accounting must match total submitted tasks"
        );
    }

    #[tokio::test]
    async fn capacity_near_limit_under_concurrent_pressure() {
        let max_queue_size = 32usize;
        let queue = Arc::new(test_queue(max_queue_size));

        for i in 0..(max_queue_size - 1) {
            queue
                .submit(test_agent(&format!("prefill-{i}")), TaskPriority::Normal)
                .await
                .expect("prefill should fit");
        }

        // First wave races for a single remaining slot.
        let first_wave_submitters = 40usize;
        let first_barrier = Arc::new(Barrier::new(first_wave_submitters));
        let mut first_wave = tokio::task::JoinSet::new();
        for i in 0..first_wave_submitters {
            let queue = Arc::clone(&queue);
            let barrier = Arc::clone(&first_barrier);
            first_wave.spawn(async move {
                barrier.wait().await;
                queue
                    .submit(test_agent(&format!("burst-a-{i}")), TaskPriority::Normal)
                    .await
            });
        }

        let mut accepted_a = 0usize;
        let mut rejected_a = 0usize;
        while let Some(joined) = first_wave.join_next().await {
            match joined.expect("first wave task should not panic") {
                Ok(()) => accepted_a += 1,
                Err(QueueError::QueueFull) => rejected_a += 1,
                Err(other) => panic!("unexpected submit error in first wave: {:?}", other),
            }
        }
        assert_eq!(accepted_a, 1, "only one slot should be available");
        assert_eq!(rejected_a, first_wave_submitters - 1);
        assert_eq!(
            queue.get_stats().pending,
            max_queue_size,
            "queue should be exactly full after first wave"
        );

        // Drain prefilled + first-wave accepted tasks.
        while let Some(task) = queue.pop() {
            queue.mark_running(&task.task.id, 0, task.submitted_at.elapsed());
            queue.mark_completed(&task.task.id, true);
        }
        let drained_count = queue.get_stats().completed;
        assert_eq!(drained_count, max_queue_size as u64);

        // Second wave submits concurrently while a consumer drains.
        let second_wave_submitters = 120usize;
        let second_barrier = Arc::new(Barrier::new(second_wave_submitters));
        let submit_done = Arc::new(AtomicBool::new(false));
        let accepted_b = Arc::new(AtomicUsize::new(0));
        let max_pending_seen = Arc::new(AtomicUsize::new(0));

        let mut second_wave = tokio::task::JoinSet::new();
        for i in 0..second_wave_submitters {
            let queue = Arc::clone(&queue);
            let barrier = Arc::clone(&second_barrier);
            let accepted_b = Arc::clone(&accepted_b);
            second_wave.spawn(async move {
                barrier.wait().await;
                if queue
                    .submit(test_agent(&format!("burst-b-{i}")), TaskPriority::Normal)
                    .await
                    .is_ok()
                {
                    accepted_b.fetch_add(1, Ordering::AcqRel);
                }
            });
        }

        let queue_for_consumer = Arc::clone(&queue);
        let submit_done_for_consumer = Arc::clone(&submit_done);
        let max_pending_seen_for_consumer = Arc::clone(&max_pending_seen);
        let consumer = tokio::spawn(async move {
            loop {
                let pending = queue_for_consumer.get_stats().pending;
                max_pending_seen_for_consumer.fetch_max(pending, Ordering::AcqRel);

                if let Some(task) = queue_for_consumer.pop() {
                    queue_for_consumer.mark_running(&task.task.id, 1, task.submitted_at.elapsed());
                    queue_for_consumer.mark_completed(&task.task.id, true);
                } else if submit_done_for_consumer.load(Ordering::Acquire) {
                    break;
                } else {
                    tokio::task::yield_now().await;
                }
            }
        });

        while let Some(joined) = second_wave.join_next().await {
            joined.expect("second wave submitter should not panic");
        }
        submit_done.store(true, Ordering::Release);
        consumer.await.expect("consumer should not panic");

        let accepted_second_wave = accepted_b.load(Ordering::Acquire) as u64;
        let stats = queue.get_stats();
        assert_eq!(stats.pending, 0);
        assert_eq!(stats.running, 0);
        assert_eq!(stats.failed, 0);
        assert_eq!(
            stats.completed,
            max_queue_size as u64 + accepted_second_wave,
            "completed count should include drained first phase and accepted second wave"
        );
        assert!(
            max_pending_seen.load(Ordering::Acquire) <= max_queue_size,
            "pending count must never exceed max_queue_size"
        );
    }

    #[tokio::test]
    async fn fairness_prevents_low_priority_starvation_under_high_pressure() {
        let queue = test_queue(1_000);
        let low_count = 8usize;
        let high_iterations = 200usize;

        for i in 0..low_count {
            queue
                .submit(test_agent(&format!("low-{i}")), TaskPriority::Low)
                .await
                .unwrap();
        }

        let mut low_popped_before_drain = 0usize;
        for i in 0..high_iterations {
            queue
                .submit(test_agent(&format!("high-{i}")), TaskPriority::High)
                .await
                .unwrap();
            let popped = queue.pop().expect("task should be available");
            if popped.priority == TaskPriority::Low {
                low_popped_before_drain += 1;
            }
        }

        assert!(
            low_popped_before_drain > 0,
            "fair scheduler should eventually serve low-priority tasks while high tasks keep arriving"
        );

        let mut remaining_low = 0usize;
        while let Some(task) = queue.pop() {
            if task.priority == TaskPriority::Low {
                remaining_low += 1;
            }
        }
        assert_eq!(
            low_popped_before_drain + remaining_low,
            low_count,
            "all low-priority tasks should be accounted for exactly once"
        );
        assert!(queue.pop().is_none());
    }

    #[tokio::test]
    async fn avg_wait_time_uses_started_tasks_not_completed_only() {
        let queue = test_queue(100);
        queue
            .submit(test_agent("t1"), TaskPriority::Normal)
            .await
            .unwrap();
        queue
            .submit(test_agent("t2"), TaskPriority::Normal)
            .await
            .unwrap();

        let t1 = queue.pop().unwrap();
        queue.mark_running(&t1.task.id, 0, Duration::from_millis(100));
        queue.mark_completed(&t1.task.id, true);

        let t2 = queue.pop().unwrap();
        queue.mark_running(&t2.task.id, 0, Duration::from_millis(300));

        let stats = queue.get_stats();
        assert_eq!(stats.completed, 1);
        assert_eq!(stats.running, 1);
        assert_eq!(
            stats.avg_wait_time_ms, 200,
            "avg_wait_time_ms should include all started tasks (100 + 300) / 2"
        );
    }

    #[tokio::test]
    async fn stats_snapshot_keeps_started_finished_running_invariant() {
        let total_tasks = 200usize;
        let queue = Arc::new(test_queue(total_tasks + 8));
        for i in 0..total_tasks {
            queue
                .submit(test_agent(&format!("inv-{i}")), TaskPriority::Normal)
                .await
                .unwrap();
        }

        let stop = Arc::new(AtomicBool::new(false));
        let mut workers = tokio::task::JoinSet::new();
        for worker_id in 0..4usize {
            let queue = Arc::clone(&queue);
            let stop = Arc::clone(&stop);
            workers.spawn(async move {
                loop {
                    if let Some(task) = queue.pop() {
                        queue.mark_running(&task.task.id, worker_id, Duration::from_millis(1));
                        queue.mark_completed(&task.task.id, true);
                    } else if stop.load(Ordering::Acquire) {
                        break;
                    } else {
                        tokio::task::yield_now().await;
                    }
                }
            });
        }

        for _ in 0..2_000 {
            let snapshot = queue.get_stats();
            let started = snapshot.completed + snapshot.failed + snapshot.running as u64;
            assert!(
                started >= snapshot.completed + snapshot.failed,
                "started must be >= finished in every snapshot"
            );
        }

        stop.store(true, Ordering::Release);
        while let Some(joined) = workers.join_next().await {
            joined.expect("worker should not panic");
        }

        let final_stats = queue.get_stats();
        assert_eq!(final_stats.pending, 0);
        assert_eq!(final_stats.running, 0);
        assert_eq!(final_stats.completed, total_tasks as u64);
        assert_eq!(final_stats.failed, 0);
    }
}
