//! Agent Task Runner - Background scheduler for agent tasks.
//!
//! The BackgroundAgentRunner is responsible for:
//! - Polling storage for runnable tasks
//! - Executing agents on schedule
//! - Handling task lifecycle (start, complete, fail)
//! - Persisting conversation memory to long-term storage
//! - Sending notifications on completion/failure

use crate::channel::{ChannelRouter, MessageLevel, OutboundMessage};
use crate::hooks::HookExecutor;
use crate::models::{
    BackgroundAgent, BackgroundAgentStatus, BackgroundMessageSource, ExecutionMode, HookContext,
    MemoryConfig, MemoryScope, NotificationConfig, SteerMessage, SteerSource,
};
use crate::performance::{
    TaskExecutor, TaskPriority, TaskQueue, TaskQueueConfig, WorkerPool, WorkerPoolConfig,
};
use crate::runtime::output::{ensure_success_output, format_error_output};
use crate::steer::SteerRegistry;
use crate::storage::{BackgroundAgentStorage, MemoryStorage};
use anyhow::{Result, anyhow};
use restflow_ai::agent::StreamEmitter;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{RwLock, mpsc, oneshot};
use tokio::time::{Duration, Instant, interval};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use super::broadcast_emitter::BroadcastStreamEmitter;
use super::events::{NoopEventEmitter, TaskEventEmitter, TaskStreamEvent};
use super::persist::MemoryPersister;
use crate::runtime::trace::{
    RestflowTrace, TraceEvent, append_trace_event, build_restflow_telemetry_emitter,
};
use restflow_traits::{
    DEFAULT_BACKGROUND_RUNNER_MAX_CONCURRENT_TASKS, DEFAULT_BACKGROUND_RUNNER_POLL_INTERVAL_MS,
};

use super::heartbeat::{
    HeartbeatEmitter, HeartbeatEvent, HeartbeatPulse, NoopHeartbeatEmitter, RunnerStatus,
    RunnerStatusEvent,
};
use super::outcome::ExecutionOutcome;
mod notification;
mod persistence;

#[cfg(test)]
mod tests;

pub type ExecutionResult = ExecutionOutcome;

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

/// Message types for controlling the runner
#[derive(Debug)]
pub enum RunnerCommand {
    /// Stop the runner
    Stop,
    /// Trigger immediate check for runnable tasks
    CheckNow,
    /// Run a specific task immediately (bypassing schedule)
    RunTaskNow(String),
    /// Stop a running task
    StopTask(String),
    /// Resume a task from a checkpoint
    ResumeTask {
        task_id: String,
        payload: crate::models::ResumePayload,
    },
}

/// Configuration for the BackgroundAgentRunner
#[derive(Debug, Clone)]
pub struct RunnerConfig {
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

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            poll_interval_ms: DEFAULT_BACKGROUND_RUNNER_POLL_INTERVAL_MS,
            max_concurrent_tasks: DEFAULT_BACKGROUND_RUNNER_MAX_CONCURRENT_TASKS,
            worker_count: DEFAULT_BACKGROUND_RUNNER_MAX_CONCURRENT_TASKS,
            task_timeout_secs: None,
            stall_timeout_secs: None,
        }
    }
}

/// Handle to control a running BackgroundAgentRunner
pub struct RunnerHandle {
    command_tx: mpsc::Sender<RunnerCommand>,
}

impl RunnerHandle {
    /// Stop the runner
    pub async fn stop(&self) -> Result<()> {
        self.command_tx
            .send(RunnerCommand::Stop)
            .await
            .map_err(|e| anyhow!("Failed to send stop command: {}", e))
    }

    /// Trigger an immediate check for runnable tasks
    pub async fn check_now(&self) -> Result<()> {
        self.command_tx
            .send(RunnerCommand::CheckNow)
            .await
            .map_err(|e| anyhow!("Failed to send check command: {}", e))
    }

    /// Run a specific task immediately
    pub async fn run_task_now(&self, task_id: String) -> Result<()> {
        self.command_tx
            .send(RunnerCommand::RunTaskNow(task_id))
            .await
            .map_err(|e| anyhow!("Failed to send run task command: {}", e))
    }

    /// Stop a running task
    pub async fn stop_task(&self, task_id: String) -> Result<()> {
        self.command_tx
            .send(RunnerCommand::StopTask(task_id))
            .await
            .map_err(|e| anyhow!("Failed to send stop task command: {}", e))
    }

    /// Resume a task from a checkpoint
    pub async fn resume_task(
        &self,
        task_id: String,
        payload: crate::models::ResumePayload,
    ) -> Result<()> {
        self.command_tx
            .send(RunnerCommand::ResumeTask { task_id, payload })
            .await
            .map_err(|e| anyhow!("Failed to send resume task command: {}", e))
    }
}

/// Agent executor trait for dependency injection
#[async_trait::async_trait]
pub trait AgentExecutor: Send + Sync {
    /// Execute an agent with the given input.
    ///
    /// Returns an `ExecutionResult` containing the output and conversation
    /// messages for optional memory persistence.
    async fn execute(
        &self,
        agent_id: &str,
        background_task_id: Option<&str>,
        input: Option<&str>,
        memory_config: &MemoryConfig,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
    ) -> Result<ExecutionResult>;

