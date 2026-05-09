//! Agent Task Runner - Background scheduler for agent tasks.
//!
//! The TaskRunner is responsible for:
//! - Polling storage for runnable tasks
//! - Executing agents on schedule
//! - Handling task lifecycle (start, complete, fail)
//! - Persisting execution state and transcript updates

use crate::models::{SteerMessage, SteerSource, Task, TaskMessageSource, TaskRun, TaskStatus};
use crate::performance::{
    TaskExecutor, TaskPriority, TaskQueue, TaskQueueConfig, WorkerPool, WorkerPoolConfig,
};
use crate::services::session::SessionService;
use crate::steer::SteerRegistry;
use crate::storage::TaskStorage;
use ai::agent::StreamEmitter;
use anyhow::{Result, anyhow};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
#[cfg(test)]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{RwLock, mpsc, oneshot};
use tokio::time::{Duration, Instant, interval};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
use types::ExecutionScope;

use super::events::{NoopEventEmitter, TaskEventEmitter, TaskStreamEvent};
use types::{DEFAULT_TASK_RUNNER_MAX_CONCURRENT_TASKS, DEFAULT_TASK_RUNNER_POLL_INTERVAL_MS};

use super::heartbeat::{
    HeartbeatEmitter, HeartbeatEvent, HeartbeatPulse, NoopHeartbeatEmitter, RunnerStatus,
    RunnerStatusEvent,
};
use super::outcome::ExecutionOutcome;
use finalizer::TaskRunFinalizer;
mod finalizer;
mod persistence;

#[cfg(test)]
mod tests;

pub type ExecutionResult = ExecutionOutcome;

fn task_stream_event_context(event: TaskStreamEvent, task: &Task, run_id: &str) -> TaskStreamEvent {
    event.with_run_context(
        Some(run_id.to_string()),
        (!task.chat_session_id.trim().is_empty()).then(|| task.chat_session_id.clone()),
        None,
        Some(ExecutionScope::durable_background(task.id.clone())),
    )
}

struct NoopStreamEmitter;

#[async_trait::async_trait]
impl StreamEmitter for NoopStreamEmitter {
    async fn emit_text_delta(&mut self, _text: &str) {}

    async fn emit_thinking_delta(&mut self, _text: &str) {}

    async fn emit_tool_call_start(&mut self, _id: &str, _name: &str, _arguments: &str) {}

    async fn emit_tool_call_result(
        &mut self,
        _id: &str,
        _name: &str,
        _result: &str,
        _success: bool,
    ) {
    }

    async fn emit_complete(&mut self) {}
}

impl TaskRunner {
    async fn record_task_progress(&self, task: &Task, run_id: &str, phase: &str, details: &str) {
        let message = if details.trim().is_empty() {
            phase.to_string()
        } else {
            format!("{phase}: {details}")
        };
        if let Some(session_service) = &self.session_service {
            let session_id = task.chat_session_id.trim();
            if !session_id.is_empty()
                && let Err(err) = session_service.append_task_turn_progress(
                    session_id,
                    run_id,
                    &message,
                    "task_runtime",
                )
            {
                warn!(
                    task_id = task.id,
                    run_id, phase, error = %err,
                    "Failed to persist task progress to session"
                );
            }
        }
        self.event_emitter
            .emit(task_stream_event_context(
                TaskStreamEvent::progress(&task.id, phase, None, Some(details.to_string())),
                task,
                run_id,
            ))
            .await;
    }
}

/// Message types for controlling the runner
#[derive(Debug)]
pub enum TaskRunnerCommand {
    /// Stop the runner
    Stop,
    /// Trigger immediate check for runnable tasks
    CheckNow,
    /// Run a specific task immediately (bypassing schedule)
    RunTaskNow(String),
    /// Stop a running task
    StopTask(String),
}

/// Configuration for the task runner.
#[derive(Debug, Clone)]
pub struct TaskRunnerConfig {
    /// How often to poll for runnable tasks (in milliseconds)
    pub poll_interval_ms: u64,
    /// Maximum concurrent task executions
    pub max_concurrent_tasks: usize,
    /// Number of worker-pool workers used to execute queued tasks.
    pub worker_count: usize,
    /// Default timeout for individual task execution (in seconds).
    ///
    /// `None` disables timeout by default.
    pub task_timeout_secs: Option<u64>,
    /// Threshold for recovering persisted tasks that appear stalled.
    ///
    /// `None` disables periodic stalled-task recovery.
    pub stall_timeout_secs: Option<u64>,
}

impl Default for TaskRunnerConfig {
    fn default() -> Self {
        Self {
            poll_interval_ms: DEFAULT_TASK_RUNNER_POLL_INTERVAL_MS,
            max_concurrent_tasks: DEFAULT_TASK_RUNNER_MAX_CONCURRENT_TASKS,
            worker_count: DEFAULT_TASK_RUNNER_MAX_CONCURRENT_TASKS,
            task_timeout_secs: None,
            stall_timeout_secs: None,
        }
    }
}

fn resolve_execution_timeout_secs(task: &Task, config: &TaskRunnerConfig) -> Option<u64> {
    let execution_timeout_secs = task.timeout_secs.or(config.task_timeout_secs);

    match task.resource_limits.as_ref() {
        Some(limits) if limits.max_duration_secs > 0 => match execution_timeout_secs {
            Some(timeout_secs) => Some(timeout_secs.min(limits.max_duration_secs)),
            None => Some(limits.max_duration_secs),
        },
        _ => execution_timeout_secs,
    }
}

/// Handle to control a running task runner.
pub struct TaskRunnerHandle {
    command_tx: mpsc::Sender<TaskRunnerCommand>,
}

