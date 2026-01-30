//! Application state management for Tauri

use crate::agent_task::runner::{
    AgentExecutor, AgentTaskRunner, NotificationSender, RunnerConfig, RunnerHandle,
};
use crate::agent_task::{HeartbeatEmitter, TauriHeartbeatEmitter};
use crate::channel::{SystemStatus, TaskTrigger};
use crate::commands::agent_task::ActiveTaskInfo;
use anyhow::Result;
use async_trait::async_trait;
use restflow_core::channel::ChannelRouter;
use restflow_core::models::AgentTask;
use restflow_core::AppCore;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

/// Information about a running task stored in state
#[derive(Debug, Clone)]
pub struct RunningTaskState {
    pub task_id: String,
    pub task_name: String,
    pub agent_id: String,
    pub started_at: i64,
    pub execution_mode: String,
}

/// Application state shared across Tauri commands
pub struct AppState {
    pub core: Arc<AppCore>,
    /// Handle to control the background agent task runner
    runner_handle: RwLock<Option<RunnerHandle>>,
    /// Currently running tasks (task_id -> RunningTaskState)
    running_tasks: RwLock<HashMap<String, RunningTaskState>>,
    /// Channel router for message handling
    pub channel_router: Arc<ChannelRouter>,
}

impl AppState {
    pub async fn new(db_path: &str) -> anyhow::Result<Self> {
        let core = Arc::new(AppCore::new(db_path).await?);
        let channel_router = Arc::new(ChannelRouter::new());
        Ok(Self {
            core,
            runner_handle: RwLock::new(None),
            running_tasks: RwLock::new(HashMap::new()),
            channel_router,
        })
    }

    /// Get a reference to the channel router
    pub fn channel_router(&self) -> Arc<ChannelRouter> {
        self.channel_router.clone()
    }

    /// Start the agent task runner with the provided executor and notifier.
    ///
    /// This spawns a background task that polls for runnable tasks and executes them.
    /// If a runner is already active, this will stop it first.
    ///
    /// Status updates are emitted inline during the poll cycle via the heartbeat emitter.
    pub async fn start_runner<E, N>(
        &self,
        executor: E,
        notifier: N,
        config: Option<RunnerConfig>,
    ) -> Result<()>
    where
        E: AgentExecutor + 'static,
        N: NotificationSender + 'static,
    {
        // Stop existing runner if any
        self.stop_runner().await?;

        // Clone the agent_tasks storage from core
        let storage = Arc::new(self.core.storage.agent_tasks.clone());
        let runner = Arc::new(AgentTaskRunner::new(
            storage,
            Arc::new(executor),
            Arc::new(notifier),
            config.unwrap_or_default(),
        ));

        let handle = runner.start();
        info!("Agent task runner started");

        let mut guard = self.runner_handle.write().await;
        *guard = Some(handle);

        Ok(())
    }

    /// Start the agent task runner with heartbeat events emitted to Tauri.
    ///
    /// This variant includes a heartbeat emitter for sending status updates to the frontend.
    pub async fn start_runner_with_heartbeat<E, N>(
        &self,
        executor: E,
        notifier: N,
        config: Option<RunnerConfig>,
        app_handle: tauri::AppHandle,
    ) -> Result<()>
    where
        E: AgentExecutor + 'static,
        N: NotificationSender + 'static,
    {
        // Stop existing runner if any
        self.stop_runner().await?;

        // Clone the agent_tasks storage from core
        let storage = Arc::new(self.core.storage.agent_tasks.clone());
        let heartbeat_emitter: Arc<dyn HeartbeatEmitter> =
            Arc::new(TauriHeartbeatEmitter::new(app_handle));

        let runner = Arc::new(AgentTaskRunner::with_heartbeat_emitter(
            storage,
            Arc::new(executor),
            Arc::new(notifier),
            config.unwrap_or_default(),
            heartbeat_emitter,
        ));

        let handle = runner.start();
        info!("Agent task runner started with heartbeat emitter");

        let mut guard = self.runner_handle.write().await;
        *guard = Some(handle);

        Ok(())
    }

    /// Stop the agent task runner if it's running.
    ///
    /// This gracefully shuts down the background task runner.
    pub async fn stop_runner(&self) -> Result<()> {
        let mut guard = self.runner_handle.write().await;
        if let Some(handle) = guard.take() {
            info!("Stopping agent task runner");
            if let Err(e) = handle.stop().await {
                error!("Error stopping runner: {}", e);
                // Don't propagate - runner may have already stopped
            }
        }
        Ok(())
    }

    /// Check if the runner is currently active.
    pub async fn is_runner_active(&self) -> bool {
        self.runner_handle.read().await.is_some()
    }

    /// Trigger an immediate check for runnable tasks.
    ///
    /// Returns an error if no runner is active.
    pub async fn trigger_task_check(&self) -> Result<()> {
        let guard = self.runner_handle.read().await;
        if let Some(handle) = guard.as_ref() {
            handle.check_now().await
        } else {
            anyhow::bail!("No runner is active")
        }
    }