    /// Execute an agent with an optional streaming emitter for per-step updates.
    ///
    /// Default implementation keeps backward compatibility by delegating to
    /// `execute` and ignoring the emitter.
    async fn execute_with_emitter(
        &self,
        agent_id: &str,
        background_task_id: Option<&str>,
        input: Option<&str>,
        memory_config: &MemoryConfig,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
    ) -> Result<ExecutionResult> {
        let _ = emitter;
        self.execute(agent_id, background_task_id, input, memory_config, steer_rx)
            .await
    }

    /// Execute an agent from a previously persisted state.
    ///
    /// Default implementation falls back to a fresh execution.
    async fn execute_from_state(
        &self,
        agent_id: &str,
        background_task_id: Option<&str>,
        state: restflow_ai::AgentState,
        memory_config: &MemoryConfig,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
    ) -> Result<ExecutionResult> {
        let _ = state;
        self.execute_with_emitter(
            agent_id,
            background_task_id,
            None,
            memory_config,
            steer_rx,
            emitter,
        )
        .await
    }
}

/// Notification sender trait for dependency injection
#[async_trait::async_trait]
pub trait NotificationSender: Send + Sync {
    /// Send a notification with the given configuration
    async fn send(
        &self,
        config: &NotificationConfig,
        task: &BackgroundAgent,
        success: bool,
        message: &str,
    ) -> Result<()>;

    /// Send a notification message that is already fully formatted.
    async fn send_formatted(&self, message: &str) -> Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NotificationDispatchStatus {
    Sent,
    Skipped,
}

#[async_trait::async_trait]
trait NotificationSink: Send + Sync {
    fn name(&self) -> &'static str;

    async fn send(
        &self,
        task: &BackgroundAgent,
        level: MessageLevel,
        message: &str,
    ) -> Result<NotificationDispatchStatus>;
}

struct ChannelRouterNotificationSink {
    router: Arc<RwLock<Option<Arc<ChannelRouter>>>>,
}

#[async_trait::async_trait]
impl NotificationSink for ChannelRouterNotificationSink {
    fn name(&self) -> &'static str {
        "channel_router"
    }

    async fn send(
        &self,
        task: &BackgroundAgent,
        level: MessageLevel,
        message: &str,
    ) -> Result<NotificationDispatchStatus> {
        let Some(router) = self.router.read().await.as_ref().cloned() else {
            return Ok(NotificationDispatchStatus::Skipped);
        };

        // Prefer task-bound conversations to avoid confusing global broadcasts.
        let task_conversations = router.find_conversations_by_task(&task.id).await;
        if !task_conversations.is_empty() {
            let mut any_sent = false;
            let mut failures: Vec<String> = Vec::new();

            for context in task_conversations {
                let mut outbound = OutboundMessage::new(&context.conversation_id, message);
                outbound.level = level;
                // Keep payload plain to avoid markdown parse failures in adapters.
                outbound.parse_mode = None;

                match router.send_to(context.channel_type, outbound).await {
                    Ok(()) => {
                        any_sent = true;
                        info!(
                            "Notification sent to task-bound conversation '{}' for task '{}'",
                            context.conversation_id, task.name
                        );
                    }
                    Err(err) => {
                        warn!(
                            "Failed sending notification to conversation '{}' for task '{}': {}",
                            context.conversation_id, task.name, err
                        );
                        failures.push(format!("{}: {}", context.conversation_id, err));
                    }
                }
            }

            if any_sent {
                return Ok(NotificationDispatchStatus::Sent);
            }
            if !failures.is_empty() {
                return Err(anyhow!(
                    "Task-bound notification delivery failed: {}",
                    failures.join(" | ")
                ));
            }
            return Ok(NotificationDispatchStatus::Skipped);
        }

        let mut any_sent = false;
        let mut failures: Vec<String> = Vec::new();
        for (channel_type, result) in router.broadcast(message, level).await {
            match result {
                Ok(()) => {
                    any_sent = true;
                    info!(
                        "Notification sent via {:?} for task '{}'",
                        channel_type, task.name
                    );
                }
                Err(err) => {
                    warn!(
                        "Failed to send notification via {:?} for task '{}': {}",
                        channel_type, task.name, err
                    );
                    failures.push(format!("{:?}: {}", channel_type, err));
                }
            }
        }

        if any_sent {
            Ok(NotificationDispatchStatus::Sent)
        } else if !failures.is_empty() {
            Err(anyhow!(
                "Channel router did not deliver notification: {}",
                failures.join(" | ")
            ))
        } else {
            Ok(NotificationDispatchStatus::Skipped)
        }
    }
}

struct TelegramNotificationSink {
    notifier: Arc<dyn NotificationSender>,
}

#[async_trait::async_trait]
impl NotificationSink for TelegramNotificationSink {
    fn name(&self) -> &'static str {
        "telegram"
    }

    async fn send(
        &self,
        _task: &BackgroundAgent,
        _level: MessageLevel,
        message: &str,
    ) -> Result<NotificationDispatchStatus> {
        self.notifier.send_formatted(message).await?;
        Ok(NotificationDispatchStatus::Sent)
    }
}