impl TaskRunnerHandle {
    /// Stop the runner
    pub async fn stop(&self) -> Result<()> {
        self.command_tx
            .send(TaskRunnerCommand::Stop)
            .await
            .map_err(|e| anyhow!("Failed to send stop command: {}", e))
    }

    /// Trigger an immediate check for runnable tasks
    pub async fn check_now(&self) -> Result<()> {
        self.command_tx
            .send(TaskRunnerCommand::CheckNow)
            .await
            .map_err(|e| anyhow!("Failed to send check command: {}", e))
    }

    /// Run a specific task immediately
    pub async fn run_task_now(&self, task_id: String) -> Result<()> {
        self.command_tx
            .send(TaskRunnerCommand::RunTaskNow(task_id))
            .await
            .map_err(|e| anyhow!("Failed to send run task command: {}", e))
    }

    /// Stop a running task
    pub async fn stop_task(&self, task_id: String) -> Result<()> {
        self.command_tx
            .send(TaskRunnerCommand::StopTask(task_id))
            .await
            .map_err(|e| anyhow!("Failed to send stop task command: {}", e))
    }
}

/// Agent executor trait for dependency injection
#[async_trait::async_trait]
pub trait AgentExecutor: Send + Sync {
    /// Execute a task input through the same session-turn runtime used by
    /// foreground chat. Background tasks must be bound to a chat session.
    ///
    /// Returns an `ExecutionResult` containing the final output and metrics.
    #[allow(clippy::too_many_arguments)]
    async fn execute(
        &self,
        agent_id: &str,
        task_id: Option<&str>,
        input: Option<&str>,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
    ) -> Result<ExecutionResult>;
}

#[cfg(any(test, feature = "test-utils"))]
#[async_trait::async_trait]
pub trait NotificationSender: Send + Sync {
    async fn send(&self, task: &Task, success: bool, message: &str) -> Result<()>;

    async fn send_formatted(&self, message: &str) -> Result<()>;
}

#[cfg(any(test, feature = "test-utils"))]
pub struct NoopNotificationSender;

#[cfg(any(test, feature = "test-utils"))]
#[async_trait::async_trait]
impl NotificationSender for NoopNotificationSender {
    async fn send(&self, _task: &Task, _success: bool, _message: &str) -> Result<()> {
        Ok(())
    }

    async fn send_formatted(&self, _message: &str) -> Result<()> {
        Ok(())
    }
}

/// The main task runner that schedules and executes agent tasks.
pub struct TaskRunner {
    storage: Arc<TaskStorage>,
    executor: Arc<dyn AgentExecutor>,
    config: TaskRunnerConfig,
    running_tasks: Arc<RwLock<HashSet<String>>>,
    stop_senders: Arc<RwLock<HashMap<String, oneshot::Sender<()>>>>,
    pending_stop_receivers: Arc<RwLock<HashMap<String, oneshot::Receiver<()>>>>,
    resume_states: Arc<RwLock<HashMap<String, ai::AgentState>>>,
    task_queue: Arc<TaskQueue>,
    heartbeat_emitter: Arc<dyn HeartbeatEmitter>,
    event_emitter: Arc<dyn TaskEventEmitter>,
    sequence: AtomicU64,
    start_time: Instant,
    /// Optional JSONL-first session service for bound task transcript updates.
    session_service: Option<SessionService>,
    steer_registry: Arc<SteerRegistry>,
    #[cfg(test)]
    fail_start_task_run_once: Arc<AtomicBool>,
}

