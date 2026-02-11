//! Agent Task Runner - Background scheduler for agent tasks.
//!
//! The BackgroundAgentRunner is responsible for:
//! - Polling storage for runnable tasks
//! - Executing agents on schedule
//! - Handling task lifecycle (start, complete, fail)
//! - Persisting conversation memory to long-term storage
//! - Sending notifications on completion/failure

use crate::channel::{ChannelRouter, MessageLevel};
use crate::hooks::HookExecutor;
use crate::models::{
    BackgroundAgent, BackgroundAgentStatus, BackgroundMessageSource, ExecutionMode, HookContext,
    MemoryConfig, MemoryScope, NotificationConfig, SteerMessage, SteerSource,
};
use crate::performance::{
    TaskExecutor, TaskPriority, TaskQueue, TaskQueueConfig, WorkerPool, WorkerPoolConfig,
};
use crate::steer::SteerRegistry;
use crate::storage::{BackgroundAgentStorage, MemoryStorage};
use anyhow::{Result, anyhow};
use restflow_ai::llm::Message;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{RwLock, mpsc, oneshot};
use tokio::time::{Duration, Instant, interval};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use super::events::{NoopEventEmitter, TaskEventEmitter, TaskStreamEvent};
use super::persist::MemoryPersister;

use super::heartbeat::{
    HeartbeatEmitter, HeartbeatEvent, HeartbeatPulse, NoopHeartbeatEmitter, RunnerStatus,
    RunnerStatusEvent,
};

/// Result of agent execution including conversation messages.
///
/// This extended result allows the runner to persist the conversation
/// to long-term memory after task completion.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// The final output/answer from the agent
    pub output: String,
    /// All messages from the conversation (for memory persistence)
    pub messages: Vec<Message>,
    /// Whether the execution was successful
    pub success: bool,
    /// Aggregated memory compaction metrics from this execution, if any.
    pub compaction: Option<CompactionMetrics>,
}

/// Aggregated working-memory compaction metrics.
#[derive(Debug, Clone, Default)]
pub struct CompactionMetrics {
    pub event_count: u32,
    pub tokens_before: usize,
    pub tokens_after: usize,
    pub messages_compacted: usize,
}

impl ExecutionResult {
    /// Create a successful execution result.
    pub fn success(output: String, messages: Vec<Message>) -> Self {
        Self {
            output,
            messages,
            success: true,
            compaction: None,
        }
    }

    /// Create a successful execution result with compaction metrics.
    pub fn success_with_compaction(
        output: String,
        messages: Vec<Message>,
        compaction: CompactionMetrics,
    ) -> Self {
        Self {
            output,
            messages,
            success: true,
            compaction: Some(compaction),
        }
    }

    /// Create a failed execution result.
    pub fn failure(error: String) -> Self {
        Self {
            output: error,
            messages: Vec::new(),
            success: false,
            compaction: None,
        }
    }
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
    /// Cancel a running task
    CancelTask(String),
}

/// Configuration for the BackgroundAgentRunner
#[derive(Debug, Clone)]
pub struct RunnerConfig {
    /// How often to poll for runnable tasks (in milliseconds)
    pub poll_interval_ms: u64,
    /// Maximum concurrent task executions
    pub max_concurrent_tasks: usize,
    /// Timeout for individual task execution (in seconds)
    pub task_timeout_secs: u64,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            poll_interval_ms: 10_000, // 10 seconds
            max_concurrent_tasks: 5,
            task_timeout_secs: 300, // 5 minutes
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