/// The main BackgroundAgentRunner that schedules and executes agent tasks
pub struct BackgroundAgentRunner {
    storage: Arc<BackgroundAgentStorage>,
    executor: Arc<dyn AgentExecutor>,
    notifier: Arc<dyn NotificationSender>,
    config: RunnerConfig,
    running_tasks: Arc<RwLock<HashSet<String>>>,
    stop_senders: Arc<RwLock<HashMap<String, oneshot::Sender<()>>>>,
    pending_stop_receivers: Arc<RwLock<HashMap<String, oneshot::Receiver<()>>>>,
    resume_states: Arc<RwLock<HashMap<String, restflow_ai::AgentState>>>,
    task_queue: Arc<TaskQueue>,
    heartbeat_emitter: Arc<dyn HeartbeatEmitter>,
    event_emitter: Arc<dyn TaskEventEmitter>,
    sequence: AtomicU64,
    start_time: Instant,
    /// Optional memory persister for long-term memory storage
    memory_persister: Option<MemoryPersister>,
    /// Optional hook executor for lifecycle automation
    hook_executor: Option<Arc<HookExecutor>>,
    steer_registry: Arc<SteerRegistry>,
    /// Optional channel router for broadcasting notifications to all configured channels
    channel_router: Arc<RwLock<Option<Arc<ChannelRouter>>>>,
}

impl BackgroundAgentRunner {
    /// Create a new BackgroundAgentRunner
    pub fn new(
        storage: Arc<BackgroundAgentStorage>,
        executor: Arc<dyn AgentExecutor>,
        notifier: Arc<dyn NotificationSender>,
        config: RunnerConfig,
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
            notifier,
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
            memory_persister: None,
            hook_executor: None,
            steer_registry,
            channel_router: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a new BackgroundAgentRunner with a heartbeat emitter for status updates
    pub fn with_heartbeat_emitter(
        storage: Arc<BackgroundAgentStorage>,
        executor: Arc<dyn AgentExecutor>,
        notifier: Arc<dyn NotificationSender>,
        config: RunnerConfig,
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
            notifier,
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
            memory_persister: None,
            hook_executor: None,
            steer_registry,
            channel_router: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a new BackgroundAgentRunner with memory persistence enabled.
    ///
    /// When memory persistence is enabled, conversation messages from task
    /// executions are stored in long-term memory for later retrieval and search.
    pub fn with_memory_persistence(
        storage: Arc<BackgroundAgentStorage>,
        executor: Arc<dyn AgentExecutor>,
        notifier: Arc<dyn NotificationSender>,
        config: RunnerConfig,
        heartbeat_emitter: Arc<dyn HeartbeatEmitter>,
        memory_storage: MemoryStorage,
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
            notifier,
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
            memory_persister: Some(MemoryPersister::new(memory_storage)),
            hook_executor: None,
            steer_registry,
            channel_router: Arc::new(RwLock::new(None)),
        }
    }

    /// Attach a task event emitter for streaming updates.
    pub fn with_event_emitter(mut self, event_emitter: Arc<dyn TaskEventEmitter>) -> Self {
        self.event_emitter = event_emitter;
        self
    }

    /// Attach a hook executor for lifecycle actions.
    pub fn with_hook_executor(mut self, hook_executor: Arc<HookExecutor>) -> Self {
        self.hook_executor = Some(hook_executor);
        self
    }

    /// Replace the internal channel-router handle with a shared pointer.
    ///
    /// This is useful when other runtime components (for example reply senders)
    /// need to observe the same router availability/updates.
    pub fn with_channel_router_handle(
        mut self,
        channel_router: Arc<RwLock<Option<Arc<ChannelRouter>>>>,
    ) -> Self {
        self.channel_router = channel_router;
        self
    }

    /// Set the channel router for broadcasting notifications to all configured channels.
    ///
    /// When a channel router is set, task notifications are broadcast through
    /// configured channels (e.g., Telegram) instead of requiring per-task
    /// notification configuration.
    pub async fn set_channel_router(&self, router: Arc<ChannelRouter>) {
        let mut guard = self.channel_router.write().await;
        *guard = Some(router);
    }

    /// Get a reference to the steer registry for sending messages to running tasks.
    pub fn steer_registry(&self) -> Arc<SteerRegistry> {
        self.steer_registry.clone()
    }

    /// Get the execution trace storage for persisting runtime events.
    fn execution_trace_storage(&self) -> &crate::storage::ExecutionTraceStorage {
        self.storage.execution_traces()
    }

    /// Install RestFlow git hooks in the given repository.
    ///
    /// This installs a pre-commit hook that prevents background agents from
    /// committing directly to main/master branches.
    pub fn install_git_hooks(repo_path: &str) {
        let hook_path = format!("{}/.git/hooks/pre-commit", repo_path);

        // Only install if no existing pre-commit hook
        if std::path::Path::new(&hook_path).exists() {
            debug!("Pre-commit hook already exists at {}", hook_path);
            return;
        }

        let hook_content = include_str!("../../../assets/hooks/pre-commit");
        if let Ok(()) = std::fs::write(&hook_path, hook_content) {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ =
                    std::fs::set_permissions(&hook_path, std::fs::Permissions::from_mode(0o755));
            }
            info!("Installed RestFlow pre-commit hook at {}", hook_path);
        } else {
            warn!("Failed to install pre-commit hook at {}", hook_path);
        }
    }

