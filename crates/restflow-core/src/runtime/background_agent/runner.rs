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
    BackgroundAgent, BackgroundAgentRun, BackgroundAgentStatus, BackgroundMessageSource,
    ExecutionMode, HookContext, MemoryConfig, MemoryScope, NotificationConfig, SteerMessage,
    SteerSource,
};
use crate::performance::{
    TaskExecutor, TaskPriority, TaskQueue, TaskQueueConfig, WorkerPool, WorkerPoolConfig,
};
use crate::runtime::output::{ensure_success_output, format_error_output};
use crate::steer::SteerRegistry;
use crate::storage::{BackgroundAgentStorage, MemoryStorage};
use anyhow::{Result, anyhow};
use restflow_ai::agent::StreamEmitter;
use restflow_telemetry::{RunDescriptor, RunKind, RunLifecycleService};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
#[cfg(test)]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{RwLock, mpsc, oneshot};
use tokio::time::{Duration, Instant, interval};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use super::broadcast_emitter::BroadcastStreamEmitter;
use super::events::{NoopEventEmitter, TaskEventEmitter, TaskStreamEvent};
use super::persist::MemoryPersister;
use restflow_traits::{
    DEFAULT_BACKGROUND_RUNNER_MAX_CONCURRENT_TASKS, DEFAULT_BACKGROUND_RUNNER_POLL_INTERVAL_MS,
};

