//! Application state management for Tauri

use crate::agent::{SubagentDeps, ToolRegistry};
use crate::channel::{BackgroundAgentTrigger, SystemStatus};
use crate::chat::StreamManager;
use crate::commands::background_agent::ActiveBackgroundAgentInfo;
use crate::daemon_manager::DaemonManager;
use crate::executor::TauriExecutor;
use crate::subagent::{AgentDefinitionRegistry, SubagentConfig, SubagentTracker};
use anyhow::Result;
use async_trait::async_trait;
use restflow_ai::{LlmClient, SecretResolver};
use restflow_core::channel::ChannelRouter;
use restflow_core::models::{BackgroundAgent, BackgroundAgentStatus, BackgroundMessageSource};
use restflow_core::process::ProcessRegistry;
use restflow_core::security::SecurityChecker;
use restflow_core::steer::SteerRegistry;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use tracing::error;

/// Application state shared across Tauri commands.
///
/// In desktop mode, this state acts as a UI facade over daemon IPC.
/// Business logic and storage access stay in daemon/core.
pub struct AppState {
    /// Security checker for command execution control
    security_checker: Arc<SecurityChecker>,
    /// Channel router for message handling
    pub channel_router: Arc<ChannelRouter>,
    /// Process registry for background process tool
    pub process_registry: Arc<ProcessRegistry>,
    /// Active chat stream manager
    pub stream_manager: StreamManager,
    /// Daemon manager for IPC connections
    pub daemon: Arc<Mutex<DaemonManager>>,
    /// IPC executor for daemon-backed operations
    pub executor: Arc<TauriExecutor>,
    /// Sub-agent tracker for spawned agent tasks
    pub subagent_tracker: Arc<SubagentTracker>,
    /// Registry of available sub-agent definitions
    pub subagent_definitions: Arc<AgentDefinitionRegistry>,
    /// Configuration for sub-agent execution
    pub subagent_config: SubagentConfig,
    /// Steer registry for sending messages to running tasks
    pub steer_registry: Arc<SteerRegistry>,
}

impl AppState {
    /// Compatibility constructor.
    ///
    /// Tauri should use daemon IPC mode; the db_path is ignored.
    pub async fn new(_db_path: &str) -> anyhow::Result<Self> {
        Self::with_ipc().await
    }

    pub async fn with_ipc() -> anyhow::Result<Self> {
        let security_checker = Arc::new(SecurityChecker::with_defaults());
        let channel_router = Arc::new(ChannelRouter::new());
        let process_registry = Arc::new(ProcessRegistry::new());
        let (completion_tx, completion_rx) = mpsc::channel(100);
        let subagent_tracker = Arc::new(SubagentTracker::new(completion_tx, completion_rx));
        let subagent_definitions = Arc::new(AgentDefinitionRegistry::with_builtins());
        let subagent_config = SubagentConfig::default();
        let daemon = Arc::new(Mutex::new(DaemonManager::new()));
        let executor = Arc::new(TauriExecutor::new(daemon.clone()));
        let steer_registry = Arc::new(SteerRegistry::new());

        Ok(Self {
            security_checker,
            channel_router,
            process_registry,
            stream_manager: StreamManager::new(),
            daemon,
            executor,
            subagent_tracker,
            subagent_definitions,
            subagent_config,
            steer_registry,
        })
    }

    /// Get a reference to the security checker.
    pub fn security_checker(&self) -> &SecurityChecker {
        &self.security_checker
    }

    /// Get the security checker as an Arc (for sharing with tools).
    pub fn security_checker_arc(&self) -> Arc<SecurityChecker> {
        self.security_checker.clone()
    }

    /// Get a reference to the channel router.
    pub fn channel_router(&self) -> Arc<ChannelRouter> {
        self.channel_router.clone()
    }

    /// Get the IPC executor.
    pub fn executor(&self) -> Arc<TauriExecutor> {
        self.executor.clone()
    }

    /// Build sub-agent dependencies for tool registry construction.
    pub fn subagent_deps(&self, llm_client: Arc<dyn LlmClient>) -> SubagentDeps {
        SubagentDeps {
            tracker: self.subagent_tracker.clone(),
            definitions: self.subagent_definitions.clone(),
            llm_client,
            tool_registry: Arc::new(ToolRegistry::new()),
            config: self.subagent_config.clone(),
        }
    }

    /// Build a secret resolver from process environment.
    ///
    /// Tauri no longer reads secrets from local storage directly.
    pub fn secret_resolver(&self) -> Option<SecretResolver> {
        Some(Arc::new(|key| std::env::var(key).ok()))
    }

    /// No-op in IPC-only mode.
    pub async fn stop_runner(&self) -> Result<()> {
        Ok(())
    }

    /// Check if daemon currently reports running background agents.
    pub async fn is_runner_active(&self) -> bool {
        match self
            .executor()
            .list_background_agents(Some("running".to_string()))
            .await
        {
            Ok(tasks) => !tasks.is_empty(),
            Err(err) => {
                error!(error = %err, "Failed to query runner status via IPC");
                false
            }
        }
    }

    /// Trigger an immediate check for runnable tasks.
    pub async fn trigger_task_check(&self) -> Result<()> {
        let runnable = self
            .executor()
            .list_runnable_background_agents(Some(chrono::Utc::now().timestamp_millis()))
            .await?;
        for task in runnable {
            self.executor()
                .control_background_agent(
                    task.id,
                    restflow_core::models::BackgroundAgentControlAction::RunNow,
                )
                .await?;
        }
        Ok(())
    }

    /// Run a specific task immediately, bypassing its schedule.
    pub async fn run_task_now(&self, task_id: String) -> Result<()> {
        self.executor()
            .control_background_agent(
                task_id,
                restflow_core::models::BackgroundAgentControlAction::RunNow,
            )
            .await?;
        Ok(())
    }

