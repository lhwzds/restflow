use anyhow::Result;
use async_trait::async_trait;
use restflow_core::AppCore;
use restflow_core::auth::{AuthManagerConfig, AuthProfileManager};
use restflow_core::storage::SecretStorage;
use restflow_core::channel::ChannelRouter;
use restflow_core::models::{AgentTask, AgentTaskStatus};
use restflow_core::paths;
use restflow_core::process::ProcessRegistry;
use restflow_tauri_lib::{
    AgentTaskRunner, ChatDispatcher, ChatDispatcherConfig, ChatSessionManager, MessageDebouncer,
    MessageHandlerConfig, RealAgentExecutor, RunnerConfig, RunnerHandle, SystemStatus, TaskTrigger,
    TelegramNotifier, start_message_handler_with_chat,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

use super::telegram;

pub struct CliTaskRunner {
    core: Arc<AppCore>,
    handle: Arc<RwLock<Option<Arc<RunnerHandle>>>>,
    runner: Arc<RwLock<Option<Arc<AgentTaskRunner>>>>,
    router: Arc<RwLock<Option<Arc<ChannelRouter>>>>,
}

fn create_auth_manager(secrets: Arc<SecretStorage>) -> Result<AuthProfileManager> {
    let mut config = AuthManagerConfig::default();
    let profiles_path = paths::ensure_data_dir()?.join("auth_profiles.json");
    config.profiles_path = Some(profiles_path);
    Ok(AuthProfileManager::with_config(config, secrets))
}

impl CliTaskRunner {
    pub fn new(core: Arc<AppCore>) -> Self {
        Self {
            core,
            handle: Arc::new(RwLock::new(None)),
            runner: Arc::new(RwLock::new(None)),
            router: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn start(&mut self) -> Result<()> {
        if self.handle.read().await.is_some() {
            anyhow::bail!("Runner already started");
        }

        let storage = self.core.storage.clone();
        let secrets = Arc::new(self.core.storage.secrets.clone());
        let process_registry = Arc::new(ProcessRegistry::new());

        let auth_manager = Arc::new(create_auth_manager(secrets.clone())?);
        auth_manager.initialize().await?;
        auth_manager.discover().await?;

        let executor = RealAgentExecutor::new(storage.clone(), process_registry, auth_manager.clone());
        let notifier = TelegramNotifier::new(secrets);

        let runner = Arc::new(AgentTaskRunner::new(
            Arc::new(storage.agent_tasks.clone()),
            Arc::new(executor),
            Arc::new(notifier),
            RunnerConfig {
                poll_interval_ms: 30_000,
                max_concurrent_tasks: 5,
                task_timeout_secs: 3600,
            },
        ));

        let handle = runner.clone().start();

        {
            let mut handle_guard = self.handle.write().await;
            *handle_guard = Some(Arc::new(handle));
        }

        {
            let mut runner_guard = self.runner.write().await;
            *runner_guard = Some(runner);
        }

        if let Some(router) = telegram::setup_telegram_channel(&self.core.storage.secrets)? {
            let trigger = Arc::new(CliTaskTrigger::new(
                self.core.clone(),
                self.handle.clone(),
                self.runner.clone(),
            ));

            // Create ChatDispatcher for AI conversations
            let session_manager = Arc::new(ChatSessionManager::new(
                storage.clone(),
                20, // max history messages
            ));
            let debouncer = Arc::new(MessageDebouncer::default_timeout());
            let chat_dispatcher = Arc::new(ChatDispatcher::new(
                session_manager,
                storage.clone(),
                auth_manager.clone(),
                debouncer,
                router.clone(),
                ChatDispatcherConfig::default(),
            ));

            start_message_handler_with_chat(
                router.clone(),
                trigger,
                chat_dispatcher,
                MessageHandlerConfig::default(),
            );
            let mut router_guard = self.router.write().await;
            *router_guard = Some(router);
            info!("Telegram channel enabled for CLI daemon with AI chat support");
        }

        info!("Task runner started");
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        if let Some(handle) = self.handle.write().await.take() {
            handle.stop().await?;
            info!("Task runner stopped");
        }

        let mut runner_guard = self.runner.write().await;
        *runner_guard = None;

        let mut router_guard = self.router.write().await;
        *router_guard = None;

        Ok(())
    }

    #[allow(dead_code)]
    pub async fn is_running(&self) -> bool {
        self.handle.read().await.is_some()
    }

    pub async fn run_task_now(&self, task_id: &str) -> Result<()> {
        if let Some(handle) = self.handle.read().await.as_ref() {
            handle.run_task_now(task_id.to_string()).await?;
        } else {
            anyhow::bail!("Runner not started");
        }
        Ok(())
    }
}

struct CliTaskTrigger {
    core: Arc<AppCore>,
    handle: Arc<RwLock<Option<Arc<RunnerHandle>>>>,
    runner: Arc<RwLock<Option<Arc<AgentTaskRunner>>>>,
}

impl CliTaskTrigger {
    fn new(
        core: Arc<AppCore>,
        handle: Arc<RwLock<Option<Arc<RunnerHandle>>>>,
        runner: Arc<RwLock<Option<Arc<AgentTaskRunner>>>>,
    ) -> Self {
        Self {
            core,
            handle,
            runner,
        }
    }

    async fn runner_handle(&self) -> Result<Arc<RunnerHandle>> {
        let handle_guard = self.handle.read().await;
        handle_guard
            .as_ref()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Runner not started"))
    }

    async fn running_task_count(&self) -> usize {
        let runner_guard = self.runner.read().await;
        match runner_guard.as_ref() {
            Some(runner) => runner.running_task_count().await,
            None => 0,
        }
    }
}

#[async_trait]
impl TaskTrigger for CliTaskTrigger {
    async fn list_tasks(&self) -> Result<Vec<AgentTask>> {
        self.core.storage.agent_tasks.list_tasks()
    }

    async fn find_and_run_task(&self, name_or_id: &str) -> Result<AgentTask> {
        if let Ok(Some(task)) = self.core.storage.agent_tasks.get_task(name_or_id) {
            self.runner_handle()
                .await?
                .run_task_now(task.id.clone())
                .await?;
            return Ok(task);
        }

        let tasks = self.core.storage.agent_tasks.list_tasks()?;
        let task = tasks
            .into_iter()
            .find(|t| t.name.eq_ignore_ascii_case(name_or_id))
            .ok_or_else(|| anyhow::anyhow!("Task not found: {}", name_or_id))?;

        self.runner_handle()
            .await?
            .run_task_now(task.id.clone())
            .await?;
        Ok(task)
    }

    async fn stop_task(&self, task_id: &str) -> Result<()> {
        let cancel_requested = match self.handle.read().await.as_ref() {
            Some(handle) => match handle.cancel_task(task_id.to_string()).await {
                Ok(()) => true,
                Err(e) => {
                    error!("Failed to request cancel for task {}: {}", task_id, e);
                    false
                }
            },
            None => false,
        };

        if let Ok(Some(task)) = self.core.storage.agent_tasks.get_task(task_id)
            && (task.status != AgentTaskStatus::Running || !cancel_requested)
        {
            self.core.storage.agent_tasks.pause_task(task_id)?;
        }

        Ok(())
    }

    async fn get_status(&self) -> Result<SystemStatus> {
        let runner_active = self.handle.read().await.is_some();
        let active_count = self.running_task_count().await;

        let tasks = self.core.storage.agent_tasks.list_tasks()?;
        let pending_count = tasks
            .iter()
            .filter(|t| t.status == AgentTaskStatus::Active)
            .count();

        let today_start = chrono::Utc::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .map(|dt| dt.and_utc().timestamp_millis())
            .unwrap_or(0);

        let completed_today = tasks
            .iter()
            .filter(|t| t.status == AgentTaskStatus::Completed && t.updated_at >= today_start)
            .count();

        Ok(SystemStatus {
            runner_active,
            active_count,
            pending_count,
            completed_today,
        })
    }

    async fn send_input_to_task(&self, task_id: &str, input: &str) -> Result<()> {
        info!(
            "Task input forwarding not yet implemented: {} -> {}",
            task_id, input
        );
        Ok(())
    }

    async fn handle_approval(&self, task_id: &str, approved: bool) -> Result<bool> {
        info!(
            "Task approval handling not yet implemented: {} approved={}",
            task_id, approved
        );
        Ok(false)
    }
}