use super::heartbeat::{
    HeartbeatEmitter, HeartbeatEvent, HeartbeatPulse, NoopHeartbeatEmitter, RunnerStatus,
    RunnerStatusEvent,
};
use super::outcome::ExecutionOutcome;
use finalizer::BackgroundRunFinalizer;
mod finalizer;
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

    /// Execute an agent with an emitter and an explicit telemetry context.
    ///
    /// Background task execution should prefer this method so the runner can
    /// provide the authoritative top-level run identity for the current
    /// execution attempt.
    #[allow(clippy::too_many_arguments)]
    async fn execute_with_emitter_and_telemetry(
        &self,
        agent_id: &str,
        background_task_id: Option<&str>,
        input: Option<&str>,
        memory_config: &MemoryConfig,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
        telemetry_context: Option<restflow_telemetry::TelemetryContext>,
    ) -> Result<ExecutionResult> {
        let _ = telemetry_context;
        self.execute_with_emitter(
            agent_id,
            background_task_id,
            input,
            memory_config,
            steer_rx,
            emitter,
        )
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

    /// Execute an agent from a previously persisted state with an explicit
    /// telemetry context supplied by the runner.
    #[allow(clippy::too_many_arguments)]
    async fn execute_from_state_with_emitter_and_telemetry(
        &self,
        agent_id: &str,
        background_task_id: Option<&str>,
        state: restflow_ai::AgentState,
        memory_config: &MemoryConfig,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
        telemetry_context: Option<restflow_telemetry::TelemetryContext>,
    ) -> Result<ExecutionResult> {
        let _ = telemetry_context;
        self.execute_from_state(
            agent_id,
            background_task_id,
            state,
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
    resume_checkpoint_ids: Arc<RwLock<HashMap<String, String>>>,
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
    #[cfg(test)]
    fail_start_task_run_once: Arc<AtomicBool>,
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
            resume_checkpoint_ids: Arc::new(RwLock::new(HashMap::new())),
            task_queue,
            heartbeat_emitter: Arc::new(NoopHeartbeatEmitter),
            event_emitter: Arc::new(NoopEventEmitter),
            sequence: AtomicU64::new(0),
            start_time: Instant::now(),
            memory_persister: None,
            hook_executor: None,
            steer_registry,
            channel_router: Arc::new(RwLock::new(None)),
            #[cfg(test)]
            fail_start_task_run_once: Arc::new(AtomicBool::new(false)),
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
            resume_checkpoint_ids: Arc::new(RwLock::new(HashMap::new())),
            task_queue,
            heartbeat_emitter,
            event_emitter: Arc::new(NoopEventEmitter),
            sequence: AtomicU64::new(0),
            start_time: Instant::now(),
            memory_persister: None,
            hook_executor: None,
            steer_registry,
            channel_router: Arc::new(RwLock::new(None)),
            #[cfg(test)]
            fail_start_task_run_once: Arc::new(AtomicBool::new(false)),
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
            resume_checkpoint_ids: Arc::new(RwLock::new(HashMap::new())),
            task_queue,
            heartbeat_emitter,
            event_emitter: Arc::new(NoopEventEmitter),
            sequence: AtomicU64::new(0),
            start_time: Instant::now(),
            memory_persister: Some(MemoryPersister::new(memory_storage)),
            hook_executor: None,
            steer_registry,
            channel_router: Arc::new(RwLock::new(None)),
            #[cfg(test)]
            fail_start_task_run_once: Arc::new(AtomicBool::new(false)),
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

    #[cfg(test)]
    fn inject_start_task_run_failure(&self) {
        self.fail_start_task_run_once.store(true, Ordering::SeqCst);
    }

    async fn has_resume_intent(&self, task_id: &str) -> bool {
        self.resume_states.read().await.contains_key(task_id)
            || self
                .resume_checkpoint_ids
                .read()
                .await
                .contains_key(task_id)
    }

    async fn staged_resume_intent(
        &self,
        task_id: &str,
    ) -> (Option<restflow_ai::AgentState>, Option<String>) {
        let state = self.resume_states.read().await.get(task_id).cloned();
        let checkpoint_id = self
            .resume_checkpoint_ids
            .read()
            .await
            .get(task_id)
            .cloned();
        (state, checkpoint_id)
    }

    fn build_run_handle_for_task_run(
        &self,
        task: &BackgroundAgent,
        run: &BackgroundAgentRun,
    ) -> restflow_telemetry::RunHandle {
        let trace_session_id = {
            let session_id = task.chat_session_id.trim();
            if session_id.is_empty() {
                task.id.clone()
            } else {
                session_id.to_string()
            }
        };
        let telemetry_sink =
            crate::telemetry::build_execution_trace_sink(self.execution_trace_storage());
        RunLifecycleService::new(telemetry_sink).handle(RunDescriptor::new(
            RunKind::BackgroundTask,
            run.run_id.clone(),
            trace_session_id,
            task.id.clone(),
            task.agent_id.clone(),
        ))
    }

    async fn activate_resume_intent_for_launch(&self, task_id: &str) -> Result<bool> {
        if !self.has_resume_intent(task_id).await {
            return Ok(false);
        }
        let Some(mut task) = self.storage.get_task(task_id)? else {
            return Err(anyhow!("Task {} not found", task_id));
        };
        if task.status != BackgroundAgentStatus::Paused {
            return Ok(false);
        }
        task.status = BackgroundAgentStatus::Active;
        task.updated_at = chrono::Utc::now().timestamp_millis();
        self.storage.save_task(&task)?;
        Ok(true)
    }

    async fn consume_resume_checkpoint(&self, task_id: &str, checkpoint_id: &str) -> Result<()> {
        let checkpoint = self
            .storage
            .load_checkpoint(checkpoint_id)?
            .ok_or_else(|| {
                anyhow!(
                    "Checkpoint {} not found for task {}",
                    checkpoint_id,
                    task_id
                )
            })?;
        if checkpoint.task_id.as_deref() != Some(task_id) {
            anyhow::bail!(
                "Checkpoint {} no longer belongs to task {}",
                checkpoint_id,
                task_id
            );
        }

        let mut checkpoint = checkpoint;
        if !checkpoint.is_resumed() {
            checkpoint.mark_resumed();
            self.storage.save_checkpoint(&checkpoint)?;
        }
        if let Some(savepoint_id) = checkpoint.savepoint_id {
            self.storage.delete_checkpoint_savepoint(savepoint_id)?;
        }
        Ok(())
    }

    async fn rollback_precommit_launch(
        &self,
        task_id: &str,
        original_task: &BackgroundAgent,
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
                "Failed to mark pre-commit background run as interrupted"
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
                    "Failed to load task during pre-commit rollback"
                );
            }
        }

        self.cleanup_runtime_tracking(task_id).await;
    }

    async fn recover_active_run_with_finalizer(
        &self,
        task: &BackgroundAgent,
        run: &BackgroundAgentRun,
        reason: &str,
        ended_at: i64,
    ) {
        let run_handle = self.build_run_handle_for_task_run(task, run);
        let finalizer = BackgroundRunFinalizer::new(self, task.clone(), None, run_handle);
        let duration_ms = ended_at.saturating_sub(run.started_at);
        finalizer.finalize_interrupted(reason, duration_ms).await;
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
                    if task.status == BackgroundAgentStatus::Running
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
                error!(
                    "Failed to list background task runs for startup recovery: {}",
                    err
                );
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
            if task.status == BackgroundAgentStatus::Running {
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
                        "Recovered stalled background execution",
                        current_time,
                    )
                    .await;
                    recovered_task_ids.insert(run.task_id.clone());
                    if task.status == BackgroundAgentStatus::Running
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
                    "Failed to list background task runs for stalled-task recovery: {}",
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
            if task.status != BackgroundAgentStatus::Running {
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
        if original_task.status == BackgroundAgentStatus::Paused && !resume_launch {
            warn!("Cannot run paused task {}", task_id);
            self.cleanup_runtime_tracking(&task_id_owned).await;
            return;
        }
        if original_task.status == BackgroundAgentStatus::Completed {
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
                self.rollback_precommit_launch(
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
                self.rollback_precommit_launch(
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
            self.rollback_precommit_launch(
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
        let checkpoint_id = payload.checkpoint_id.trim();
        if checkpoint_id.is_empty() {
            warn!("Cannot resume task {} with empty checkpoint_id", task_id);
            return;
        }

        // Load checkpoint from storage
        let checkpoint = match self.storage.load_checkpoint(checkpoint_id) {
            Ok(Some(cp)) => cp,
            Ok(None) => {
                warn!("No checkpoint {} found for task {}", checkpoint_id, task_id);
                return;
            }
            Err(e) => {
                error!(
                    "Failed to load checkpoint {} for task {}: {}",
                    checkpoint_id, task_id, e
                );
                return;
            }
        };

        if checkpoint.task_id.as_deref() != Some(task_id) {
            warn!(
                "Checkpoint {} does not belong to task {}",
                checkpoint_id, task_id
            );
            return;
        }

        if checkpoint.is_resumed() {
            warn!(
                "Checkpoint {} for task {} was already resumed",
                checkpoint_id, task_id
            );
            return;
        }

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

        let checkpoint_id = checkpoint.id.clone();

        if !payload.approved {
            let mut denied_checkpoint = checkpoint;
            denied_checkpoint.mark_resumed();
            if let Err(e) = self.storage.save_checkpoint(&denied_checkpoint) {
                warn!("Failed to mark checkpoint as resumed: {}", e);
            }
            if let Some(savepoint_id) = denied_checkpoint.savepoint_id
                && let Err(e) = self.storage.delete_checkpoint_savepoint(savepoint_id)
            {
                warn!(
                    "Failed to delete checkpoint savepoint {} for task {}: {}",
                    savepoint_id, task_id, e
                );
            }

            if let Ok(Some(mut task)) = self.storage.get_task(task_id) {
                task.status = BackgroundAgentStatus::Paused;
                task.updated_at = chrono::Utc::now().timestamp_millis();
                if let Err(e) = self.storage.save_task(&task) {
                    error!(
                        "Failed to update task status after checkpoint denial: {}",
                        e
                    );
                    return;
                }
            }
        } else {
            let Some(state) = restored_state else {
                error!(
                    "Cannot resume task {} from checkpoint {} without a valid restored state",
                    task_id, checkpoint_id
                );
                return;
            };
            self.resume_states
                .write()
                .await
                .insert(task_id.to_string(), state);
            self.resume_checkpoint_ids
                .write()
                .await
                .insert(task_id.to_string(), checkpoint_id.clone());

            info!(
                task_id = %task_id,
                checkpoint_id = %checkpoint_id,
                approved = payload.approved,
                "Staged checkpoint resume intent"
            );
            self.run_task_immediate(task_id).await;
        }

        let detail = format!(
            "Resumed from checkpoint {}: {}",
            checkpoint_id,
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

        info!(
            task_id = %task_id,
            checkpoint_id = %checkpoint_id,
            approved = payload.approved,
            "Processed checkpoint resume request"
        );
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
                self.rollback_precommit_launch(
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

        let execution_mode_str = match &task.execution_mode {
            ExecutionMode::Api => "api".to_string(),
            ExecutionMode::Cli(cfg) => format!("cli:{}", cfg.binary),
        };

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
        let (resume_state, resume_checkpoint_id) = self.staged_resume_intent(task_id).await;

        let execution_trace_storage = self.execution_trace_storage();
        let telemetry_sink = crate::telemetry::build_execution_trace_sink(execution_trace_storage);
        let run_handle = RunLifecycleService::new(telemetry_sink).handle(RunDescriptor::new(
            RunKind::BackgroundTask,
            run_id,
            trace_session_id,
            task.id.clone(),
            task.agent_id.clone(),
        ));
        let execution_id = resume_state
            .as_ref()
            .map(|state| state.execution_id.clone())
            .unwrap_or_else(|| run_handle.run_id().to_string());
        #[cfg(test)]
        if self.fail_start_task_run_once.swap(false, Ordering::SeqCst) {
            pump_cancel.cancel();
            if let Some(pump) = message_pump.take() {
                let _ = pump.await;
            }
            self.rollback_precommit_launch(
                &task.id,
                &original_task,
                resume_launch,
                None,
                "Injected start_task_run failure",
            )
            .await;
            return Err(anyhow!(
                "Injected background task run creation failure for {}",
                task.id
            ));
        }
        let persisted_resume_checkpoint_id = resume_checkpoint_id.clone();
        if let Err(err) = self.storage.start_task_run(
            &task.id,
            run_handle.run_id().to_string(),
            execution_id,
            start_time,
            persisted_resume_checkpoint_id,
        ) {
            pump_cancel.cancel();
            if let Some(pump) = message_pump.take() {
                let _ = pump.await;
            }
            self.rollback_precommit_launch(
                &task.id,
                &original_task,
                resume_launch,
                None,
                "Failed to create background task run",
            )
            .await;
            return Err(anyhow!(
                "Failed to create background task run for {}: {}",
                task.id,
                err
            ));
        }
        run_handle.start().await;
        let finalizer = BackgroundRunFinalizer::new(
            self,
            task.clone(),
            resolved_input.clone(),
            run_handle.clone(),
        );
        if let Some(checkpoint_id) = resume_checkpoint_id.as_deref()
            && let Err(err) = self
                .consume_resume_checkpoint(&task.id, checkpoint_id)
                .await
        {
            let duration_ms = chrono::Utc::now().timestamp_millis() - start_time;
            let error_msg = format!("Execution error: failed to consume resume checkpoint: {err}");
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
        self.clear_resume_intent(task_id).await;
        self.event_emitter
            .emit(TaskStreamEvent::started(
                &task.id,
                &task.name,
                &task.agent_id,
                &execution_mode_str,
            ))
            .await;
        self.fire_hooks(&HookContext::from_started(&task)).await;
        let telemetry_context = Some(run_handle.cloned_context());

        if resolved_input
            .as_deref()
            .is_none_or(|value: &str| value.trim().is_empty())
        {
            let duration_ms = chrono::Utc::now().timestamp_millis() - start_time;
            let reason = "Background task requires non-empty input or input_template";
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
            Some(inner)
        } else {
            broadcast_emitter
        };

        let exec_future = async {
            match &task.execution_mode {
                ExecutionMode::Api => {
                    // Use the injected API executor
                    debug!("Using API executor for task '{}'", task.name);
                    if let Some(state) = resume_state {
                        if let Some(timeout_secs) = execution_timeout_secs {
                            tokio::time::timeout(
                                Duration::from_secs(timeout_secs),
                                self.executor.execute_from_state_with_emitter_and_telemetry(
                                    &task.agent_id,
                                    Some(&task.id),
                                    state,
                                    &task.memory,
                                    steer_rx,
                                    step_emitter,
                                    telemetry_context.clone(),
                                ),
                            )
                            .await
                        } else {
                            Ok(self
                                .executor
                                .execute_from_state_with_emitter_and_telemetry(
                                    &task.agent_id,
                                    Some(&task.id),
                                    state,
                                    &task.memory,
                                    steer_rx,
                                    step_emitter,
                                    telemetry_context.clone(),
                                )
                                .await)
                        }
                    } else if let Some(timeout_secs) = execution_timeout_secs {
                        tokio::time::timeout(
                            Duration::from_secs(timeout_secs),
                            self.executor.execute_with_emitter_and_telemetry(
                                &task.agent_id,
                                Some(&task.id),
                                resolved_input.as_deref(),
                                &task.memory,
                                steer_rx,
                                step_emitter,
                                telemetry_context.clone(),
                            ),
                        )
                        .await
                    } else {
                        Ok(self
                            .executor
                            .execute_with_emitter_and_telemetry(
                                &task.agent_id,
                                Some(&task.id),
                                resolved_input.as_deref(),
                                &task.memory,
                                steer_rx,
                                step_emitter,
                                telemetry_context.clone(),
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