    /// Start the runner and return a handle for controlling it
    pub fn start(self: Arc<Self>) -> RunnerHandle {
        let (command_tx, command_rx) = mpsc::channel(32);
        let runner = self.clone();

        tokio::spawn(async move {
            runner.run_loop(command_rx).await;
        });

        RunnerHandle { command_tx }
    }

    /// Main run loop
    async fn run_loop(self: Arc<Self>, mut command_rx: mpsc::Receiver<RunnerCommand>) {
        let mut poll_interval = interval(Duration::from_millis(self.config.poll_interval_ms));

        info!(
            "BackgroundAgentRunner started (poll_interval={}ms, max_concurrent={})",
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
                        Some(RunnerCommand::Stop) => {
                            info!("BackgroundAgentRunner stopping...");
                            self.emit_status(RunnerStatus::Stopping, Some("Runner stopping".to_string())).await;
                            worker_pool.stop().await;
                            break;
                        }
                        Some(RunnerCommand::CheckNow) => {
                            debug!("Manual check triggered");
                            self.check_and_run_tasks().await;
                        }
                        Some(RunnerCommand::RunTaskNow(task_id)) => {
                            debug!("Manual run triggered for task: {}", task_id);
                            self.run_task_immediate(&task_id).await;
                        }
                        Some(RunnerCommand::StopTask(task_id)) => {
                            debug!("Stop requested for task: {}", task_id);
                            self.stop_task_execution(&task_id).await;
                        }
                        Some(RunnerCommand::ResumeTask { task_id, payload }) => {
                            info!(task_id = %task_id, "Resume from checkpoint requested");
                            self.resume_from_checkpoint(&task_id, payload).await;
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
        info!("BackgroundAgentRunner stopped");
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
        let tasks = match self.storage.list_tasks() {
            Ok(t) => t,
            Err(e) => {
                error!("Failed to list tasks for startup recovery: {}", e);
                return;
            }
        };

        let mut recovered = 0;
        for task in tasks {
            if task.status == BackgroundAgentStatus::Running {
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

        if let Err(e) = self.storage.cleanup_expired_checkpoints() {
            warn!(
                "Failed to cleanup expired checkpoints during startup: {}",
                e
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

        let tasks = match self.storage.list_tasks() {
            Ok(tasks) => tasks,
            Err(error) => {
                warn!("Failed to list tasks for stalled-task recovery: {}", error);
                return;
            }
        };

        let mut recovered = 0;
        for task in tasks {
            if task.status != BackgroundAgentStatus::Running {
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

        // Verify task exists and is not paused/completed
        match self.storage.get_task(&task_id_owned) {
            Ok(Some(task)) => {
                if task.status == BackgroundAgentStatus::Paused {
                    warn!("Cannot run paused task {}", task_id);
                    self.cleanup_task_tracking(&task_id_owned).await;
                    return;
                }
                if task.status == BackgroundAgentStatus::Completed {
                    warn!("Cannot run completed task {}", task_id);
                    self.cleanup_task_tracking(&task_id_owned).await;
                    return;
                }
            }
            Ok(None) => {
                warn!("Task {} not found", task_id);
                self.cleanup_task_tracking(&task_id_owned).await;
                return;
            }
            Err(e) => {
                error!("Failed to get task {}: {}", task_id, e);
                self.cleanup_task_tracking(&task_id_owned).await;
                return;
            }
        }

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
                self.cleanup_task_tracking(&task_id_owned).await;
                return;
            }
            Err(e) => {
                error!("Failed to load task {}: {}", task_id_owned, e);
                self.cleanup_task_tracking(&task_id_owned).await;
                return;
            }
        };

        if let Err(err) = self.task_queue.submit(task, TaskPriority::High).await {
            warn!("Failed to enqueue task {}: {:?}", task_id_owned, err);
            self.cleanup_task_tracking(&task_id_owned).await;
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
            && task.status == BackgroundAgentStatus::Running
            && let Err(e) = self.storage.control_background_agent(
                task_id,
                crate::models::BackgroundAgentControlAction::Stop,
            )
        {
            error!("Failed to mark task {} as interrupted: {}", task_id, e);
        }
    }

    async fn resume_from_checkpoint(&self, task_id: &str, payload: crate::models::ResumePayload) {
        // Load checkpoint from storage
        let checkpoint = match self.storage.load_checkpoint_by_task_id(task_id) {
            Ok(Some(cp)) => cp,
            Ok(None) => {
                warn!("No checkpoint found for task {}", task_id);
                return;
            }
            Err(e) => {
                error!("Failed to load checkpoint for task {}: {}", task_id, e);
                return;
            }
        };

        // Deserialize checkpointed agent state for real resume.
        let restored_state: Option<restflow_ai::AgentState> =
            match serde_json::from_slice(&checkpoint.state_json) {
                Ok(state) => Some(state),
                Err(e) => {
                    error!(
                        "Failed to deserialize checkpoint state for task {}: {}",
                        task_id, e
                    );
                    None
                }
            };

        // Mark checkpoint as resumed
        let mut cp = checkpoint;
        cp.mark_resumed();
        if let Err(e) = self.storage.save_checkpoint(&cp) {
            warn!("Failed to mark checkpoint as resumed: {}", e);
        }
        if let Some(savepoint_id) = cp.savepoint_id
            && let Err(e) = self.storage.delete_checkpoint_savepoint(savepoint_id)
        {
            warn!(
                "Failed to delete checkpoint savepoint {} for task {}: {}",
                savepoint_id, task_id, e
            );
        }

        // Transition task status back to Running
        if let Ok(Some(mut task)) = self.storage.get_task(task_id) {
            task.status = if payload.approved {
                BackgroundAgentStatus::Running
            } else {
                BackgroundAgentStatus::Paused
            };
            task.updated_at = chrono::Utc::now().timestamp_millis();
            if let Err(e) = self.storage.save_task(&task) {
                error!(
                    "Failed to update task status after checkpoint decision: {}",
                    e
                );
                return;
            }
        }

        // Emit event
        {
            let detail = format!(
                "Resumed from checkpoint {}: {}",
                cp.id,
                if payload.approved {
                    "approved"
                } else {
                    "denied"
                }
            );
            self.event_emitter
                .emit(TaskStreamEvent::progress(
                    task_id,
                    "resumed",
                    None,
                    Some(detail),
                ))
                .await;
        }

        // Run the task. If state deserialization succeeded, execution resumes
        // from that state on the next queue dispatch.
        info!(
            task_id = %task_id,
            checkpoint_id = %cp.id,
            approved = payload.approved,
            "Resuming task from checkpoint"
        );
        if payload.approved {
            if let Some(state) = restored_state {
                self.resume_states
                    .write()
                    .await
                    .insert(task_id.to_string(), state);
            }
            self.run_task_immediate(task_id).await;
        }
    }

    fn to_steer_source(source: &BackgroundMessageSource) -> SteerSource {
        match source {
            BackgroundMessageSource::User => SteerSource::User,
            BackgroundMessageSource::Agent => SteerSource::Api,
            BackgroundMessageSource::System => SteerSource::Hook,
        }
    }

    async fn forward_pending_messages(&self, task_id: &str) {
        let pending_messages = match self.storage.list_pending_background_messages(task_id, 32) {
            Ok(messages) => messages,
            Err(e) => {
                warn!(
                    "Failed to list pending background messages for task {}: {}",
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
            if sent && let Err(e) = self.storage.mark_background_message_consumed(&queued.id) {
                warn!(
                    "Failed to mark background message {} as consumed: {}",
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

        // Start execution in storage
        let task = match self.storage.start_task_execution(task_id) {
            Ok(task) => task,
            Err(e) => {
                error!("Failed to start task execution for {}: {}", task_id, e);
                self.cleanup_task_tracking(task_id).await;
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

        let execution_mode_str = match &task.execution_mode {
            ExecutionMode::Api => "api".to_string(),
            ExecutionMode::Cli(cfg) => format!("cli:{}", cfg.binary),
        };

        self.event_emitter
            .emit(TaskStreamEvent::started(
                &task.id,
                &task.name,
                &task.agent_id,
                &execution_mode_str,
            ))
            .await;
        self.fire_hooks(&HookContext::from_started(&task)).await;

        // Register steer channel for API-based tasks
        let steer_rx = if matches!(task.execution_mode, ExecutionMode::Api) {
            Some(self.steer_registry.register(task_id).await)
        } else {
            None
        };

        // Start a lightweight message pump so queued background messages can be
        // injected into the running agent loop.
        let pump_cancel = CancellationToken::new();
        let mut message_pump = if matches!(task.execution_mode, ExecutionMode::Api) {
            self.forward_pending_messages(task_id).await;

            let storage = self.storage.clone();
            let steer_registry = self.steer_registry.clone();
            let task_id = task_id.to_string();
            let cancel = pump_cancel.clone();

            Some(tokio::spawn(async move {
                let mut ticker = interval(Duration::from_millis(500));

                loop {
                    tokio::select! {
                        _ = cancel.cancelled() => break,
                        _ = ticker.tick() => {}
                    }

                    let pending_messages =
                        match storage.list_pending_background_messages(&task_id, 32) {
                            Ok(messages) => messages,
                            Err(e) => {
                                warn!(
                                    "Failed to list pending background messages for task {}: {}",
                                    task_id, e
                                );
                                continue;
                            }
                        };

                    if pending_messages.is_empty() {
                        continue;
                    }

                    for queued in pending_messages {
                        let source = match &queued.source {
                            BackgroundMessageSource::User => SteerSource::User,
                            BackgroundMessageSource::Agent => SteerSource::Api,
                            BackgroundMessageSource::System => SteerSource::Hook,
                        };
                        let steer_message = SteerMessage::message(queued.message.clone(), source);

                        let sent = steer_registry.steer(&task_id, steer_message).await;
                        if sent && let Err(e) = storage.mark_background_message_consumed(&queued.id)
                        {
                            warn!(
                                "Failed to mark background message {} as consumed: {}",
                                queued.id, e
                            );
                        }
                    }
                }
            }))
        } else {
            None
        };

        let resolved_input = self.resolve_task_input(&task);
        let trace_session_id = {
            let session_id = task.chat_session_id.trim();
            if session_id.is_empty() {
                task.id.clone()
            } else {
                session_id.to_string()
            }
        };
        let run_id = format!(
            "{}-{}",
            chrono::Utc::now().timestamp_millis(),
            uuid::Uuid::new_v4()
        );
        let restflow_telemetry = RestflowTrace::new(
            run_id,
            trace_session_id.clone(),
            task.id.clone(),
            task.agent_id.clone(),
        );
        let execution_trace_storage = self.execution_trace_storage();
        append_trace_event(
            execution_trace_storage,
            &TraceEvent::run_started(restflow_telemetry.clone()),
        );

        if resolved_input
            .as_deref()
            .is_none_or(|value: &str| value.trim().is_empty())
        {
            let duration_ms = chrono::Utc::now().timestamp_millis() - start_time;
            let reason = "Background task requires non-empty input or input_template";
            let error_msg = format!("Execution error: {}", reason);
            append_trace_event(
                execution_trace_storage,
                &TraceEvent::run_failed(
                    restflow_telemetry.clone(),
                    error_msg.clone(),
                    Some(duration_ms.max(0) as u64),
                ),
            );

            error!("Task '{}' failed preflight: {}", task.name, reason);
            pump_cancel.cancel();
            if let Some(pump) = message_pump.take() {
                let _ = pump.await;
            }

            self.event_emitter
                .emit(TaskStreamEvent::failed(
                    task_id,
                    &error_msg,
                    duration_ms,
                    false,
                ))
                .await;
            self.fire_hooks(&HookContext::from_failed(&task, &error_msg, duration_ms))
                .await;

            if let Err(e) =
                self.storage
                    .fail_task_execution(task_id, error_msg.clone(), duration_ms)
            {
                error!("Failed to record preflight task failure: {}", e);
            }

            self.send_notification(&task, false, &error_msg).await;
            self.clear_task_conversation_links(task_id).await;
            self.cleanup_task_tracking(task_id).await;
            return Ok(false);
        }

        let broadcast_emitter = if matches!(task.execution_mode, ExecutionMode::Api)
            && task.notification.broadcast_steps
        {
            self.channel_router
                .read()
                .await
                .as_ref()
                .cloned()
                .map(|router| {
                    Box::new(BroadcastStreamEmitter::new(task.name.clone(), router))
                        as Box<dyn StreamEmitter>
                })
        } else {
            None
        };

        let step_emitter = if matches!(task.execution_mode, ExecutionMode::Api) {
            let inner: Box<dyn StreamEmitter> = match broadcast_emitter {
                Some(emitter) => emitter,
                None => Box::new(NoopStreamEmitter),
            };
            Some(build_restflow_telemetry_emitter(
                inner,
                Some(execution_trace_storage.clone()),
                &restflow_telemetry,
            ))
        } else {
            broadcast_emitter
        };

        let execution_timeout_secs = match &task.execution_mode {
            ExecutionMode::Api => task.timeout_secs.or(self.config.task_timeout_secs),
            ExecutionMode::Cli(cli_config) => Some(cli_config.timeout_secs),
        };
        let execution_timeout_secs = if task.resource_limits.max_duration_secs > 0 {
            match execution_timeout_secs {
                Some(timeout_secs) => {
                    Some(timeout_secs.min(task.resource_limits.max_duration_secs))
                }
                None => Some(task.resource_limits.max_duration_secs),
            }
        } else {
            execution_timeout_secs
        };
        let resume_state = self.resume_states.write().await.remove(task_id);

        let exec_future = async {
            match &task.execution_mode {
                ExecutionMode::Api => {
                    // Use the injected API executor
                    debug!("Using API executor for task '{}'", task.name);
                    if let Some(state) = resume_state {
                        if let Some(timeout_secs) = execution_timeout_secs {
                            tokio::time::timeout(
                                Duration::from_secs(timeout_secs),
                                self.executor.execute_from_state(
                                    &task.agent_id,
                                    Some(&task.id),
                                    state,
                                    &task.memory,
                                    steer_rx,
                                    step_emitter,
                                ),
                            )
                            .await
                        } else {
                            Ok(self
                                .executor
                                .execute_from_state(
                                    &task.agent_id,
                                    Some(&task.id),
                                    state,
                                    &task.memory,
                                    steer_rx,
                                    step_emitter,
                                )
                                .await)
                        }
                    } else if let Some(timeout_secs) = execution_timeout_secs {
                        tokio::time::timeout(
                            Duration::from_secs(timeout_secs),
                            self.executor.execute_with_emitter(
                                &task.agent_id,
                                Some(&task.id),
                                resolved_input.as_deref(),
                                &task.memory,
                                steer_rx,
                                step_emitter,
                            ),
                        )
                        .await
                    } else {
                        Ok(self
                            .executor
                            .execute_with_emitter(
                                &task.agent_id,
                                Some(&task.id),
                                resolved_input.as_deref(),
                                &task.memory,
                                steer_rx,
                                step_emitter,
                            )
                            .await)
                    }
                }
                ExecutionMode::Cli(cli_config) => {
                    // Use CliAgentExecutor for CLI-based execution
                    use super::cli_executor::CliAgentExecutor;

                    info!(
                        "Using CLI executor for task '{}' (binary: {})",
                        task.name, cli_config.binary
                    );

                    // Create CLI executor with event streaming
                    let event_emitter = self.event_emitter.clone();
                    let task_id_for_events = task_id.to_string();

                    let cli_executor = CliAgentExecutor::with_output_callback(move |line| {
                        let event = TaskStreamEvent::output(&task_id_for_events, line, false);
                        let emitter = event_emitter.clone();
                        // Spawn a task to emit the event asynchronously
                        tokio::spawn(async move {
                            emitter.emit(event).await;
                        });
                    });

                    if let Some(timeout_secs) = execution_timeout_secs {
                        tokio::time::timeout(
                            Duration::from_secs(timeout_secs),
                            cli_executor.execute_cli(
                                cli_config,
                                resolved_input.as_deref(),
                                Some(task_id),
                            ),
                        )
                        .await
                    } else {
                        Ok(cli_executor
                            .execute_cli(cli_config, resolved_input.as_deref(), Some(task_id))
                            .await)
                    }
                }
            }
        };

        // Fail fast: refuse to run uncancellable tasks
        let stop_rx = stop_rx.ok_or_else(|| {
            error!(
                "No stop receiver found for task '{}'. Refusing to run unstoppably tracked task.",
                task_id
            );
            anyhow!("Task {} has no stop channel", task_id)
        })?;

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
                append_trace_event(
                    execution_trace_storage,
                    &TraceEvent::run_interrupted(
                        restflow_telemetry.clone(),
                        "Stopped by user",
                        Some(duration_ms.max(0) as u64),
                    ),
                );
                pump_cancel.cancel();
                if let Some(pump) = message_pump.take() {
                    let _ = pump.await;
                }
                self.event_emitter
                    .emit(TaskStreamEvent::interrupted(
                        task_id,
                        "Stopped by user",
                        duration_ms,
                    ))
                    .await;
                self.fire_hooks(&HookContext::from_interrupted(
                    &task,
                    "Stopped by user",
                    duration_ms,
                ))
                .await;
                if let Err(e) = self
                    .storage
                    .control_background_agent(task_id, crate::models::BackgroundAgentControlAction::Stop)
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
                        Ok(Some(stored_task)) if stored_task.status == BackgroundAgentStatus::Paused => {
                            return PauseSignal::Paused;
                        }
                        Ok(Some(stored_task)) if stored_task.status == BackgroundAgentStatus::Interrupted => {
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
                        append_trace_event(
                            execution_trace_storage,
                            &TraceEvent::run_interrupted(
                                restflow_telemetry.clone(),
                                "Paused by user",
                                Some(duration_ms.max(0) as u64),
                            ),
                        );
                        self.event_emitter
                            .emit(TaskStreamEvent::interrupted(
                                task_id,
                                "Paused by user",
                                duration_ms,
                            ))
                            .await;
                        self.fire_hooks(&HookContext::from_interrupted(
                            &task,
                            "Paused by user",
                            duration_ms,
                        ))
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
                        append_trace_event(
                            execution_trace_storage,
                            &TraceEvent::run_interrupted(
                                restflow_telemetry.clone(),
                                "Stopped by user",
                                Some(duration_ms.max(0) as u64),
                            ),
                        );
                        self.event_emitter
                            .emit(TaskStreamEvent::interrupted(
                                task_id,
                                "Stopped by user",
                                duration_ms,
                            ))
                            .await;
                        self.fire_hooks(&HookContext::from_interrupted(
                            &task,
                            "Stopped by user",
                            duration_ms,
                        ))
                        .await;
                        if let Err(e) = self
                            .storage
                            .control_background_agent(task_id, crate::models::BackgroundAgentControlAction::Stop)
                        {
                            error!("Failed to keep task {} interrupted: {}", task_id, e);
                        }
                    }
                    PauseSignal::Deleted => {
                        info!(
                            "Task '{}' stopped because task record was deleted (duration={}ms)",
                            task.name, duration_ms
                        );
                        append_trace_event(
                            execution_trace_storage,
                            &TraceEvent::run_interrupted(
                                restflow_telemetry.clone(),
                                "Task deleted",
                                Some(duration_ms.max(0) as u64),
                            ),
                        );
                        self.event_emitter
                            .emit(TaskStreamEvent::interrupted(
                                task_id,
                                "Task deleted",
                                duration_ms,
                            ))
                            .await;
                        self.fire_hooks(&HookContext::from_interrupted(
                            &task,
                            "Task deleted",
                            duration_ms,
                        ))
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
                append_trace_event(
                    execution_trace_storage,
                    &TraceEvent::run_completed(
                        restflow_telemetry.clone(),
                        Some(duration_ms.max(0) as u64),
                    ),
                );

                self.event_emitter
                    .emit(TaskStreamEvent::completed(
                        task_id,
                        &exec_result.output,
                        duration_ms,
                    ))
                    .await;
                self.fire_hooks(&HookContext::from_completed(
                    &task,
                    &exec_result.output,
                    duration_ms,
                ))
                .await;

                if let Err(e) = self.storage.complete_task_execution(
                    task_id,
                    Some(exec_result.output.clone()),
                    duration_ms,
                ) {
                    error!("Failed to record task completion: {}", e);
                }

                // Persist input/output to the bound chat session
                self.persist_to_chat_session(
                    &task,
                    resolved_input.as_deref(),
                    &exec_result.output,
                    false,
                    duration_ms,
                );

                if let Some(compaction) = exec_result.metrics.compaction.as_ref() {
                    let compaction_message = format!(
                        "Compacted {} messages ({} -> {} tokens) across {} event(s)",
                        compaction.messages_compacted,
                        compaction.tokens_before,
                        compaction.tokens_after,
                        compaction.event_count
                    );
                    let event = crate::models::BackgroundAgentEvent::new(
                        task.id.clone(),
                        crate::models::BackgroundAgentEventType::Compaction,
                    )
                    .with_message(compaction_message.clone());
                    if let Err(err) = self.storage.add_event(&event) {
                        warn!(
                            "Failed to record compaction event for '{}': {}",
                            task.id, err
                        );
                    }
                    self.event_emitter
                        .emit(TaskStreamEvent::progress(
                            &task.id,
                            "compaction",
                            None,
                            Some(compaction_message),
                        ))
                        .await;
                }

                // Persist conversation to long-term memory if enabled
                if task.memory.persist_on_complete {
                    self.persist_memory(&task, &exec_result.messages);
                }

                // Send notification if configured
                self.send_notification(&task, true, &exec_result.output)
                    .await;
            }
            Ok(Err(e)) => {
                // Execution error
                let error_msg = format!("Execution error: {}", e);
                error!("Task '{}' failed: {}", task.name, error_msg);
                append_trace_event(
                    execution_trace_storage,
                    &TraceEvent::run_failed(
                        restflow_telemetry.clone(),
                        error_msg.clone(),
                        Some(duration_ms.max(0) as u64),
                    ),
                );

                self.event_emitter
                    .emit(TaskStreamEvent::failed(
                        task_id,
                        &error_msg,
                        duration_ms,
                        false,
                    ))
                    .await;
                self.fire_hooks(&HookContext::from_failed(&task, &error_msg, duration_ms))
                    .await;

                if let Err(e) =
                    self.storage
                        .fail_task_execution(task_id, error_msg.clone(), duration_ms)
                {
                    error!("Failed to record task failure: {}", e);
                }

                // Persist error to the bound chat session
                self.persist_to_chat_session(
                    &task,
                    resolved_input.as_deref(),
                    &error_msg,
                    true,
                    duration_ms,
                );

                // Send failure notification
                self.send_notification(&task, false, &error_msg).await;
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
                append_trace_event(
                    execution_trace_storage,
                    &TraceEvent::run_failed(
                        restflow_telemetry.clone(),
                        error_msg.clone(),
                        Some(duration_ms.max(0) as u64),
                    ),
                );

                self.event_emitter
                    .emit(TaskStreamEvent::timeout(task_id, timeout_secs, duration_ms))
                    .await;
                self.fire_hooks(&HookContext::from_failed(&task, &error_msg, duration_ms))
                    .await;

                if let Err(e) =
                    self.storage
                        .fail_task_execution(task_id, error_msg.clone(), duration_ms)
                {
                    error!("Failed to record task timeout: {}", e);
                }

                // Persist timeout error to the bound chat session
                self.persist_to_chat_session(
                    &task,
                    resolved_input.as_deref(),
                    &error_msg,
                    true,
                    duration_ms,
                );

                // Send timeout notification
                self.send_notification(&task, false, &error_msg).await;
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

/// No-op notification sender for when notifications are not configured
pub struct NoopNotificationSender;

#[async_trait::async_trait]
impl NotificationSender for NoopNotificationSender {
    async fn send(
        &self,
        _config: &NotificationConfig,
        _task: &BackgroundAgent,
        _success: bool,
        _message: &str,
    ) -> Result<()> {
        // No-op: notifications are handled elsewhere or disabled
        Ok(())
    }

    async fn send_formatted(&self, _message: &str) -> Result<()> {
        Ok(())
    }
}

struct RunnerTaskExecutor {
    runner: Arc<BackgroundAgentRunner>,
}

#[async_trait::async_trait]
impl TaskExecutor for RunnerTaskExecutor {
    async fn execute(&self, task: &BackgroundAgent) -> Result<bool> {
        let stop_rx = self.runner.take_stop_receiver(&task.id).await;
        self.runner.execute_task(&task.id, stop_rx).await
    }
}