    /// Cancel a running task
    pub async fn cancel_task(&self, task_id: String) -> Result<()> {
        self.command_tx
            .send(RunnerCommand::CancelTask(task_id))
            .await
            .map_err(|e| anyhow!("Failed to send cancel task command: {}", e))
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
    cancel_senders: Arc<RwLock<HashMap<String, oneshot::Sender<()>>>>,
    pending_cancel_receivers: Arc<RwLock<HashMap<String, oneshot::Receiver<()>>>>,
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
            cancel_senders: Arc::new(RwLock::new(HashMap::new())),
            pending_cancel_receivers: Arc::new(RwLock::new(HashMap::new())),
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
            cancel_senders: Arc::new(RwLock::new(HashMap::new())),
            pending_cancel_receivers: Arc::new(RwLock::new(HashMap::new())),
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
            cancel_senders: Arc::new(RwLock::new(HashMap::new())),
            pending_cancel_receivers: Arc::new(RwLock::new(HashMap::new())),
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

        let executor = Arc::new(RunnerTaskExecutor {
            runner: self.clone(),
        });
        let mut worker_pool = WorkerPool::new(
            self.task_queue.clone(),
            executor,
            WorkerPoolConfig {
                worker_count: self.config.max_concurrent_tasks,
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
                        Some(RunnerCommand::CancelTask(task_id)) => {
                            debug!("Cancel requested for task: {}", task_id);
                            self.cancel_task(&task_id).await;
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

    /// Check for runnable tasks and execute them
    async fn check_and_run_tasks(&self) {
        let current_time = chrono::Utc::now().timestamp_millis();

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
            // Skip if already running
            if self.running_tasks.read().await.contains(&task.id) {
                continue;
            }

            // Add to running set BEFORE enqueuing to prevent duplicates.
            let task_id = task.id.clone();
            self.running_tasks.write().await.insert(task_id.clone());
            let (cancel_tx, cancel_rx) = oneshot::channel();
            self.cancel_senders
                .write()
                .await
                .insert(task.id.clone(), cancel_tx);
            self.pending_cancel_receivers
                .write()
                .await
                .insert(task.id.clone(), cancel_rx);

            if let Err(err) = self.task_queue.submit(task, TaskPriority::Normal).await {
                warn!("Failed to enqueue task {}: {:?}", task_id, err);
                self.cleanup_task_tracking(task_id.as_str()).await;
            }
        }
    }

    /// Run a task immediately, bypassing schedule check
    async fn run_task_immediate(&self, task_id: &str) {
        // Check if already running
        if self.running_tasks.read().await.contains(task_id) {
            warn!("Task {} is already running", task_id);
            return;
        }

        // Check concurrency limit
        let running_count = self.running_tasks.read().await.len();
        if running_count >= self.config.max_concurrent_tasks {
            warn!(
                "Cannot run task {} - max concurrent tasks ({}) reached",
                task_id, self.config.max_concurrent_tasks
            );
            return;
        }

        // Verify task exists and is not paused/completed
        match self.storage.get_task(task_id) {
            Ok(Some(task)) => {
                if task.status == BackgroundAgentStatus::Paused {
                    warn!("Cannot run paused task {}", task_id);
                    return;
                }
                if task.status == BackgroundAgentStatus::Completed {
                    warn!("Cannot run completed task {}", task_id);
                    return;
                }
            }
            Ok(None) => {
                warn!("Task {} not found", task_id);
                return;
            }
            Err(e) => {
                error!("Failed to get task {}: {}", task_id, e);
                return;
            }
        }

        // Add to running set BEFORE enqueuing to prevent duplicates.
        let task_id = task_id.to_string();
        self.running_tasks.write().await.insert(task_id.clone());
        let (cancel_tx, cancel_rx) = oneshot::channel();
        self.cancel_senders
            .write()
            .await
            .insert(task_id.clone(), cancel_tx);
        self.pending_cancel_receivers
            .write()
            .await
            .insert(task_id.clone(), cancel_rx);

        let task = match self.storage.get_task(&task_id) {
            Ok(Some(task)) => task,
            Ok(None) => {
                warn!("Task {} not found", task_id);
                self.cleanup_task_tracking(&task_id).await;
                return;
            }
            Err(e) => {
                error!("Failed to load task {}: {}", task_id, e);
                self.cleanup_task_tracking(&task_id).await;
                return;
            }
        };

        if let Err(err) = self.task_queue.submit(task, TaskPriority::High).await {
            warn!("Failed to enqueue task {}: {:?}", task_id, err);
            self.cleanup_task_tracking(&task_id).await;
        }
    }

    /// Cancel a running task
    async fn cancel_task(&self, task_id: &str) {
        if !self.running_tasks.read().await.contains(task_id) {
            debug!(
                "Cancel requested for task {}, but it is not running",
                task_id
            );
        }

        let cancel_sender = self.cancel_senders.write().await.remove(task_id);
        if let Some(sender) = cancel_sender {
            if sender.send(()).is_err() {
                debug!(
                    "Cancel signal for task {} dropped (task already finished)",
                    task_id
                );
            }
            return;
        }

        // No cancel channel found; if the task is marked running in storage, pause it.
        if let Ok(Some(task)) = self.storage.get_task(task_id)
            && task.status == BackgroundAgentStatus::Running
            && let Err(e) = self.storage.pause_task(task_id)
        {
            error!("Failed to mark task {} as paused: {}", task_id, e);
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
            let steer_message = SteerMessage {
                instruction: queued.message.clone(),
                source: Self::to_steer_source(&queued.source),
                timestamp: chrono::Utc::now().timestamp_millis(),
            };

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
        mut cancel_rx: Option<oneshot::Receiver<()>>,
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
                        let steer_message = SteerMessage {
                            instruction: queued.message.clone(),
                            source,
                            timestamp: chrono::Utc::now().timestamp_millis(),
                        };

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

        let exec_future = async {
            match &task.execution_mode {
                ExecutionMode::Api => {
                    // Use the injected API executor
                    debug!("Using API executor for task '{}'", task.name);
                    let timeout = Duration::from_secs(self.config.task_timeout_secs);
                    tokio::time::timeout(
                        timeout,
                        self.executor.execute(
                            &task.agent_id,
                            Some(&task.id),
                            resolved_input.as_deref(),
                            &task.memory,
                            steer_rx,
                        ),
                    )
                    .await
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

                    // Execute with timeout
                    let timeout = Duration::from_secs(cli_config.timeout_secs);
                    tokio::time::timeout(
                        timeout,
                        cli_executor.execute_cli(cli_config, resolved_input.as_deref()),
                    )
                    .await
                }
            }
        };

        if cancel_rx.is_none() {
            warn!(
                "No cancel receiver found for task '{}'. Task will run without cancellation support.",
                task_id
            );
        }

        let result = tokio::select! {
            // Cancel branch: resolves when user sends cancel signal.
            // If no receiver exists, pending() never resolves — task runs to completion.
            _ = async {
                if let Some(rx) = &mut cancel_rx {
                    let _ = rx.await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {
                let duration_ms = chrono::Utc::now().timestamp_millis() - start_time;
                info!(
                    "Task '{}' cancelled by user (duration={}ms)",
                    task.name, duration_ms
                );
                pump_cancel.cancel();
                if let Some(pump) = message_pump.take() {
                    let _ = pump.await;
                }
                self.event_emitter
                    .emit(TaskStreamEvent::cancelled(
                        task_id,
                        "Cancelled by user",
                        duration_ms,
                    ))
                    .await;
                self.fire_hooks(&HookContext::from_cancelled(
                    &task,
                    "Cancelled by user",
                    duration_ms,
                ))
                .await;
                if let Err(e) = self.storage.pause_task(task_id) {
                    error!("Failed to mark task {} as paused: {}", task_id, e);
                }
                self.cleanup_task_tracking(task_id).await;
                return Ok(false);
            }
            // Pause branch: if control API sets task status to Paused while this
            // execution is running, stop current run immediately.
            _ = async {
                loop {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    match self.storage.get_task(task_id) {
                        Ok(Some(stored_task)) if stored_task.status == BackgroundAgentStatus::Paused => break,
                        Ok(Some(_)) => {}
                        Ok(None) => break,
                        Err(err) => {
                            warn!("Failed to read task {} while waiting for pause signal: {}", task_id, err);
                        }
                    }
                }
            } => {
                let duration_ms = chrono::Utc::now().timestamp_millis() - start_time;
                info!(
                    "Task '{}' interrupted by pause request (duration={}ms)",
                    task.name, duration_ms
                );
                pump_cancel.cancel();
                if let Some(pump) = message_pump.take() {
                    let _ = pump.await;
                }
                self.event_emitter
                    .emit(TaskStreamEvent::cancelled(
                        task_id,
                        "Paused by user",
                        duration_ms,
                    ))
                    .await;
                self.fire_hooks(&HookContext::from_cancelled(
                    &task,
                    "Paused by user",
                    duration_ms,
                ))
                .await;
                if let Err(e) = self.storage.pause_task(task_id) {
                    error!("Failed to keep task {} paused: {}", task_id, e);
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

                if let Some(compaction) = exec_result.compaction.as_ref() {
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

                // Send failure notification
                self.send_notification(&task, false, &error_msg).await;
            }
            Err(_) => {
                // Timeout
                let error_msg = format!(
                    "Task timed out after {} seconds",
                    self.config.task_timeout_secs
                );
                error!("Task '{}' timed out", task.name);

                self.event_emitter
                    .emit(TaskStreamEvent::timeout(
                        task_id,
                        self.config.task_timeout_secs,
                        duration_ms,
                    ))
                    .await;
                self.fire_hooks(&HookContext::from_failed(&task, &error_msg, duration_ms))
                    .await;

                if let Err(e) =
                    self.storage
                        .fail_task_execution(task_id, error_msg.clone(), duration_ms)
                {
                    error!("Failed to record task timeout: {}", e);
                }

                // Send timeout notification
                self.send_notification(&task, false, &error_msg).await;
            }
        }

        self.cleanup_task_tracking(task_id).await;
        Ok(success)
    }

    /// Remove a task from runner tracking maps.
    async fn cleanup_task_tracking(&self, task_id: &str) {
        self.running_tasks.write().await.remove(task_id);
        self.cancel_senders.write().await.remove(task_id);
        self.pending_cancel_receivers.write().await.remove(task_id);
        self.steer_registry.unregister(task_id).await;
    }

    /// Take the cancel receiver for a task, returning None if not found.
    /// When None, the task runs without cancellation support (uses `pending()` in select).
    async fn take_cancel_receiver(&self, task_id: &str) -> Option<oneshot::Receiver<()>> {
        self.pending_cancel_receivers.write().await.remove(task_id)
    }

    /// Persist conversation messages to long-term memory.
    ///
    /// Called after successful task execution when `persist_on_complete` is enabled.
    fn persist_memory(&self, task: &BackgroundAgent, messages: &[Message]) {
        let Some(persister) = &self.memory_persister else {
            debug!("Memory persistence not configured, skipping");
            return;
        };

        if messages.is_empty() {
            debug!("No messages to persist for task '{}'", task.name);
            return;
        }

        // Generate tags from task metadata
        // Note: BackgroundAgent doesn't have a tags field, so we use task name and agent_id
        let tags: Vec<String> = vec![
            format!("task:{}", task.id),
            format!("agent:{}", task.agent_id),
            format!(
                "memory_scope:{}",
                Self::memory_scope_label(&task.memory.memory_scope)
            ),
        ];
        let memory_agent_id = Self::resolve_memory_agent_id(task);

        match persister.persist(messages, &memory_agent_id, &task.id, &task.name, &tags) {
            Ok(result) => {
                if result.chunk_count > 0 {
                    info!(
                        "Persisted {} memory chunks for task '{}' (session: {}, namespace: {})",
                        result.chunk_count, task.name, result.session_id, memory_agent_id
                    );
                }
            }
            Err(e) => {
                warn!("Failed to persist memory for task '{}': {}", task.name, e);
            }
        }
    }

    fn resolve_task_input(&self, task: &BackgroundAgent) -> Option<String> {
        if let Some(template) = task.input_template.as_deref() {
            Some(Self::render_input_template(task, template))
        } else {
            task.input.clone()
        }
    }

    /// Single-pass template renderer that prevents double-substitution.
    /// Scans for `{{...}}` placeholders left-to-right; replacement values are
    /// emitted verbatim so any `{{` inside a value will NOT be re-expanded.
    fn render_input_template(task: &BackgroundAgent, template: &str) -> String {
        let now = chrono::Utc::now();
        // NOTE: `{{task.input}}` is preferred. `{{input}}` is kept for compatibility.
        let replacement_strings = std::collections::HashMap::from([
            ("{{task.id}}", task.id.clone()),
            ("{{task.name}}", task.name.clone()),
            ("{{task.agent_id}}", task.agent_id.clone()),
            (
                "{{task.description}}",
                task.description.clone().unwrap_or_default(),
            ),
            ("{{task.input}}", task.input.clone().unwrap_or_default()),
            ("{{input}}", task.input.clone().unwrap_or_default()),
            (
                "{{task.last_run_at}}",
                Self::format_optional_timestamp(task.last_run_at),
            ),
            (
                "{{task.next_run_at}}",
                Self::format_optional_timestamp(task.next_run_at),
            ),
            ("{{now.iso}}", now.to_rfc3339()),
            ("{{now.unix_ms}}", now.timestamp_millis().to_string()),
        ]);
        let replacements: std::collections::HashMap<&str, &str> = replacement_strings
            .iter()
            .map(|(key, value)| (*key, value.as_str()))
            .collect();
        crate::utils::template::render_template_single_pass(template, &replacements)
    }

    fn format_optional_timestamp(timestamp: Option<i64>) -> String {
        timestamp.map(|value| value.to_string()).unwrap_or_default()
    }

    fn resolve_memory_agent_id(task: &BackgroundAgent) -> String {
        match task.memory.memory_scope {
            MemoryScope::SharedAgent => task.agent_id.clone(),
            MemoryScope::PerBackgroundAgent => format!("{}::task::{}", task.agent_id, task.id),
        }
    }

    fn memory_scope_label(scope: &MemoryScope) -> &'static str {
        match scope {
            MemoryScope::SharedAgent => "shared_agent",
            MemoryScope::PerBackgroundAgent => "per_background_agent",
        }
    }

    async fn fire_hooks(&self, context: &HookContext) {
        if let Some(executor) = &self.hook_executor {
            executor.fire(context).await;
        }
    }

    /// Format a notification message for broadcasting.
    fn format_notification(task: &BackgroundAgent, success: bool, message: &str) -> String {
        if success {
            if message.trim().is_empty() {
                "Task completed successfully".to_string()
            } else {
                message.to_string()
            }
        } else if message.trim().is_empty() {
            format!("Background agent '{}' failed.", task.name)
        } else {
            format!("Background agent '{}' failed:\n{}", task.name, message)
        }
    }

    /// Send notification for task completion/failure.
    ///
    /// Prefers broadcasting through ChannelRouter when available. Falls
    /// back to the dedicated Telegram sender only when router delivery
    /// does not succeed, avoiding duplicate notifications.
    async fn send_notification(&self, task: &BackgroundAgent, success: bool, message: &str) {
        // Check if we should only notify on failure
        if success && task.notification.notify_on_failure_only {
            return;
        }

        let notification_message = Self::format_notification(task, success, message);
        let level = if success {
            MessageLevel::Plain
        } else {
            MessageLevel::Error
        };

        let mut sent_via: Vec<&'static str> = Vec::new();
        let mut failures: Vec<String> = Vec::new();

        let router_sink = ChannelRouterNotificationSink {
            router: self.channel_router.clone(),
        };
        match router_sink.send(task, level, &notification_message).await {
            Ok(NotificationDispatchStatus::Sent) => sent_via.push(router_sink.name()),
            Ok(NotificationDispatchStatus::Skipped) => {}
            Err(err) => {
                let detail = format!("{}: {}", router_sink.name(), err);
                error!(
                    task_id = %task.id,
                    sink = router_sink.name(),
                    error = %err,
                    "Failed to dispatch notification"
                );
                failures.push(detail);
            }
        }

        // Avoid duplicate sends: only try direct Telegram fallback when no
        // router delivery has succeeded.
        if sent_via.is_empty() {
            let telegram_sink = TelegramNotificationSink {
                notifier: self.notifier.clone(),
            };
            match telegram_sink.send(task, level, &notification_message).await {
                Ok(NotificationDispatchStatus::Sent) => sent_via.push(telegram_sink.name()),
                Ok(NotificationDispatchStatus::Skipped) => {}
                Err(err) => {
                    let detail = format!("{}: {}", telegram_sink.name(), err);
                    error!(
                        task_id = %task.id,
                        sink = telegram_sink.name(),
                        error = %err,
                        "Failed to dispatch notification"
                    );
                    failures.push(detail);
                }
            }
        }

        if !sent_via.is_empty() {
            let summary = format!(
                "Notification sent via [{}]: {}",
                sent_via.join(","),
                if success { "success" } else { "failure" }
            );
            if let Err(err) = self
                .storage
                .record_notification_sent(&task.id, summary.clone())
            {
                warn!("Failed to record notification sent event: {}", err);
            }
            self.event_emitter
                .emit(TaskStreamEvent::progress(
                    &task.id,
                    "notification",
                    None,
                    Some(summary),
                ))
                .await;
            return;
        }

        if !failures.is_empty() {
            let detail = failures.join(" | ");
            if let Err(err) = self
                .storage
                .record_notification_failed(&task.id, detail.clone())
            {
                warn!("Failed to record notification failure event: {}", err);
            }
            self.event_emitter
                .emit(TaskStreamEvent::progress(
                    &task.id,
                    "notification_failed",
                    None,
                    Some(detail),
                ))
                .await;
            return;
        }

        self.event_emitter
            .emit(TaskStreamEvent::progress(
                &task.id,
                "notification_skipped",
                None,
                Some("No enabled notification sinks".to_string()),
            ))
            .await;
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
        let cancel_rx = self.runner.take_cancel_receiver(&task.id).await;
        self.runner.execute_task(&task.id, cancel_rx).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::{HookExecutor, HookTaskScheduler};
    use crate::models::{Hook, HookAction, HookEvent, MemoryScope, TaskEventType, TaskSchedule};
    use crate::runtime::background_agent::{ChannelEventEmitter, StreamEventKind};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Instant;
    use tempfile::tempdir;

    /// Mock executor for testing
    struct MockExecutor {
        call_count: AtomicU32,
        should_fail: bool,
        delay_ms: u64,
    }

    impl MockExecutor {
        fn new() -> Self {
            Self {
                call_count: AtomicU32::new(0),
                should_fail: false,
                delay_ms: 0,
            }
        }

        fn with_failure() -> Self {
            Self {
                call_count: AtomicU32::new(0),
                should_fail: true,
                delay_ms: 0,
            }
        }

        fn with_delay(delay_ms: u64) -> Self {
            Self {
                call_count: AtomicU32::new(0),
                should_fail: false,
                delay_ms,
            }
        }

        fn call_count(&self) -> u32 {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl AgentExecutor for MockExecutor {
        async fn execute(
            &self,
            agent_id: &str,
            _background_task_id: Option<&str>,
            input: Option<&str>,
            _memory_config: &MemoryConfig,
            _steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        ) -> Result<ExecutionResult> {
            self.call_count.fetch_add(1, Ordering::SeqCst);

            if self.delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
            }

            if self.should_fail {
                Err(anyhow!("Mock execution failure"))
            } else {
                let output = format!("Executed agent {} with input: {:?}", agent_id, input);
                // Return empty messages for mock - real executor would return actual conversation
                Ok(ExecutionResult::success(output, Vec::new()))
            }
        }
    }

    /// Mock notifier for testing
    struct MockNotifier {
        notifications: Arc<RwLock<Vec<(String, bool, String)>>>,
    }

    impl MockNotifier {
        fn new() -> Self {
            Self {
                notifications: Arc::new(RwLock::new(Vec::new())),
            }
        }

        async fn notification_count(&self) -> usize {
            self.notifications.read().await.len()
        }

        async fn last_message(&self) -> Option<String> {
            self.notifications
                .read()
                .await
                .last()
                .map(|(_, _, message)| message.clone())
        }
    }

    #[async_trait::async_trait]
    impl NotificationSender for MockNotifier {
        async fn send(
            &self,
            _config: &NotificationConfig,
            task: &BackgroundAgent,
            success: bool,
            message: &str,
        ) -> Result<()> {
            self.notifications
                .write()
                .await
                .push((task.id.clone(), success, message.to_string()));
            Ok(())
        }

        async fn send_formatted(&self, message: &str) -> Result<()> {
            self.notifications.write().await.push((
                "formatted".to_string(),
                true,
                message.to_string(),
            ));
            Ok(())
        }
    }

    struct MockHookScheduler {
        call_count: AtomicU32,
    }

    impl MockHookScheduler {
        fn new() -> Self {
            Self {
                call_count: AtomicU32::new(0),
            }
        }

        fn call_count(&self) -> u32 {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl HookTaskScheduler for MockHookScheduler {
        async fn schedule_task(&self, _agent_id: &str, _input: &str) -> Result<()> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    /// Creates test storage and returns it along with the TempDir.
    /// The TempDir must be kept alive for the duration of the test to prevent
    /// the database from being deleted (important on Windows).
    fn create_test_storage() -> (Arc<BackgroundAgentStorage>, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(redb::Database::create(db_path).unwrap());
        (Arc::new(BackgroundAgentStorage::new(db).unwrap()), temp_dir)
    }

    #[tokio::test]
    async fn test_take_cancel_receiver_returns_none_when_missing() {
        let (storage, _temp_dir) = create_test_storage();
        let runner = BackgroundAgentRunner::new(
            storage,
            Arc::new(MockExecutor::new()),
            Arc::new(NoopNotificationSender),
            RunnerConfig::default(),
            Arc::new(SteerRegistry::new()),
        );

        let result = runner.take_cancel_receiver("nonexistent-task").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_take_cancel_receiver_returns_receiver_when_present() {
        let (storage, _temp_dir) = create_test_storage();
        let runner = BackgroundAgentRunner::new(
            storage,
            Arc::new(MockExecutor::new()),
            Arc::new(NoopNotificationSender),
            RunnerConfig::default(),
            Arc::new(SteerRegistry::new()),
        );

        let (tx, rx) = oneshot::channel();
        runner
            .pending_cancel_receivers
            .write()
            .await
            .insert("task-1".to_string(), rx);

        let mut result = runner.take_cancel_receiver("task-1").await;
        assert!(result.is_some());
        assert!(
            !runner
                .pending_cancel_receivers
                .read()
                .await
                .contains_key("task-1")
        );

        tx.send(()).unwrap();
        assert!(result.as_mut().unwrap().await.is_ok());
    }

    #[tokio::test]
    async fn test_runner_start_stop() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = Arc::new(MockExecutor::new());
        let notifier = Arc::new(NoopNotificationSender);

        let config = RunnerConfig {
            poll_interval_ms: 100,
            ..Default::default()
        };

        let steer_registry = Arc::new(SteerRegistry::new());
        let runner = Arc::new(BackgroundAgentRunner::new(
            storage,
            executor,
            notifier,
            config,
            steer_registry,
        ));

        let handle = runner.clone().start();

        // Let it run for a bit
        tokio::time::sleep(Duration::from_millis(250)).await;

        // Stop it
        handle.stop().await.unwrap();

        // Give it time to stop
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    #[tokio::test]
    async fn test_runner_executes_runnable_task() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = Arc::new(MockExecutor::new());
        let notifier = Arc::new(MockNotifier::new());

        // Create a task that should run immediately
        let past_time = chrono::Utc::now().timestamp_millis() - 1000;
        let mut task = storage
            .create_task(
                "Test Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Once { run_at: past_time },
            )
            .unwrap();

        // Update next_run_at to be in the past
        task.next_run_at = Some(past_time);
        storage.update_task(&task).unwrap();

        let config = RunnerConfig {
            poll_interval_ms: 100,
            ..Default::default()
        };

        let steer_registry = Arc::new(SteerRegistry::new());
        let runner = Arc::new(BackgroundAgentRunner::new(
            storage.clone(),
            executor.clone(),
            notifier,
            config,
            steer_registry,
        ));

        let handle = runner.clone().start();

        // Wait for execution
        tokio::time::sleep(Duration::from_millis(500)).await;

        handle.stop().await.unwrap();

        // Verify execution happened
        assert_eq!(executor.call_count(), 1);

        // Verify task status updated
        let updated_task = storage.get_task(&task.id).unwrap().unwrap();
        assert_eq!(updated_task.status, BackgroundAgentStatus::Completed);
        assert_eq!(updated_task.success_count, 1);
    }

    #[tokio::test]
    async fn test_runner_emits_stream_events_with_custom_emitter() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = Arc::new(MockExecutor::new());
        let notifier = Arc::new(NoopNotificationSender);
        let (channel_emitter, mut event_rx) = ChannelEventEmitter::new();

        let past_time = chrono::Utc::now().timestamp_millis() - 1000;
        let mut task = storage
            .create_task(
                "Event Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Once { run_at: past_time },
            )
            .unwrap();
        task.next_run_at = Some(past_time);
        storage.update_task(&task).unwrap();

        let config = RunnerConfig {
            poll_interval_ms: 100,
            ..Default::default()
        };

        let steer_registry = Arc::new(SteerRegistry::new());
        let runner = Arc::new(
            BackgroundAgentRunner::new(storage, executor, notifier, config, steer_registry)
                .with_event_emitter(Arc::new(channel_emitter)),
        );

        let handle = runner.clone().start();
        tokio::time::sleep(Duration::from_millis(500)).await;
        handle.stop().await.unwrap();

        let mut started_seen = false;
        let mut completed_seen = false;
        while let Ok(event) = event_rx.try_recv() {
            if let StreamEventKind::Started { .. } = event.kind {
                started_seen = true;
            }
            if let StreamEventKind::Completed { .. } = event.kind {
                completed_seen = true;
            }
        }

        assert!(started_seen);
        assert!(completed_seen);
    }

    #[tokio::test]
    async fn test_runner_triggers_hooks_on_completion() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = Arc::new(MockExecutor::new());
        let notifier = Arc::new(NoopNotificationSender);
        let hook_scheduler = Arc::new(MockHookScheduler::new());

        let hook = Hook::new(
            "Run follow-up".to_string(),
            HookEvent::TaskCompleted,
            HookAction::RunTask {
                agent_id: "agent-next".to_string(),
                input_template: "From hook".to_string(),
            },
        );
        let hook_executor =
            Arc::new(HookExecutor::new(vec![hook]).with_task_scheduler(hook_scheduler.clone()));

        let past_time = chrono::Utc::now().timestamp_millis() - 1000;
        let mut task = storage
            .create_task(
                "Task With Hook".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Once { run_at: past_time },
            )
            .unwrap();
        task.next_run_at = Some(past_time);
        storage.update_task(&task).unwrap();

        let config = RunnerConfig {
            poll_interval_ms: 100,
            ..Default::default()
        };

        let steer_registry = Arc::new(SteerRegistry::new());
        let runner = Arc::new(
            BackgroundAgentRunner::new(storage, executor, notifier, config, steer_registry)
                .with_hook_executor(hook_executor),
        );

        let handle = runner.clone().start();
        tokio::time::sleep(Duration::from_millis(500)).await;
        handle.stop().await.unwrap();

        assert_eq!(hook_scheduler.call_count(), 1);
    }

    #[tokio::test]
    async fn test_runner_handles_failure() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = Arc::new(MockExecutor::with_failure());
        let notifier = Arc::new(MockNotifier::new());

        // Create a task that should run immediately
        let past_time = chrono::Utc::now().timestamp_millis() - 1000;
        let mut task = storage
            .create_task(
                "Failing Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Once { run_at: past_time },
            )
            .unwrap();

        task.next_run_at = Some(past_time);
        storage.update_task(&task).unwrap();

        let config = RunnerConfig {
            poll_interval_ms: 100,
            ..Default::default()
        };

        let steer_registry = Arc::new(SteerRegistry::new());
        let runner = Arc::new(BackgroundAgentRunner::new(
            storage.clone(),
            executor,
            notifier.clone(),
            config,
            steer_registry,
        ));

        let handle = runner.clone().start();

        tokio::time::sleep(Duration::from_millis(500)).await;

        handle.stop().await.unwrap();

        // Verify task failed
        let updated_task = storage.get_task(&task.id).unwrap().unwrap();
        assert_eq!(updated_task.status, BackgroundAgentStatus::Failed);
        assert_eq!(updated_task.failure_count, 1);
        assert!(updated_task.last_error.is_some());
    }

    #[tokio::test]
    async fn test_runner_respects_concurrency_limit() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = Arc::new(MockExecutor::with_delay(500)); // 500ms delay
        let notifier = Arc::new(NoopNotificationSender);

        // Create multiple tasks
        let past_time = chrono::Utc::now().timestamp_millis() - 1000;
        for i in 0..5 {
            let mut task = storage
                .create_task(
                    format!("Task {}", i),
                    "agent-001".to_string(),
                    TaskSchedule::Once { run_at: past_time },
                )
                .unwrap();
            task.next_run_at = Some(past_time);
            storage.update_task(&task).unwrap();
        }

        let config = RunnerConfig {
            poll_interval_ms: 50,
            max_concurrent_tasks: 2, // Only 2 at a time
            ..Default::default()
        };

        let steer_registry = Arc::new(SteerRegistry::new());
        let runner = Arc::new(BackgroundAgentRunner::new(
            storage,
            executor.clone(),
            notifier,
            config,
            steer_registry,
        ));

        let handle = runner.clone().start();

        // Check running count shortly after start
        tokio::time::sleep(Duration::from_millis(100)).await;
        let running = runner.running_task_count().await;
        assert!(running <= 2, "Should respect concurrency limit");

        // Wait for all to complete (5 tasks * 500ms each / 2 concurrent = 1250ms min)
        // Use a retry loop to reduce timing flakes on Windows CI.
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if executor.call_count() >= 5 {
                break;
            }
            if Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        handle.stop().await.unwrap();

        // All tasks should have run eventually
        assert_eq!(executor.call_count(), 5);
    }

    #[tokio::test]
    async fn test_runner_check_now() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = Arc::new(MockExecutor::new());
        let notifier = Arc::new(NoopNotificationSender);

        let config = RunnerConfig {
            poll_interval_ms: 10000, // Very long poll interval
            ..Default::default()
        };

        let steer_registry = Arc::new(SteerRegistry::new());
        let runner = Arc::new(BackgroundAgentRunner::new(
            storage.clone(),
            executor.clone(),
            notifier,
            config,
            steer_registry,
        ));

        let handle = runner.clone().start();

        // Create a runnable task
        let past_time = chrono::Utc::now().timestamp_millis() - 1000;
        let mut task = storage
            .create_task(
                "Test Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Once { run_at: past_time },
            )
            .unwrap();
        task.next_run_at = Some(past_time);
        storage.update_task(&task).unwrap();

        // Without check_now, it wouldn't run for 10 seconds
        // Trigger immediate check
        handle.check_now().await.unwrap();

        tokio::time::sleep(Duration::from_millis(300)).await;

        handle.stop().await.unwrap();

        // Should have executed despite long poll interval
        assert_eq!(executor.call_count(), 1);
    }

    #[tokio::test]
    async fn test_runner_run_task_now() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = Arc::new(MockExecutor::new());
        let notifier = Arc::new(NoopNotificationSender);

        let config = RunnerConfig {
            poll_interval_ms: 10000, // Very long poll interval
            ..Default::default()
        };

        let steer_registry = Arc::new(SteerRegistry::new());
        let runner = Arc::new(BackgroundAgentRunner::new(
            storage.clone(),
            executor.clone(),
            notifier,
            config,
            steer_registry,
        ));

        let handle = runner.clone().start();

        // Create a task with future run time (shouldn't run automatically)
        let future_time = chrono::Utc::now().timestamp_millis() + 3600000;
        let task = storage
            .create_task(
                "Future Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Once {
                    run_at: future_time,
                },
            )
            .unwrap();

        // Run it immediately
        handle.run_task_now(task.id.clone()).await.unwrap();

        tokio::time::sleep(Duration::from_millis(300)).await;

        handle.stop().await.unwrap();

        // Should have executed despite future schedule
        assert_eq!(executor.call_count(), 1);
    }

    #[tokio::test]
    async fn test_runner_uses_input_template_when_running_task() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = Arc::new(MockExecutor::new());
        let notifier = Arc::new(NoopNotificationSender);

        let config = RunnerConfig {
            poll_interval_ms: 10_000,
            ..Default::default()
        };

        let steer_registry = Arc::new(SteerRegistry::new());
        let runner = Arc::new(BackgroundAgentRunner::new(
            storage.clone(),
            executor.clone(),
            notifier,
            config,
            steer_registry,
        ));

        let handle = runner.clone().start();

        let future_time = chrono::Utc::now().timestamp_millis() + 3_600_000;
        let mut task = storage
            .create_task(
                "Template Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Once {
                    run_at: future_time,
                },
            )
            .unwrap();
        task.input = Some("fallback input".to_string());
        task.input_template = Some("Task {{task.id}} for {{task.name}}".to_string());
        storage.update_task(&task).unwrap();

        handle.run_task_now(task.id.clone()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        handle.stop().await.unwrap();

        assert_eq!(executor.call_count(), 1);

        let events = storage.list_events_for_task(&task.id).unwrap();
        let completed = events
            .iter()
            .find(|event| event.event_type == TaskEventType::Completed)
            .and_then(|event| event.output.as_deref())
            .unwrap_or_default()
            .to_string();

        assert!(completed.contains(&format!("Task {} for Template Task", task.id)));
        assert!(!completed.contains("fallback input"));
    }

    #[tokio::test]
    async fn test_runner_skips_paused_tasks() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = Arc::new(MockExecutor::new());
        let notifier = Arc::new(NoopNotificationSender);

        // Create a runnable task and pause it
        let past_time = chrono::Utc::now().timestamp_millis() - 1000;
        let mut task = storage
            .create_task(
                "Paused Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Once { run_at: past_time },
            )
            .unwrap();
        task.next_run_at = Some(past_time);
        storage.update_task(&task).unwrap();
        storage.pause_task(&task.id).unwrap();

        let config = RunnerConfig {
            poll_interval_ms: 100,
            ..Default::default()
        };

        let steer_registry = Arc::new(SteerRegistry::new());
        let runner = Arc::new(BackgroundAgentRunner::new(
            storage,
            executor.clone(),
            notifier,
            config,
            steer_registry,
        ));

        let handle = runner.clone().start();

        tokio::time::sleep(Duration::from_millis(300)).await;

        handle.stop().await.unwrap();

        // Should not have executed paused task
        assert_eq!(executor.call_count(), 0);
    }

    #[test]
    fn test_render_input_template_replaces_known_placeholders() {
        let mut task = BackgroundAgent::new(
            "task-123".to_string(),
            "Template Unit Test".to_string(),
            "agent-456".to_string(),
            TaskSchedule::default(),
        );
        task.description = Some("description".to_string());
        task.input = Some("input".to_string());

        let rendered = BackgroundAgentRunner::render_input_template(
            &task,
            "ID={{task.id}}, NAME={{task.name}}, AGENT={{task.agent_id}}, DESC={{task.description}}, INPUT={{task.input}}, NOW={{now.unix_ms}}",
        );

        assert!(rendered.contains("ID=task-123"));
        assert!(rendered.contains("NAME=Template Unit Test"));
        assert!(rendered.contains("AGENT=agent-456"));
        assert!(rendered.contains("DESC=description"));
        assert!(rendered.contains("INPUT=input"));
        assert!(!rendered.contains("{{now.unix_ms}}"));
    }

    #[test]
    fn test_render_input_template_supports_input_alias() {
        let mut task = BackgroundAgent::new(
            "task-123".to_string(),
            "Template Alias Test".to_string(),
            "agent-456".to_string(),
            TaskSchedule::default(),
        );
        task.input = Some("alias-input".to_string());

        let rendered = BackgroundAgentRunner::render_input_template(&task, "INPUT={{input}}");

        assert_eq!(rendered, "INPUT=alias-input");
    }

    #[test]
    fn test_render_input_template_input_alias_matches_task_input() {
        let mut task = BackgroundAgent::new(
            "task-123".to_string(),
            "Template Unit Test".to_string(),
            "agent-456".to_string(),
            TaskSchedule::default(),
        );
        task.input = Some("input".to_string());

        let rendered = BackgroundAgentRunner::render_input_template(
            &task,
            "ALIAS={{input}}, REQUIRED={{task.input}}",
        );

        // Both {{input}} and {{task.input}} should resolve to the same value
        assert!(rendered.contains("ALIAS=input"));
        assert!(rendered.contains("REQUIRED=input"));
    }

    #[test]
    fn test_render_input_template_no_double_substitution() {
        let task = BackgroundAgent::new(
            "task-123".to_string(),
            "Process {{task.id}}".to_string(), // name contains a placeholder
            "agent-456".to_string(),
            TaskSchedule::default(),
        );

        let rendered = BackgroundAgentRunner::render_input_template(
            &task,
            "Name: {{task.name}}, ID: {{task.id}}",
        );

        // Name should be literal "Process {{task.id}}", NOT "Process task-123"
        assert_eq!(rendered, "Name: Process {{task.id}}, ID: task-123");
    }

    #[test]
    fn test_resolve_memory_agent_id_respects_scope() {
        let mut task = BackgroundAgent::new(
            "task-123".to_string(),
            "Memory Scope Test".to_string(),
            "agent-456".to_string(),
            TaskSchedule::default(),
        );

        task.memory.memory_scope = MemoryScope::SharedAgent;
        assert_eq!(
            BackgroundAgentRunner::resolve_memory_agent_id(&task),
            "agent-456"
        );

        task.memory.memory_scope = MemoryScope::PerBackgroundAgent;
        assert_eq!(
            BackgroundAgentRunner::resolve_memory_agent_id(&task),
            "agent-456::task::task-123"
        );
    }

    #[test]
    fn test_resolve_task_input_keeps_plain_input_unchanged() {
        let (storage, _temp_dir) = create_test_storage();
        let mut task = BackgroundAgent::new(
            "task-plain".to_string(),
            "Plain Input Task".to_string(),
            "agent-456".to_string(),
            TaskSchedule::default(),
        );
        task.input = Some("Collect latest news.".to_string());

        let runner = BackgroundAgentRunner::new(
            storage,
            Arc::new(MockExecutor::new()),
            Arc::new(NoopNotificationSender),
            RunnerConfig::default(),
            Arc::new(SteerRegistry::new()),
        );

        let resolved = runner
            .resolve_task_input(&task)
            .expect("resolved input should exist");

        assert_eq!(resolved, "Collect latest news.");
    }

    #[test]
    fn test_resolve_task_input_renders_template_without_injection() {
        let (storage, _temp_dir) = create_test_storage();
        let mut task = BackgroundAgent::new(
            "task-template".to_string(),
            "Template Input Task".to_string(),
            "agent-789".to_string(),
            TaskSchedule::default(),
        );
        task.input = Some("fallback".to_string());
        task.input_template = Some("Template for {{task.name}}".to_string());

        let runner = BackgroundAgentRunner::new(
            storage,
            Arc::new(MockExecutor::new()),
            Arc::new(NoopNotificationSender),
            RunnerConfig::default(),
            Arc::new(SteerRegistry::new()),
        );

        let resolved = runner
            .resolve_task_input(&task)
            .expect("resolved input should exist");

        assert_eq!(resolved, "Template for Template Input Task");
    }

    #[test]
    fn test_resolve_task_input_returns_none_when_no_input_or_template() {
        let (storage, _temp_dir) = create_test_storage();
        let task = BackgroundAgent::new(
            "task-empty".to_string(),
            "Empty Input Task".to_string(),
            "agent-000".to_string(),
            TaskSchedule::default(),
        );

        let runner = BackgroundAgentRunner::new(
            storage,
            Arc::new(MockExecutor::new()),
            Arc::new(NoopNotificationSender),
            RunnerConfig::default(),
            Arc::new(SteerRegistry::new()),
        );

        assert!(runner.resolve_task_input(&task).is_none());
    }

    #[tokio::test]
    async fn test_runner_sends_notifications() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = Arc::new(MockExecutor::new());
        let notifier = Arc::new(MockNotifier::new());

        // Create a task with notifications enabled
        let past_time = chrono::Utc::now().timestamp_millis() - 1000;
        let mut task = storage
            .create_task(
                "Notified Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Once { run_at: past_time },
            )
            .unwrap();
        task.next_run_at = Some(past_time);
        storage.update_task(&task).unwrap();

        let config = RunnerConfig {
            poll_interval_ms: 100,
            ..Default::default()
        };

        let steer_registry = Arc::new(SteerRegistry::new());
        let runner = Arc::new(BackgroundAgentRunner::new(
            storage,
            executor,
            notifier.clone(),
            config,
            steer_registry,
        ));

        let handle = runner.clone().start();

        tokio::time::sleep(Duration::from_millis(500)).await;

        handle.stop().await.unwrap();

        // Should have sent notification
        assert_eq!(notifier.notification_count().await, 1);
    }

    #[tokio::test]
    async fn test_runner_notify_on_failure_only() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = Arc::new(MockExecutor::new()); // This succeeds
        let notifier = Arc::new(MockNotifier::new());

        // Create a task with notify_on_failure_only
        let past_time = chrono::Utc::now().timestamp_millis() - 1000;
        let mut task = storage
            .create_task(
                "Success No Notify".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Once { run_at: past_time },
            )
            .unwrap();
        task.next_run_at = Some(past_time);
        task.notification.notify_on_failure_only = true;
        storage.update_task(&task).unwrap();

        let config = RunnerConfig {
            poll_interval_ms: 100,
            ..Default::default()
        };

        let steer_registry = Arc::new(SteerRegistry::new());
        let runner = Arc::new(BackgroundAgentRunner::new(
            storage,
            executor,
            notifier.clone(),
            config,
            steer_registry,
        ));

        let handle = runner.clone().start();

        tokio::time::sleep(Duration::from_millis(500)).await;

        handle.stop().await.unwrap();

        // Should NOT have sent notification (success with notify_on_failure_only)
        assert_eq!(notifier.notification_count().await, 0);
    }

    #[tokio::test]
    async fn test_runner_success_notification_uses_agent_output_even_when_include_output_disabled()
    {
        let (storage, _temp_dir) = create_test_storage();
        let executor = Arc::new(MockExecutor::new());
        let notifier = Arc::new(MockNotifier::new());

        let past_time = chrono::Utc::now().timestamp_millis() - 1000;
        let mut task = storage
            .create_task(
                "Success Output".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Once { run_at: past_time },
            )
            .unwrap();
        task.next_run_at = Some(past_time);
        task.notification.include_output = false;
        storage.update_task(&task).unwrap();

        let config = RunnerConfig {
            poll_interval_ms: 100,
            ..Default::default()
        };

        let steer_registry = Arc::new(SteerRegistry::new());
        let runner = Arc::new(BackgroundAgentRunner::new(
            storage,
            executor,
            notifier.clone(),
            config,
            steer_registry,
        ));

        let handle = runner.clone().start();

        tokio::time::sleep(Duration::from_millis(500)).await;

        handle.stop().await.unwrap();

        assert_eq!(notifier.notification_count().await, 1);
        let message = notifier.last_message().await.unwrap_or_default();
        assert!(message.contains("Executed agent agent-001"));
        assert!(!message.contains("Task completed successfully"));
    }

    #[tokio::test]
    async fn test_runner_failure_notification_includes_error_when_include_output_disabled() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = Arc::new(MockExecutor::with_failure());
        let notifier = Arc::new(MockNotifier::new());

        let past_time = chrono::Utc::now().timestamp_millis() - 1000;
        let mut task = storage
            .create_task(
                "Failure Output".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Once { run_at: past_time },
            )
            .unwrap();
        task.next_run_at = Some(past_time);
        task.notification.include_output = false;
        storage.update_task(&task).unwrap();

        let config = RunnerConfig {
            poll_interval_ms: 100,
            ..Default::default()
        };

        let steer_registry = Arc::new(SteerRegistry::new());
        let runner = Arc::new(BackgroundAgentRunner::new(
            storage,
            executor,
            notifier.clone(),
            config,
            steer_registry,
        ));

        let handle = runner.clone().start();
        tokio::time::sleep(Duration::from_millis(500)).await;
        handle.stop().await.unwrap();

        assert_eq!(notifier.notification_count().await, 1);
        let message = notifier.last_message().await.unwrap_or_default();
        assert!(message.contains("Execution error: Mock execution failure"));
        assert_ne!(message, "Task failed");
    }

