//! Application state management for Tauri

use crate::agent_task::runner::{
    AgentExecutor, AgentTaskRunner, NotificationSender, RunnerConfig, RunnerHandle,
};
use anyhow::Result;
use restflow_core::AppCore;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

/// Application state shared across Tauri commands
pub struct AppState {
    pub core: Arc<AppCore>,
    /// Handle to control the background agent task runner
    runner_handle: RwLock<Option<RunnerHandle>>,
}

impl AppState {
    pub async fn new(db_path: &str) -> anyhow::Result<Self> {
        let core = Arc::new(AppCore::new(db_path).await?);
        Ok(Self {
            core,
            runner_handle: RwLock::new(None),
        })
    }

    /// Start the agent task runner with the provided executor and notifier.
    ///
    /// This spawns a background task that polls for runnable tasks and executes them.
    /// If a runner is already active, this will stop it first.
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
}