    /// Cancel a running task.
    pub async fn cancel_task(&self, task_id: String) -> Result<()> {
        self.executor()
            .control_background_agent(
                task_id,
                restflow_core::models::BackgroundAgentControlAction::Stop,
            )
            .await?;
        Ok(())
    }

    /// Check if a task is currently running.
    pub async fn is_task_running(&self, task_id: &str) -> bool {
        match self
            .executor()
            .get_background_agent(task_id.to_string())
            .await
        {
            Ok(Some(task)) => task.status == BackgroundAgentStatus::Running,
            Ok(None) => false,
            Err(err) => {
                error!(error = %err, task_id = task_id, "Failed to query background agent status");
                false
            }
        }
    }

    /// Get list of all currently running tasks.
    pub async fn get_active_tasks(&self) -> Result<Vec<ActiveBackgroundAgentInfo>> {
        let running = self
            .executor()
            .list_background_agents(Some("running".to_string()))
            .await?;
        Ok(running
            .into_iter()
            .map(|task| ActiveBackgroundAgentInfo {
                task_id: task.id,
                task_name: task.name,
                agent_id: task.agent_id,
                started_at: task.last_run_at.unwrap_or(task.updated_at),
                execution_mode: match task.execution_mode {
                    restflow_core::models::ExecutionMode::Api => "api".to_string(),
                    restflow_core::models::ExecutionMode::Cli(cfg) => format!("cli:{}", cfg.binary),
                },
            })
            .collect())
    }
}

// ============================================================================
// BackgroundAgentTrigger Implementation
// ============================================================================

/// Wrapper to implement BackgroundAgentTrigger for AppState.
pub struct AppBackgroundAgentTrigger {
    state: Arc<AppState>,
}

impl AppBackgroundAgentTrigger {
    /// Create a new AppBackgroundAgentTrigger.
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl BackgroundAgentTrigger for AppBackgroundAgentTrigger {
    async fn list_background_agents(&self) -> Result<Vec<BackgroundAgent>> {
        self.state.executor().list_background_agents(None).await
    }

    async fn find_and_run_background_agent(&self, name_or_id: &str) -> Result<BackgroundAgent> {
        // Try to find by ID first.
        if let Ok(Some(task)) = self
            .state
            .executor()
            .get_background_agent(name_or_id.to_string())
            .await
        {
            self.state.run_task_now(task.id.clone()).await?;
            return Ok(task);
        }

        // Try to find by name.
        let tasks = self.state.executor().list_background_agents(None).await?;
        let task = tasks
            .into_iter()
            .find(|t| t.name.to_lowercase() == name_or_id.to_lowercase())
            .ok_or_else(|| anyhow::anyhow!("Background agent not found: {}", name_or_id))?;

        self.state.run_task_now(task.id.clone()).await?;
        Ok(task)
    }

    async fn stop_background_agent(&self, task_id: &str) -> Result<()> {
        let cancel_requested = match self.state.cancel_task(task_id.to_string()).await {
            Ok(()) => true,
            Err(e) => {
                error!("Failed to request cancel for task {}: {}", task_id, e);
                false
            }
        };

        // If the task isn't running (or cancel couldn't be requested), pause it directly.
        if let Ok(Some(task)) = self
            .state
            .executor()
            .get_background_agent(task_id.to_string())
            .await
            && (task.status != BackgroundAgentStatus::Running || !cancel_requested)
        {
            let _ = self
                .state
                .executor()
                .control_background_agent(
                    task_id.to_string(),
                    restflow_core::models::BackgroundAgentControlAction::Pause,
                )
                .await?;
        }

        Ok(())
    }

    async fn get_status(&self) -> Result<SystemStatus> {
        let tasks = self.state.executor().list_background_agents(None).await?;
        let active_count = tasks
            .iter()
            .filter(|t| t.status == BackgroundAgentStatus::Running)
            .count();
        let runner_active = active_count > 0;

        // Count pending (active but not running) tasks.
        let pending_count = tasks
            .iter()
            .filter(|t| t.status == BackgroundAgentStatus::Active)
            .count();

        // Count completed today.
        let today_start = chrono::Utc::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .map(|dt| dt.and_utc().timestamp_millis())
            .unwrap_or(0);

        let completed_today = tasks
            .iter()
            .filter(|t| t.status == BackgroundAgentStatus::Completed && t.updated_at >= today_start)
            .count();

        Ok(SystemStatus {
            runner_active,
            active_count,
            pending_count,
            completed_today,
        })
    }

    async fn send_message_to_background_agent(&self, task_id: &str, input: &str) -> Result<()> {
        self.state
            .executor()
            .send_background_agent_message(
                task_id.to_string(),
                input.to_string(),
                Some(BackgroundMessageSource::User),
            )
            .await?;
        Ok(())
    }

    async fn handle_background_agent_approval(
        &self,
        task_id: &str,
        approved: bool,
    ) -> Result<bool> {
        let approval_manager = self.state.security_checker().approval_manager();
        let pending = approval_manager.get_for_task(task_id).await;
        if let Some(approval) = pending.first() {
            let resolved = if approved {
                approval_manager.approve(&approval.id).await?
            } else {
                approval_manager
                    .reject(
                        &approval.id,
                        Some("Rejected via background agent control command".to_string()),
                    )
                    .await?
            };
            if resolved.is_some() {
                return Ok(true);
            }
        }

        let message = if approved {
            "User approved the pending action."
        } else {
            "User rejected the pending action."
        };

        self.state
            .executor()
            .send_background_agent_message(
                task_id.to_string(),
                message.to_string(),
                Some(BackgroundMessageSource::System),
            )
            .await?;
        Ok(false)
    }
}