    #[test]
    fn test_format_notification_success_returns_raw_message() {
        let task = BackgroundAgent::new(
            "task-1".to_string(),
            "Demo Task".to_string(),
            "agent-1".to_string(),
            TaskSchedule::Interval {
                interval_ms: 60_000,
                start_at: None,
            },
        );

        let output = "hello\nworld";
        let formatted = BackgroundAgentRunner::format_notification(&task, true, output);
        assert_eq!(formatted, output);
    }

    #[test]
    fn test_format_notification_failure_keeps_context() {
        let task = BackgroundAgent::new(
            "task-1".to_string(),
            "Demo Task".to_string(),
            "agent-1".to_string(),
            TaskSchedule::Interval {
                interval_ms: 60_000,
                start_at: None,
            },
        );

        let formatted = BackgroundAgentRunner::format_notification(&task, false, "boom");
        assert!(formatted.contains("Demo Task"));
        assert!(formatted.contains("boom"));
    }

    #[tokio::test]
    async fn test_running_task_tracking() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = Arc::new(MockExecutor::with_delay(500));
        let notifier = Arc::new(NoopNotificationSender);

        let past_time = chrono::Utc::now().timestamp_millis() - 1000;
        let mut task = storage
            .create_task(
                "Slow Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Once { run_at: past_time },
            )
            .unwrap();
        task.next_run_at = Some(past_time);
        storage.update_task(&task).unwrap();