    /// Run a specific task immediately, bypassing its schedule.
    ///
    /// Returns an error if no runner is active.
    pub async fn run_task_now(&self, task_id: String) -> Result<()> {
        let guard = self.runner_handle.read().await;
        if let Some(handle) = guard.as_ref() {
            handle.run_task_now(task_id).await
        } else {
            anyhow::bail!("No runner is active")
        }
    }

    // ========================================================================
    // Running Task Tracking
    // ========================================================================

    /// Check if a task is currently running
    pub async fn is_task_running(&self, task_id: &str) -> bool {
        self.running_tasks.read().await.contains_key(task_id)
    }

    /// Mark a task as running
    pub async fn mark_task_running(&self, state: RunningTaskState) {
        let mut guard = self.running_tasks.write().await;
        guard.insert(state.task_id.clone(), state);
    }

    /// Mark a task as completed (remove from running)
    pub async fn mark_task_completed(&self, task_id: &str) {
        let mut guard = self.running_tasks.write().await;
        guard.remove(task_id);
    }

    /// Get list of all currently running tasks
    pub async fn get_active_tasks(&self) -> Result<Vec<ActiveTaskInfo>> {
        let guard = self.running_tasks.read().await;
        Ok(guard
            .values()
            .map(|state| ActiveTaskInfo {
                task_id: state.task_id.clone(),
                task_name: state.task_name.clone(),
                agent_id: state.agent_id.clone(),
                started_at: state.started_at,
                execution_mode: state.execution_mode.clone(),
            })
            .collect())
    }

    /// Get count of running tasks
    pub async fn running_task_count(&self) -> usize {
        self.running_tasks.read().await.len()
    }
}

// ============================================================================
// TaskTrigger Implementation
// ============================================================================

/// Wrapper to implement TaskTrigger for AppState
///
/// This struct wraps an Arc<AppState> to implement the TaskTrigger trait,
/// allowing the channel message handler to interact with tasks.
pub struct AppTaskTrigger {
    state: Arc<AppState>,
}

impl AppTaskTrigger {
    /// Create a new AppTaskTrigger
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl TaskTrigger for AppTaskTrigger {
    async fn list_tasks(&self) -> Result<Vec<AgentTask>> {
        self.state.core.storage.agent_tasks.list_tasks()
    }

    async fn find_and_run_task(&self, name_or_id: &str) -> Result<AgentTask> {
        // Try to find by ID first
        if let Ok(Some(task)) = self.state.core.storage.agent_tasks.get_task(name_or_id) {
            // Trigger the task to run
            self.state.run_task_now(task.id.clone()).await?;
            return Ok(task);
        }

        // Try to find by name
        let tasks = self.state.core.storage.agent_tasks.list_tasks()?;
        let task = tasks
            .into_iter()
            .find(|t| t.name.to_lowercase() == name_or_id.to_lowercase())
            .ok_or_else(|| anyhow::anyhow!("Task not found: {}", name_or_id))?;

        // Trigger the task to run
        self.state.run_task_now(task.id.clone()).await?;
        Ok(task)
    }

    async fn stop_task(&self, task_id: &str) -> Result<()> {
        // Mark the task as completed/stopped in our tracking
        self.state.mark_task_completed(task_id).await;

        // Update task status in storage
        if let Ok(Some(mut task)) = self.state.core.storage.agent_tasks.get_task(task_id) {
            task.status = restflow_core::models::AgentTaskStatus::Paused;
            self.state.core.storage.agent_tasks.update_task(&task)?;
        }

        Ok(())
    }

    async fn get_status(&self) -> Result<SystemStatus> {
        let runner_active = self.state.is_runner_active().await;
        let active_count = self.state.running_task_count().await;

        // Count pending (active but not running) tasks
        let tasks = self.state.core.storage.agent_tasks.list_tasks()?;
        let pending_count = tasks
            .iter()
            .filter(|t| t.status == restflow_core::models::AgentTaskStatus::Active)
            .count();

        // Count completed today
        let today_start = chrono::Utc::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .map(|dt| dt.and_utc().timestamp_millis())
            .unwrap_or(0);

        let completed_today = tasks
            .iter()
            .filter(|t| {
                t.status == restflow_core::models::AgentTaskStatus::Completed
                    && t.updated_at >= today_start
            })
            .count();

        Ok(SystemStatus {
            runner_active,
            active_count,
            pending_count,
            completed_today,
        })
    }

    async fn send_input_to_task(&self, _task_id: &str, _input: &str) -> Result<()> {
        // TODO: Implement task input forwarding once we have a task input channel
        // For now, this is a placeholder that will be implemented when we add
        // interactive task support
        info!(
            "Task input forwarding not yet implemented: {} -> {}",
            _task_id, _input
        );
        Ok(())
    }

    async fn handle_approval(&self, _task_id: &str, _approved: bool) -> Result<bool> {
        // TODO: Implement approval handling once we have the approval queue
        // For now, this is a placeholder
        info!(
            "Task approval handling not yet implemented: {} approved={}",
            _task_id, _approved
        );
        Ok(false)
    }
}