impl TaskRunner {
    /// Create a new task runner.
    pub fn new(
        storage: Arc<TaskStorage>,
        executor: Arc<dyn AgentExecutor>,
        #[cfg(any(test, feature = "test-utils"))] _notifier: Arc<dyn NotificationSender>,
        config: TaskRunnerConfig,
        steer_registry: Arc<SteerRegistry>,
    ) -> Self {
        let queue_config = TaskQueueConfig {
            max_concurrent: config.max_concurrent_tasks,
            ..Default::default()
        };
        let task_queue = Arc::new(TaskQueue::new(queue_config, None));

        Self {
            storage,
            executor,
            config,
            running_tasks: Arc::new(RwLock::new(HashSet::new())),
            stop_senders: Arc::new(RwLock::new(HashMap::new())),
            pending_stop_receivers: Arc::new(RwLock::new(HashMap::new())),
            resume_states: Arc::new(RwLock::new(HashMap::new())),
            task_queue,
            heartbeat_emitter: Arc::new(NoopHeartbeatEmitter),
            event_emitter: Arc::new(NoopEventEmitter),
            sequence: AtomicU64::new(0),
            start_time: Instant::now(),
            session_service: None,
            steer_registry,
            #[cfg(test)]
            fail_start_task_run_once: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Create a new task runner with a heartbeat emitter for status updates.
    pub fn with_heartbeat_emitter(
        storage: Arc<TaskStorage>,
        executor: Arc<dyn AgentExecutor>,
        #[cfg(any(test, feature = "test-utils"))] _notifier: Arc<dyn NotificationSender>,
        config: TaskRunnerConfig,
        heartbeat_emitter: Arc<dyn HeartbeatEmitter>,
        steer_registry: Arc<SteerRegistry>,
    ) -> Self {
        let queue_config = TaskQueueConfig {
            max_concurrent: config.max_concurrent_tasks,
            ..Default::default()
        };
        let task_queue = Arc::new(TaskQueue::new(queue_config, None));

        Self {
            storage,
            executor,
            config,
            running_tasks: Arc::new(RwLock::new(HashSet::new())),
            stop_senders: Arc::new(RwLock::new(HashMap::new())),
            pending_stop_receivers: Arc::new(RwLock::new(HashMap::new())),
            resume_states: Arc::new(RwLock::new(HashMap::new())),
            task_queue,
            heartbeat_emitter,
            event_emitter: Arc::new(NoopEventEmitter),
            sequence: AtomicU64::new(0),
            start_time: Instant::now(),
            session_service: None,
            steer_registry,
            #[cfg(test)]
            fail_start_task_run_once: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Attach a task event emitter for streaming updates.
    pub fn with_event_emitter(mut self, event_emitter: Arc<dyn TaskEventEmitter>) -> Self {
        self.event_emitter = event_emitter;
        self
    }

    /// Attach the canonical session service used for bound task transcripts.
    pub fn with_session_service(mut self, session_service: SessionService) -> Self {
        self.session_service = Some(session_service);
        self
    }

    /// Get a reference to the steer registry for sending messages to running tasks.
    pub fn steer_registry(&self) -> Arc<SteerRegistry> {
        self.steer_registry.clone()
    }

    async fn has_resume_intent(&self, task_id: &str) -> bool {
        self.resume_states.read().await.contains_key(task_id)
    }

    async fn staged_resume_intent(&self, task_id: &str) -> Option<ai::AgentState> {
        self.resume_states.read().await.get(task_id).cloned()
    }

    async fn activate_resume_intent_for_launch(&self, task_id: &str) -> Result<bool> {
        if !self.has_resume_intent(task_id).await {
            return Ok(false);
        }
        let Some(mut task) = self.storage.get_task(task_id)? else {
            return Err(anyhow!("Task {} not found", task_id));
        };
        if task.status != TaskStatus::Paused {
            return Ok(false);
        }
        task.status = TaskStatus::Active;
        task.updated_at = chrono::Utc::now().timestamp_millis();
        self.storage.save_task(&task)?;
        Ok(true)
    }

    async fn rollback_failed_launch_start(
        &self,
        task_id: &str,
        original_task: &Task,
        resume_launch: bool,
        run_id: Option<&str>,
        reason: &str,
    ) {
        let now = chrono::Utc::now().timestamp_millis();
        if let Some(run_id) = run_id
            && let Err(err) = self.storage.interrupt_task_run(run_id, now, reason)
        {
            warn!(
                task_id = %task_id,
                run_id = %run_id,
                error = %err,
                "Failed to mark launch task run as interrupted"
            );
        }

        match self.storage.get_task(task_id) {
            Ok(Some(mut latest)) => {
                if resume_launch {
                    latest.pause();
                    if let Err(err) = self.storage.save_task(&latest) {
                        warn!(
                            task_id = %task_id,
                            error = %err,
                            "Failed to rollback resumed task to paused state"
                        );
                    }
                } else {
                    let mut rollback = original_task.clone();
                    rollback.updated_at = now;
                    if let Err(err) = self.storage.save_task(&rollback) {
                        warn!(
                            task_id = %task_id,
                            error = %err,
                            "Failed to rollback task to pre-launch snapshot"
                        );
                    }
                }
            }
            Ok(None) => {}
            Err(err) => {
                warn!(
                    task_id = %task_id,
                    error = %err,
                    "Failed to load task during launch rollback"
                );
            }
        }

        self.cleanup_runtime_tracking(task_id).await;
    }

    async fn recover_active_run_with_finalizer(
        &self,
        task: &Task,
        run: &TaskRun,
        reason: &str,
        ended_at: i64,
    ) {
        let finalizer = TaskRunFinalizer::new(self, task.clone(), run.run_id.clone());
        let duration_ms = ended_at.saturating_sub(run.started_at);
        finalizer.finalize_interrupted(reason, duration_ms).await;
    }

    /// Start the runner and return a handle for controlling it
    pub fn start(self: Arc<Self>) -> TaskRunnerHandle {
        let (command_tx, command_rx) = mpsc::channel(32);
        let runner = self.clone();

        tokio::spawn(async move {
            runner.run_loop(command_rx).await;
        });

        TaskRunnerHandle { command_tx }
    }

    /// Main run loop
    async fn run_loop(self: Arc<Self>, mut command_rx: mpsc::Receiver<TaskRunnerCommand>) {
        let mut poll_interval = interval(Duration::from_millis(self.config.poll_interval_ms));

        info!(
            "TaskRunner started (poll_interval={}ms, max_concurrent={})",
            self.config.poll_interval_ms, self.config.max_concurrent_tasks
        );

        // Emit initial status
        self.emit_status(RunnerStatus::Running, Some("Runner started".to_string()))
            .await;

        // Recover tasks stuck in Running status from a previous daemon session.
        // When the daemon restarts, in-flight tasks lose their runtime context
        // but remain marked as Running in the database, preventing rescheduling.
        self.recover_stale_running_tasks();

        let executor = Arc::new(RunnerTaskExecutor {
            runner: self.clone(),
        });
        let mut worker_pool = WorkerPool::new(
            self.task_queue.clone(),
            executor,
            WorkerPoolConfig {
                worker_count: self.config.worker_count,
                idle_sleep: Duration::from_millis(10),
            },
        );
        worker_pool.start();

        loop {
            tokio::select! {
                _ = poll_interval.tick() => {
                    // Emit status pulse during each poll cycle
                    self.emit_heartbeat_pulse().await;
                    self.check_and_run_tasks().await;
                }
                cmd = command_rx.recv() => {
                    match cmd {
                        Some(TaskRunnerCommand::Stop) => {
                            info!("TaskRunner stopping...");
                            self.emit_status(RunnerStatus::Stopping, Some("Runner stopping".to_string())).await;
                            worker_pool.stop().await;
                            break;
                        }
                        Some(TaskRunnerCommand::CheckNow) => {
                            debug!("Manual check triggered");
                            self.check_and_run_tasks().await;
                        }
                        Some(TaskRunnerCommand::RunTaskNow(task_id)) => {
                            debug!("Manual run triggered for task: {}", task_id);
                            self.run_task_immediate(&task_id).await;
                        }
                        Some(TaskRunnerCommand::StopTask(task_id)) => {
                            debug!("Stop requested for task: {}", task_id);
                            self.stop_task_execution(&task_id).await;
                        }
                        None => {
                            info!("Command channel closed, stopping runner");
                            worker_pool.stop().await;
                            break;
                        }
                    }
                }
            }
        }

        self.emit_status(RunnerStatus::Stopped, Some("Runner stopped".to_string()))
            .await;
        info!("TaskRunner stopped");
    }

    /// Emit a heartbeat pulse with current status
    async fn emit_heartbeat_pulse(&self) {
        let sequence = self.sequence.fetch_add(1, Ordering::SeqCst) + 1;
        let running_count = self.running_tasks.read().await.len() as u32;
        let pending_count = self
            .storage
            .list_runnable_tasks(chrono::Utc::now().timestamp_millis())
            .map(|t| t.len() as u32)
            .unwrap_or(0);
        let uptime_ms = self.start_time.elapsed().as_millis() as u64;

        let pulse = HeartbeatPulse {
            sequence,
            timestamp: chrono::Utc::now().timestamp_millis(),
            active_tasks: running_count,
            pending_tasks: pending_count,
            uptime_ms,
            stats: None,
        };

        debug!(
            "Emitting heartbeat: seq={}, active={}, pending={}",
            sequence, running_count, pending_count
        );

        self.heartbeat_emitter
            .emit(HeartbeatEvent::Pulse(pulse))
            .await;
    }

    /// Emit a status change event
    async fn emit_status(&self, status: RunnerStatus, message: Option<String>) {
        self.heartbeat_emitter
            .emit(HeartbeatEvent::StatusChange(RunnerStatusEvent {
                status,
                timestamp: chrono::Utc::now().timestamp_millis(),
                message,
            }))
            .await;
    }

    /// Recover tasks stuck in Running status from a previous daemon session.
    ///
    /// On startup, no tasks should be Running (this daemon instance hasn't
    /// started any yet). Any Running tasks are leftovers from a previous
    /// daemon that was killed mid-execution. Reset them to Active so they
    /// can be rescheduled.
    ///
    /// This assumes a single active daemon per workspace/database. If multiple
    /// daemons operate on the same storage, this recovery strategy is unsafe.
    fn recover_stale_running_tasks(&self) {
        let now = chrono::Utc::now().timestamp_millis();
        let mut recovered_task_ids = HashSet::new();

        match self.storage.list_active_task_runs() {
            Ok(runs) => {
                for run in runs {
                    let task = match self.storage.get_task(&run.task_id) {
                        Ok(Some(task)) => task,
                        Ok(None) => continue,
                        Err(err) => {
                            error!(
                                "Failed to load task '{}' during startup recovery: {}",
                                run.task_id, err
                            );
                            continue;
                        }
                    };
                    futures::executor::block_on(self.recover_active_run_with_finalizer(
                        &task,
                        &run,
                        "Recovered after daemon restart",
                        now,
                    ));
                    recovered_task_ids.insert(run.task_id.clone());
                    if task.status == TaskStatus::Running
                        && let Err(err) = self.storage.resume_task(&task.id)
                    {
                        error!(
                            "Failed to recover stale Running task '{}' after run recovery: {}",
                            task.name, err
                        );
                    }
                }
            }
            Err(err) => {
                error!("Failed to list task runs for startup recovery: {}", err);
            }
        }

        let tasks = match self.storage.list_tasks() {
            Ok(t) => t,
            Err(e) => {
                error!("Failed to list tasks for startup recovery: {}", e);
                return;
            }
        };

        let mut recovered = 0;
        for task in tasks {
            if task.status == TaskStatus::Running {
                if recovered_task_ids.contains(&task.id) {
                    recovered += 1;
                    continue;
                }
                match self.storage.resume_task(&task.id) {
                    Ok(_) => {
                        info!(
                            "Recovered stale Running task '{}' ({}) → Active",
                            task.name, task.id
                        );
                        recovered += 1;
                    }
                    Err(e) => {
                        error!(
                            "Failed to recover stale Running task '{}': {}",
                            task.name, e
                        );
                    }
                }
            }
        }

        if recovered > 0 {
            info!(
                "Startup recovery: {} task(s) reset from Running to Active",
                recovered
            );
        }
    }

    /// Check for runnable tasks and execute them
    async fn check_and_run_tasks(&self) {
        let current_time = chrono::Utc::now().timestamp_millis();
        self.recover_stalled_running_tasks(current_time).await;

        let runnable_tasks = match self.storage.list_runnable_tasks(current_time) {
            Ok(tasks) => tasks,
            Err(e) => {
                error!("Failed to list runnable tasks: {}", e);
                return;
            }
        };

        if runnable_tasks.is_empty() {
            debug!("No runnable tasks found");
            return;
        }

        debug!("Found {} runnable tasks", runnable_tasks.len());

        // Check concurrency limit
        let running_count = self.running_tasks.read().await.len();
        let available_slots = self
            .config
            .max_concurrent_tasks
            .saturating_sub(running_count);

        if available_slots == 0 {
            debug!(
                "Max concurrent tasks ({}) reached, skipping this cycle",
                self.config.max_concurrent_tasks
            );
            return;
        }

        // Execute tasks up to available slots
        for task in runnable_tasks.into_iter().take(available_slots) {
            // Add to running set BEFORE enqueuing to prevent duplicates.
            let task_id = task.id.clone();
            let inserted = self.running_tasks.write().await.insert(task_id.clone());
            if !inserted {
                continue;
            }
            let (stop_tx, stop_rx) = oneshot::channel();
            self.stop_senders
                .write()
                .await
                .insert(task.id.clone(), stop_tx);
            self.pending_stop_receivers
                .write()
                .await
                .insert(task.id.clone(), stop_rx);

            if let Err(err) = self.task_queue.submit(task, TaskPriority::Normal).await {
                warn!("Failed to enqueue task {}: {:?}", task_id, err);
                self.cleanup_task_tracking(task_id.as_str()).await;
            }
        }
    }

    async fn recover_stalled_running_tasks(&self, current_time: i64) {
        let Some(timeout_secs) = self.config.stall_timeout_secs else {
            return;
        };
        let threshold_ms = timeout_secs.saturating_mul(1_000) as i64;
        let recover_before = current_time.saturating_sub(threshold_ms);
        let tracked_running = self.running_tasks.read().await.clone();
        let mut recovered_task_ids = HashSet::new();

        match self.storage.list_active_task_runs() {
            Ok(runs) => {
                for run in runs {
                    if tracked_running.contains(&run.task_id) {
                        continue;
                    }
                    if run.updated_at > recover_before {
                        continue;
                    }

                    let task = match self.storage.get_task(&run.task_id) {
                        Ok(Some(task)) => task,
                        Ok(None) => continue,
                        Err(error) => {
                            warn!(
                                "Failed to load task '{}' during stalled-task recovery: {}",
                                run.task_id, error
                            );
                            continue;
                        }
                    };
                    self.recover_active_run_with_finalizer(
                        &task,
                        &run,
                        "Recovered stalled task execution",
                        current_time,
                    )
                    .await;
                    recovered_task_ids.insert(run.task_id.clone());
                    if task.status == TaskStatus::Running
                        && let Err(error) = self.storage.resume_task(&task.id)
                    {
                        warn!(
                            "Failed to recover stalled Running task '{}' after run recovery: {}",
                            task.name, error
                        );
                    }
                }
            }
            Err(error) => {
                warn!(
                    "Failed to list task runs for stalled-task recovery: {}",
                    error
                );
            }
        }

        let tasks = match self.storage.list_tasks() {
            Ok(tasks) => tasks,
            Err(error) => {
                warn!("Failed to list tasks for stalled-task recovery: {}", error);
                return;
            }
        };

        let mut recovered = 0;
        for task in tasks {
            if task.status != TaskStatus::Running {
                continue;
            }
            if recovered_task_ids.contains(&task.id) {
                recovered += 1;
                continue;
            }
            if tracked_running.contains(&task.id) {
                continue;
            }
            if task.updated_at > recover_before {
                continue;
            }
            match self.storage.resume_task(&task.id) {
                Ok(_) => {
                    info!(
                        "Recovered stalled Running task '{}' ({}) → Active",
                        task.name, task.id
                    );
                    recovered += 1;
                }
                Err(error) => {
                    warn!(
                        "Failed to recover stalled Running task '{}' ({}): {}",
                        task.name, task.id, error
                    );
                }
            }
        }

        if recovered > 0 {
            info!(
                "Stalled-task recovery: {} task(s) reset from Running to Active",
                recovered
            );
        }
    }

    /// Run a task immediately, bypassing schedule check
    async fn run_task_immediate(&self, task_id: &str) {
        let task_id_owned = task_id.to_string();
        let resume_launch = self.has_resume_intent(&task_id_owned).await;
        match self.storage.get_active_task_run(&task_id_owned) {
            Ok(Some(run)) => {
                warn!(
                    "Cannot run task {} - active run {} is still recorded",
                    task_id, run.run_id
                );
                return;
            }
            Ok(None) => {}
            Err(err) => {
                warn!(
                    "Cannot verify active run state for task {}: {}",
                    task_id, err
                );
                return;
            }
        }
        {
            let mut running_tasks = self.running_tasks.write().await;
            if running_tasks.contains(task_id) {
                warn!("Task {} is already running", task_id);
                return;
            }
            if running_tasks.len() >= self.config.max_concurrent_tasks {
                warn!(
                    "Cannot run task {} - max concurrent tasks ({}) reached",
                    task_id, self.config.max_concurrent_tasks
                );
                return;
            }
            running_tasks.insert(task_id_owned.clone());
        }

        let original_task = match self.storage.get_task(&task_id_owned) {
            Ok(Some(task)) => task,
            Ok(None) => {
                warn!("Task {} not found", task_id);
                self.cleanup_runtime_tracking(&task_id_owned).await;
                return;
            }
            Err(error) => {
                error!("Failed to get task {}: {}", task_id, error);
                self.cleanup_runtime_tracking(&task_id_owned).await;
                return;
            }
        };
        if original_task.status == TaskStatus::Paused && !resume_launch {
            warn!("Cannot run paused task {}", task_id);
            self.cleanup_runtime_tracking(&task_id_owned).await;
            return;
        }
        if original_task.status == TaskStatus::Completed {
            warn!("Cannot run completed task {}", task_id);
            self.cleanup_runtime_tracking(&task_id_owned).await;
            return;
        }

        let resume_task_activated =
            match self.activate_resume_intent_for_launch(&task_id_owned).await {
                Ok(activated) => activated,
                Err(error) => {
                    error!(
                        "Failed to activate staged resume intent for task {}: {}",
                        task_id, error
                    );
                    self.cleanup_runtime_tracking(&task_id_owned).await;
                    return;
                }
            };

        let (stop_tx, stop_rx) = oneshot::channel();
        self.stop_senders
            .write()
            .await
            .insert(task_id_owned.clone(), stop_tx);
        self.pending_stop_receivers
            .write()
            .await
            .insert(task_id_owned.clone(), stop_rx);

        let task = match self.storage.get_task(&task_id_owned) {
            Ok(Some(task)) => task,
            Ok(None) => {
                warn!("Task {} not found", task_id_owned);
                self.rollback_failed_launch_start(
                    &task_id_owned,
                    &original_task,
                    resume_task_activated,
                    None,
                    "Task disappeared before queue submission",
                )
                .await;
                return;
            }
            Err(error) => {
                error!("Failed to load task {}: {}", task_id_owned, error);
                self.rollback_failed_launch_start(
                    &task_id_owned,
                    &original_task,
                    resume_task_activated,
                    None,
                    "Failed to load task before queue submission",
                )
                .await;
                return;
            }
        };

        if let Err(err) = self.task_queue.submit(task, TaskPriority::High).await {
            warn!("Failed to enqueue task {}: {:?}", task_id_owned, err);
            self.rollback_failed_launch_start(
                &task_id_owned,
                &original_task,
                resume_task_activated,
                None,
                "Failed to enqueue task",
            )
            .await;
        }
    }

    /// Stop a running task.
    async fn stop_task_execution(&self, task_id: &str) {
        if !self.running_tasks.read().await.contains(task_id) {
            debug!("Stop requested for task {}, but it is not running", task_id);
        }

        let stop_sender = self.stop_senders.write().await.remove(task_id);
        if let Some(sender) = stop_sender {
            if sender.send(()).is_err() {
                debug!(
                    "Stop signal for task {} dropped (task already finished)",
                    task_id
                );
            }
            return;
        }

        // No stop channel found; if the task is still marked running, persist the stop state.
        if let Ok(Some(task)) = self.storage.get_task(task_id)
            && task.status == TaskStatus::Running
            && let Err(e) = self
                .storage
                .control_task(task_id, crate::models::TaskControlAction::Stop)
        {
            error!("Failed to mark task {} as interrupted: {}", task_id, e);
        }
    }

    fn to_steer_source(source: &TaskMessageSource) -> SteerSource {
        match source {
            TaskMessageSource::User => SteerSource::User,
            TaskMessageSource::Agent => SteerSource::Api,
            TaskMessageSource::System => SteerSource::System,
        }
    }

    async fn forward_pending_messages(&self, task_id: &str) {
        let pending_messages = match self.storage.list_pending_task_messages(task_id, 32) {
            Ok(messages) => messages,
            Err(e) => {
                warn!(
                    "Failed to list pending task messages for task {}: {}",
                    task_id, e
                );
                return;
            }
        };

        if pending_messages.is_empty() {
            return;
        }

        for queued in pending_messages {
            let steer_message = SteerMessage::message(
                queued.message.clone(),
                Self::to_steer_source(&queued.source),
            );

            let sent = self.steer_registry.steer(task_id, steer_message).await;
            if sent && let Err(e) = self.storage.mark_task_message_consumed(&queued.id) {
                warn!(
                    "Failed to mark task message {} as consumed: {}",
                    queued.id, e
                );
            }
        }
    }

    /// Execute a single task
    /// Note: Task must already be in running_tasks before calling this
    async fn execute_task(
        &self,
        task_id: &str,
        stop_rx: Option<oneshot::Receiver<()>>,
    ) -> Result<bool> {
        let start_time = chrono::Utc::now().timestamp_millis();
        let stop_rx = match stop_rx {
            Some(receiver) => receiver,
            None => {
                error!(
                    "No stop receiver found for task '{}'. Refusing to run unstoppably tracked task.",
                    task_id
                );
                self.cleanup_runtime_tracking(task_id).await;
                return Err(anyhow!("Task {} has no stop channel", task_id));
            }
        };
        let resume_launch = self.has_resume_intent(task_id).await;
        let original_task = match self.storage.get_task(task_id) {
            Ok(Some(task)) => task,
            Ok(None) => {
                self.cleanup_runtime_tracking(task_id).await;
                return Err(anyhow!("Task {} not found before execution", task_id));
            }
            Err(error) => {
                self.cleanup_runtime_tracking(task_id).await;
                return Err(anyhow!(
                    "Failed to load task {} before execution: {}",
                    task_id,
                    error
                ));
            }
        };

        // Start execution in storage
        let task = match self.storage.start_task_execution(task_id) {
            Ok(task) => task,
            Err(e) => {
                error!("Failed to start task execution for {}: {}", task_id, e);
                self.rollback_failed_launch_start(
                    task_id,
                    &original_task,
                    resume_launch,
                    None,
                    "Failed to start task execution",
                )
                .await;
                return Err(anyhow!(
                    "Failed to start task execution for {}: {}",
                    task_id,
                    e
                ));
            }
        };

        info!(
            "Executing task '{}' (id={}, agent={}, mode={:?})",
            task.name, task.id, task.agent_id, task.execution_mode
        );

        // Install scope guard for panic-safe cleanup
        // This ensures resources are cleaned up even if the agent execution panics
        let task_id_for_guard = task.id.clone();
        let _cleanup_guard = scopeguard::guard(task_id_for_guard, |task_id| {
            Self::cleanup_agent_resources(&task_id);
        });

        let execution_mode_str = "api".to_string();

        let steer_rx = Some(self.steer_registry.register(task_id).await);

        // Start a lightweight message pump so queued task messages can be
        // injected into the running agent loop.
        let pump_cancel = CancellationToken::new();
        self.forward_pending_messages(task_id).await;

        let storage = self.storage.clone();
        let steer_registry = self.steer_registry.clone();
        let task_id_for_pump = task_id.to_string();
        let cancel = pump_cancel.clone();

        let mut message_pump = Some(tokio::spawn(async move {
            let mut ticker = interval(Duration::from_millis(500));

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = ticker.tick() => {}
                }

                let pending_messages =
                    match storage.list_pending_task_messages(&task_id_for_pump, 32) {
                        Ok(messages) => messages,
                        Err(e) => {
                            warn!(
                                "Failed to list pending task messages for task {}: {}",
                                task_id_for_pump, e
                            );
                            continue;
                        }
                    };

                if pending_messages.is_empty() {
                    continue;
                }

                for queued in pending_messages {
                    let source = match &queued.source {
                        TaskMessageSource::User => SteerSource::User,
                        TaskMessageSource::Agent => SteerSource::Api,
                        TaskMessageSource::System => SteerSource::System,
                    };
                    let steer_message = SteerMessage::message(queued.message.clone(), source);

                    let sent = steer_registry.steer(&task_id_for_pump, steer_message).await;
                    if sent && let Err(e) = storage.mark_task_message_consumed(&queued.id) {
                        warn!(
                            "Failed to mark task message {} as consumed: {}",
                            queued.id, e
                        );
                    }
                }
            }
        }));

        let resolved_input = self.resolve_task_input(&task);
        if task.chat_session_id.trim().is_empty() {
            anyhow::bail!("task '{}' is not bound to a chat session", task.id);
        }
        let run_id = format!(
            "{}-{}",
            chrono::Utc::now().timestamp_millis(),
            uuid::Uuid::new_v4()
        );
        let execution_timeout_secs = resolve_execution_timeout_secs(&task, &self.config);
        let resume_state = self.staged_resume_intent(task_id).await;

        let execution_id = resume_state
            .as_ref()
            .map(|state| state.execution_id.clone())
            .unwrap_or_else(|| run_id.clone());
        #[cfg(test)]
        if self.fail_start_task_run_once.swap(false, Ordering::SeqCst) {
            pump_cancel.cancel();
            if let Some(pump) = message_pump.take() {
                let _ = pump.await;
            }
            self.rollback_failed_launch_start(
                &task.id,
                &original_task,
                resume_launch,
                None,
                "Injected start_task_run failure",
            )
            .await;
            return Err(anyhow!(
                "Injected task run creation failure for {}",
                task.id
            ));
        }
        if let Err(err) =
            self.storage
                .start_task_run(&task.id, run_id.clone(), execution_id, start_time)
        {
            pump_cancel.cancel();
            if let Some(pump) = message_pump.take() {
                let _ = pump.await;
            }
            self.rollback_failed_launch_start(
                &task.id,
                &original_task,
                resume_launch,
                None,
                "Failed to create task run",
            )
            .await;
            return Err(anyhow!(
                "Failed to create task run for {}: {}",
                task.id,
                err
            ));
        }
        let finalizer = TaskRunFinalizer::new(self, task.clone(), run_id.clone());
        self.clear_resume_intent(task_id).await;
        self.event_emitter
            .emit(task_stream_event_context(
                TaskStreamEvent::started(&task.id, &task.name, &task.agent_id, &execution_mode_str),
                &task,
                &run_id,
            ))
            .await;
        self.record_task_progress(
            &task,
            &run_id,
            "Preparing session",
            "Binding task input to the session transcript",
        )
        .await;

        if resolved_input
            .as_deref()
            .is_none_or(|value: &str| value.trim().is_empty())
        {
            let duration_ms = chrono::Utc::now().timestamp_millis() - start_time;
            let reason = "Task requires non-empty input or input_template";
            let error_msg = format!("Execution error: {}", reason);

            error!("Task '{}' failed preflight: {}", task.name, reason);
            pump_cancel.cancel();
            if let Some(pump) = message_pump.take() {
                let _ = pump.await;
            }
            finalizer
                .finalize_failure(&error_msg, duration_ms, false)
                .await;
            self.clear_task_conversation_links(task_id).await;
            self.cleanup_task_tracking(task_id).await;
            return Ok(false);
        }
        self.persist_task_input_to_chat_session(&task, resolved_input.as_deref(), &run_id);
        self.record_task_progress(
            &task,
            &run_id,
            "Waiting for model response",
            "Request sent to the agent model",
        )
        .await;

        let step_emitter = Some(Box::new(NoopStreamEmitter) as Box<dyn StreamEmitter>);

        let exec_future = async {
            if resume_state.is_some() {
                warn!(
                    task_id = task.id,
                    "Ignoring legacy persisted agent state; task execution now resumes through the bound chat session"
                );
            }
            debug!("Using session-turn executor for task '{}'", task.name);
            if let Some(timeout_secs) = execution_timeout_secs {
                tokio::time::timeout(
                    Duration::from_secs(timeout_secs),
                    self.executor.execute(
                        &task.agent_id,
                        Some(&task.id),
                        resolved_input.as_deref(),
                        steer_rx,
                        step_emitter,
                    ),
                )
                .await
            } else {
                Ok(self
                    .executor
                    .execute(
                        &task.agent_id,
                        Some(&task.id),
                        resolved_input.as_deref(),
                        steer_rx,
                        step_emitter,
                    )
                    .await)
            }
        };

        enum PauseSignal {
            Paused,
            Interrupted,
            Deleted,
        }

        let result = tokio::select! {
            // Stop branch: resolves when user sends a stop signal.
            // If no receiver exists, pending() never resolves — task runs to completion.
            _ = stop_rx => {
                let duration_ms = chrono::Utc::now().timestamp_millis() - start_time;
                info!(
                    "Task '{}' stopped by user (duration={}ms)",
                    task.name, duration_ms
                );
                pump_cancel.cancel();
                if let Some(pump) = message_pump.take() {
                    let _ = pump.await;
                }
                finalizer
                    .finalize_interrupted("Stopped by user", duration_ms)
                    .await;
                if let Err(e) = self
                    .storage
                    .control_task(task_id, crate::models::TaskControlAction::Stop)
                {
                    error!("Failed to mark task {} as interrupted: {}", task_id, e);
                }
                self.cleanup_task_tracking(task_id).await;
                return Ok(false);
            }
            // Control branch: if control API sets task status to Paused or
            // Interrupted while this execution is running, stop current run immediately.
            pause_signal = async {
                let mut poll_interval = Duration::from_millis(250);
                loop {
                    tokio::time::sleep(poll_interval).await;
                    match self.storage.get_task(task_id) {
                        Ok(Some(stored_task)) if stored_task.status == TaskStatus::Paused => {
                            return PauseSignal::Paused;
                        }
                        Ok(Some(stored_task)) if stored_task.status == TaskStatus::Interrupted => {
                            return PauseSignal::Interrupted;
                        }
                        Ok(Some(_)) => {
                            poll_interval = Duration::from_millis(250);
                        }
                        Ok(None) => {
                            return PauseSignal::Deleted;
                        }
                        Err(err) => {
                            warn!("Failed to read task {} while waiting for pause signal: {}", task_id, err);
                            poll_interval = poll_interval.saturating_mul(2).min(Duration::from_secs(5));
                        }
                    }
                }
            } => {
                let duration_ms = chrono::Utc::now().timestamp_millis() - start_time;
                pump_cancel.cancel();
                if let Some(pump) = message_pump.take() {
                    let _ = pump.await;
                }
                match pause_signal {
                    PauseSignal::Paused => {
                        info!(
                            "Task '{}' interrupted by pause request (duration={}ms)",
                            task.name, duration_ms
                        );
                        finalizer
                            .finalize_interrupted("Paused by user", duration_ms)
                            .await;
                        if let Err(e) = self.storage.pause_task(task_id) {
                            error!("Failed to keep task {} paused: {}", task_id, e);
                        }
                    }
                    PauseSignal::Interrupted => {
                        info!(
                            "Task '{}' stopped by user request (duration={}ms)",
                            task.name, duration_ms
                        );
                        finalizer
                            .finalize_interrupted("Stopped by user", duration_ms)
                            .await;
                        if let Err(e) = self
                            .storage
                            .control_task(task_id, crate::models::TaskControlAction::Stop)
                        {
                            error!("Failed to keep task {} interrupted: {}", task_id, e);
                        }
                    }
                    PauseSignal::Deleted => {
                        info!(
                            "Task '{}' stopped because task record was deleted (duration={}ms)",
                            task.name, duration_ms
                        );
                        finalizer
                            .finalize_interrupted("Task deleted", duration_ms)
                            .await;
                    }
                }
                self.cleanup_task_tracking(task_id).await;
                return Ok(false);
            }
            result = exec_future => result,
        };

        pump_cancel.cancel();
        if let Some(pump) = message_pump.take() {
            let _ = pump.await;
        }

        let duration_ms = chrono::Utc::now().timestamp_millis() - start_time;
        let mut success = false;

        match result {
            Ok(Ok(exec_result)) => {
                success = true;
                // Success
                info!(
                    "Task '{}' completed successfully (duration={}ms)",
                    task.name, duration_ms
                );
                finalizer.finalize_success(&exec_result, duration_ms).await;
            }
            Ok(Err(e)) => {
                // Execution error
                let error_msg = format!("Execution error: {}", e);
                error!("Task '{}' failed: {}", task.name, error_msg);
                finalizer
                    .finalize_failure(&error_msg, duration_ms, true)
                    .await;
            }
            Err(_) => {
                // Timeout
                let timeout_secs = execution_timeout_secs.unwrap_or(0);
                let error_msg = if timeout_secs > 0 {
                    format!("Task timed out after {} seconds", timeout_secs)
                } else {
                    "Task timed out".to_string()
                };
                error!("Task '{}' timed out", task.name);
                finalizer
                    .finalize_timeout(&error_msg, timeout_secs, duration_ms)
                    .await;
            }
        }

        self.clear_task_conversation_links(task_id).await;
        self.cleanup_task_tracking(task_id).await;
        Ok(success)
    }

    /// Get the number of currently running tasks
    pub async fn running_task_count(&self) -> usize {
        self.running_tasks.read().await.len()
    }

    /// Get the IDs of currently running tasks
    pub async fn running_task_ids(&self) -> Vec<String> {
        self.running_tasks.read().await.iter().cloned().collect()
    }
}

struct RunnerTaskExecutor {
    runner: Arc<TaskRunner>,
}

#[async_trait::async_trait]
impl TaskExecutor for RunnerTaskExecutor {
    async fn execute(&self, task: &Task) -> Result<bool> {
        let stop_rx = self.runner.take_stop_receiver(&task.id).await;
        self.runner.execute_task(&task.id, stop_rx).await
    }
}
