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
use restflow_ai::agent::SubagentDefLookup;
use restflow_ai::{LlmClient, SecretResolver};
use restflow_core::channel::ChannelRouter;
use restflow_core::models::{BackgroundAgent, BackgroundAgentStatus, BackgroundMessageSource};
use restflow_core::process::ProcessRegistry;
use restflow_core::steer::SteerRegistry;
use restflow_core::storage::SystemConfig;
use std::sync::{Arc, RwLock};
use tokio::sync::{Mutex, mpsc};
use tracing::{error, warn};

/// Application state shared across Tauri commands.
///
/// In desktop mode, this state acts as a UI facade over daemon IPC.
/// Business logic and storage access stay in daemon/core.
///
/// Architecture guardrails:
/// - Do not add direct storage handles to this struct.
/// - Do not add command paths that bypass `TauriExecutor`.
pub struct AppState {
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
    pub subagent_definitions: Arc<dyn SubagentDefLookup>,
    /// Configuration for sub-agent execution
    pub subagent_config: SharedSubagentConfig,
    /// Steer registry for sending messages to running tasks
    pub steer_registry: Arc<SteerRegistry>,
}

impl AppState {
    /// Compatibility constructor.
    ///
    /// Deprecated: Tauri must run in daemon IPC mode.
    /// The `db_path` parameter is intentionally ignored.
    #[deprecated(
        note = "Use AppState::with_ipc(). Direct storage mode is forbidden in daemon-centric architecture."
    )]
    pub async fn new(_db_path: &str) -> anyhow::Result<Self> {
        Self::with_ipc().await
    }

    pub async fn with_ipc() -> anyhow::Result<Self> {
        let channel_router = Arc::new(ChannelRouter::new());
        let process_registry = Arc::new(ProcessRegistry::new());
        let (completion_tx, completion_rx) = mpsc::channel(100);
        let subagent_tracker = Arc::new(SubagentTracker::new(completion_tx, completion_rx));
        let subagent_definitions = Arc::new(AgentDefinitionRegistry::with_builtins());
        let daemon = Arc::new(Mutex::new(DaemonManager::new()));
        let executor = Arc::new(TauriExecutor::new(daemon.clone()));
        let subagent_config =
            SharedSubagentConfig::new(load_subagent_config_from_executor(&executor).await);
        let steer_registry = Arc::new(SteerRegistry::new());

        Ok(Self {
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
            config: self.subagent_config.snapshot(),
            llm_client_factory: None,
            orchestrator: None,
        }
    }

    /// Refresh the in-memory sub-agent runtime config after a config update.
    pub fn refresh_subagent_config(&self, config: &SystemConfig) {
        self.subagent_config
            .update(subagent_config_from_system_config(config));
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

    /// Stop a running task.
    pub async fn stop_task(&self, task_id: String) -> Result<()> {
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

#[derive(Clone)]
pub struct SharedSubagentConfig {
    inner: Arc<RwLock<SubagentConfig>>,
}

impl SharedSubagentConfig {
    fn new(config: SubagentConfig) -> Self {
        Self {
            inner: Arc::new(RwLock::new(config)),
        }
    }

    fn snapshot(&self) -> SubagentConfig {
        self.inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn update(&self, config: SubagentConfig) {
        *self
            .inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = config;
    }
}

fn subagent_config_from_system_config(config: &SystemConfig) -> SubagentConfig {
    SubagentConfig {
        max_parallel_agents: config.agent.max_parallel_subagents,
        subagent_timeout_secs: config.agent.subagent_timeout_secs,
        max_iterations: config.agent.max_iterations,
        max_depth: config.agent.max_depth,
    }
}

async fn load_subagent_config_from_executor(executor: &TauriExecutor) -> SubagentConfig {
    match executor.get_config().await {
        Ok(config) => subagent_config_from_system_config(&config),
        Err(error) => {
            warn!(
                error = %error,
                "Failed to load daemon config for sub-agent runtime; falling back to defaults"
            );
            SubagentConfig::default()
        }
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
        let stop_requested = match self.state.stop_task(task_id.to_string()).await {
            Ok(()) => true,
            Err(e) => {
                error!("Failed to request stop for task {}: {}", task_id, e);
                false
            }
        };

        // If the task isn't running (or stop couldn't be requested), persist stop state directly.
        if let Ok(Some(task)) = self
            .state
            .executor()
            .get_background_agent(task_id.to_string())
            .await
            && (task.status != BackgroundAgentStatus::Running || !stop_requested)
        {
            let _ = self
                .state
                .executor()
                .control_background_agent(
                    task_id.to_string(),
                    restflow_core::models::BackgroundAgentControlAction::Stop,
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
        self.state
            .executor()
            .handle_background_agent_approval(task_id.to_string(), approved)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subagent_config_from_system_config_prefers_runtime_config() {
        let mut config = SystemConfig::default();
        config.agent.max_parallel_subagents = 88;
        config.agent.subagent_timeout_secs = 7200;
        config.agent.max_iterations = 144;
        config.agent.max_depth = 5;

        let mapped = subagent_config_from_system_config(&config);

        assert_eq!(mapped.max_parallel_agents, 88);
        assert_eq!(mapped.subagent_timeout_secs, 7200);
        assert_eq!(mapped.max_iterations, 144);
        assert_eq!(mapped.max_depth, 5);
    }

    #[test]
    fn shared_subagent_config_snapshot_reflects_refresh() {
        let shared = SharedSubagentConfig::new(SubagentConfig::default());
        let mut config = SystemConfig::default();
        config.agent.max_parallel_subagents = 22;
        config.agent.subagent_timeout_secs = 4800;
        config.agent.max_iterations = 199;
        config.agent.max_depth = 6;

        shared.update(subagent_config_from_system_config(&config));
        let snapshot = shared.snapshot();

        assert_eq!(snapshot.max_parallel_agents, 22);
        assert_eq!(snapshot.subagent_timeout_secs, 4800);
        assert_eq!(snapshot.max_iterations, 199);
        assert_eq!(snapshot.max_depth, 6);
    }
}