        let config = RunnerConfig {
            poll_interval_ms: 100,
            ..Default::default()
        };

        let steer_registry = Arc::new(SteerRegistry::new());
        let runner = Arc::new(BackgroundAgentRunner::new(
            storage,
            executor,
            notifier,
            config,
            steer_registry,
        ));

        let handle = runner.clone().start();

        // Wait for task to start (allow extra time for Windows CI)
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Should show as running
        let running_ids = runner.running_task_ids().await;
        assert_eq!(running_ids.len(), 1);
        assert_eq!(running_ids[0], task.id);

        // Wait for completion (500ms task + generous buffer for Windows CI)
        tokio::time::sleep(Duration::from_millis(1500)).await;

        // Should no longer be running
        assert_eq!(runner.running_task_count().await, 0);

        handle.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_runner_interrupts_running_task_when_paused() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = Arc::new(MockExecutor::with_delay(3_000));
        let notifier = Arc::new(NoopNotificationSender);

        let past_time = chrono::Utc::now().timestamp_millis() - 1000;
        let mut task = storage
            .create_task(
                "Pause Interrupt Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Once { run_at: past_time },
            )
            .unwrap();
        task.next_run_at = Some(past_time);
        storage.update_task(&task).unwrap();

        let config = RunnerConfig {
            poll_interval_ms: 100,
            ..Default::default()
        };

        let steer_registry = Arc::new(SteerRegistry::new());
        let runner = Arc::new(BackgroundAgentRunner::new(
            storage.clone(),
            executor,
            notifier,
            config,
            steer_registry,
        ));

        let handle = runner.clone().start();

        // Wait for task to start running
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert_eq!(runner.running_task_count().await, 1);

        // Simulate pause control while the task is running
        storage.pause_task(&task.id).unwrap();

        // Runner should notice pause and stop execution early
        tokio::time::sleep(Duration::from_millis(700)).await;
        assert_eq!(runner.running_task_count().await, 0);

        let updated_task = storage.get_task(&task.id).unwrap().unwrap();
        assert_eq!(updated_task.status, BackgroundAgentStatus::Paused);

        handle.stop().await.unwrap();
    }
}
