use anyhow::Result;
use async_trait::async_trait;
use restflow_core::AppCore;
use restflow_core::auth::{AuthManagerConfig, AuthProfileManager};
use restflow_core::channel::{ChannelRouter, PairingManager};
use restflow_core::daemon::publish_background_event;
use restflow_core::hooks::HookExecutor;
use restflow_core::models::{BackgroundAgent, BackgroundAgentStatus, BackgroundMessageSource};
use restflow_core::paths;
use restflow_core::process::ProcessRegistry;
use restflow_core::runtime::channel::start_message_handler_with_pairing;
use restflow_core::runtime::{
    AgentDefinitionRegistry, AgentRuntimeExecutor, BackgroundAgentRunner, BackgroundAgentTrigger,
    ChatDispatcher, ChatDispatcherConfig, ChatSessionManager, MessageDebouncer,
    MessageHandlerConfig, NoopHeartbeatEmitter, RunnerConfig, RunnerHandle, SubagentConfig,
    SubagentTracker, SystemStatus, TelegramNotifier,
};
use restflow_core::runtime::{TaskEventEmitter, TaskStreamEvent};
use restflow_core::steer::SteerRegistry;
use restflow_core::storage::SecretStorage;
use restflow_storage::AuthProfileStorage;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

use super::telegram;

struct DaemonIpcEventEmitter;

#[async_trait]
impl TaskEventEmitter for DaemonIpcEventEmitter {
    async fn emit(&self, event: TaskStreamEvent) {
        publish_background_event(event);
    }
}

pub struct CliBackgroundAgentRunner {
    core: Arc<AppCore>,
    handle: Arc<RwLock<Option<Arc<RunnerHandle>>>>,
    runner: Arc<RwLock<Option<Arc<BackgroundAgentRunner>>>>,
    router: Arc<RwLock<Option<Arc<ChannelRouter>>>>,
}

fn create_auth_manager(
    secrets: Arc<SecretStorage>,
    db: Arc<redb::Database>,
) -> Result<AuthProfileManager> {
    let config = AuthManagerConfig::default();
    let storage = AuthProfileStorage::new(db)?;
    Ok(AuthProfileManager::with_storage(
        config,
        secrets,
        Some(storage),
    ))
}

impl CliBackgroundAgentRunner {
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

        let auth_manager = Arc::new(create_auth_manager(secrets.clone(), storage.get_db())?);
        if let Ok(data_dir) = paths::ensure_restflow_dir() {
            let old_json = data_dir.join("auth_profiles.json");
            if let Err(e) = auth_manager.migrate_from_json(&old_json).await {
                tracing::warn!(error = %e, "Failed to migrate auth profiles from JSON");
            }
        }
        auth_manager.initialize().await?;
        auth_manager.discover().await?;

        // Create subagent system components
        let (completion_tx, completion_rx) = tokio::sync::mpsc::channel(100);
        let subagent_tracker = Arc::new(SubagentTracker::new(completion_tx, completion_rx, subagent_config.max_parallel_agents));
        let subagent_definitions = Arc::new(AgentDefinitionRegistry::with_builtins());
        let subagent_config = SubagentConfig::default();

        let executor = AgentRuntimeExecutor::new(
            storage.clone(),
            process_registry,
            auth_manager.clone(),
            subagent_tracker.clone(),
            subagent_definitions.clone(),
            subagent_config.clone(),
        );
        let notifier = TelegramNotifier::new(secrets);
        let steer_registry = Arc::new(SteerRegistry::new());
        let system_config = storage.config.get_config()?.unwrap_or_default();

        let hook_executor = Arc::new(HookExecutor::with_storage(storage.hooks.clone()));

        let runner = Arc::new(
            BackgroundAgentRunner::with_memory_persistence(
                Arc::new(storage.background_agents.clone()),
                Arc::new(executor),
                Arc::new(notifier),
                RunnerConfig {
                    poll_interval_ms: 30_000,
                    max_concurrent_tasks: 5,
                    task_timeout_secs: system_config.background_api_timeout_seconds,
                },
                Arc::new(NoopHeartbeatEmitter),
                storage.memory.clone(),
                steer_registry,
            )
            .with_event_emitter(Arc::new(DaemonIpcEventEmitter))
            .with_hook_executor(hook_executor),
        );

        let handle = runner.clone().start();

        {
            let mut handle_guard = self.handle.write().await;
            *handle_guard = Some(Arc::new(handle));
        }

        {
            let mut runner_guard = self.runner.write().await;
            *runner_guard = Some(runner);
        }

        if let Some(router) = telegram::setup_telegram_channel(
            &self.core.storage.secrets,
            &self.core.storage.daemon_state,
        )? {
            let trigger = Arc::new(CliBackgroundAgentTrigger::new(
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
                subagent_tracker.clone(),
                subagent_definitions.clone(),
                subagent_config.clone(),
            ));

            let pairing_manager = Arc::new(PairingManager::new(Arc::new(storage.pairing.clone())));
            bootstrap_default_chat_pairing(&storage.secrets, pairing_manager.as_ref())?;

            start_message_handler_with_pairing(
                router.clone(),
                trigger,
                Some(chat_dispatcher),
                pairing_manager,
                MessageHandlerConfig {
                    pairing_enabled: true,
                    ..MessageHandlerConfig::default()
                },
            );

            // Pass channel router to task runner so notifications are broadcast
            // through configured channels automatically (no per-task config needed)
            if let Some(ref runner) = *self.runner.read().await {
                runner.set_channel_router(router.clone()).await;
            }

            let mut router_guard = self.router.write().await;
            *router_guard = Some(router);
            info!(
                "Telegram channel enabled for CLI daemon with AI chat support and forced pairing access control"
            );
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
}

fn non_empty_secret(secrets: &SecretStorage, key: &str) -> Result<Option<String>> {
    Ok(secrets
        .get_secret(key)?
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty()))
}

fn bootstrap_default_chat_pairing(
    secrets: &SecretStorage,
    pairing_manager: &PairingManager,
) -> Result<()> {
    let default_chat_id = non_empty_secret(secrets, "TELEGRAM_CHAT_ID")?
        .or(non_empty_secret(secrets, "TELEGRAM_DEFAULT_CHAT_ID")?);

    let Some(chat_id) = default_chat_id else {
        return Ok(());
    };

    if pairing_manager.is_allowed(&chat_id)? {
        return Ok(());
    }

    // Add default chat as allowed to avoid locking out existing owner
    // when pairing is force-enabled.
    pairing_manager.allow_peer(&chat_id, Some("bootstrap-default-chat"), "daemon-bootstrap")?;
    info!(
        "Bootstrap pairing: auto-approved TELEGRAM_CHAT_ID as allowed peer ({})",
        chat_id
    );
    Ok(())
}

struct CliBackgroundAgentTrigger {
    core: Arc<AppCore>,
    handle: Arc<RwLock<Option<Arc<RunnerHandle>>>>,
    runner: Arc<RwLock<Option<Arc<BackgroundAgentRunner>>>>,
}

impl CliBackgroundAgentTrigger {
    fn new(
        core: Arc<AppCore>,
        handle: Arc<RwLock<Option<Arc<RunnerHandle>>>>,
        runner: Arc<RwLock<Option<Arc<BackgroundAgentRunner>>>>,
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
impl BackgroundAgentTrigger for CliBackgroundAgentTrigger {
    async fn list_background_agents(&self) -> Result<Vec<BackgroundAgent>> {
        self.core.storage.background_agents.list_tasks()
    }

    async fn find_and_run_background_agent(&self, name_or_id: &str) -> Result<BackgroundAgent> {
        if let Ok(Some(task)) = self.core.storage.background_agents.get_task(name_or_id) {
            self.runner_handle()
                .await?
                .run_task_now(task.id.clone())
                .await?;
            return Ok(task);
        }

        let tasks = self.core.storage.background_agents.list_tasks()?;
        let task = tasks
            .into_iter()
            .find(|t| t.name.eq_ignore_ascii_case(name_or_id))
            .ok_or_else(|| anyhow::anyhow!("Background agent not found: {}", name_or_id))?;

        self.runner_handle()
            .await?
            .run_task_now(task.id.clone())
            .await?;
        Ok(task)
    }

    async fn stop_background_agent(&self, task_id: &str) -> Result<()> {
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

        if let Ok(Some(task)) = self.core.storage.background_agents.get_task(task_id)
            && (task.status != BackgroundAgentStatus::Running || !cancel_requested)
        {
            self.core.storage.background_agents.pause_task(task_id)?;
        }

        Ok(())
    }

    async fn get_status(&self) -> Result<SystemStatus> {
        let runner_active = self.handle.read().await.is_some();
        let active_count = self.running_task_count().await;

        let tasks = self.core.storage.background_agents.list_tasks()?;
        let pending_count = tasks
            .iter()
            .filter(|t| t.status == BackgroundAgentStatus::Active)
            .count();

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
        self.core
            .storage
            .background_agents
            .send_background_agent_message(
                task_id,
                input.to_string(),
                BackgroundMessageSource::User,
            )?;
        Ok(())
    }

    async fn handle_background_agent_approval(
        &self,
        task_id: &str,
        approved: bool,
    ) -> Result<bool> {
        let message = if approved {
            "User approved the pending action."
        } else {
            "User rejected the pending action."
        };
        self.core
            .storage
            .background_agents
            .send_background_agent_message(
                task_id,
                message.to_string(),
                BackgroundMessageSource::System,
            )?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_core::models::{
        BackgroundAgentSpec, MemoryConfig, NotificationConfig, TaskSchedule,
    };
    use tempfile::tempdir;

    async fn setup_trigger_with_background_agent() -> (
        Arc<AppCore>,
        CliBackgroundAgentTrigger,
        BackgroundAgent,
        tempfile::TempDir,
    ) {
        let temp_dir = tempdir().expect("failed to create temp dir");
        let db_path = temp_dir.path().join("runner-test.db");
        let core = Arc::new(
            AppCore::new(db_path.to_str().expect("invalid db path"))
                .await
                .expect("failed to initialize core"),
        );

        let default_agent = core
            .storage
            .agents
            .list_agents()
            .expect("failed to list agents")
            .into_iter()
            .next()
            .expect("default agent missing");

        let task = core
            .storage
            .background_agents
            .create_background_agent(BackgroundAgentSpec {
                name: "Background Agent Test".to_string(),
                agent_id: default_agent.id,
                description: Some("test".to_string()),
                input: Some("hello".to_string()),
                input_template: None,
                schedule: TaskSchedule::default(),
                notification: Some(NotificationConfig::default()),
                execution_mode: None,
                timeout_secs: None,
                memory: Some(MemoryConfig::default()),
                durability_mode: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("failed to create background agent");

        let trigger = CliBackgroundAgentTrigger::new(
            core.clone(),
            Arc::new(RwLock::new(None)),
            Arc::new(RwLock::new(None)),
        );

        (core, trigger, task, temp_dir)
    }

    #[tokio::test]
    async fn send_input_to_task_enqueues_user_message() {
        let (core, trigger, task, _temp_dir) = setup_trigger_with_background_agent().await;

        trigger
            .send_message_to_background_agent(&task.id, "hello from main agent")
            .await
            .expect("failed to send input");

        let messages = core
            .storage
            .background_agents
            .list_background_agent_messages(&task.id, 10)
            .expect("failed to list background messages");

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].source, BackgroundMessageSource::User);
        assert_eq!(messages[0].message, "hello from main agent");
    }

    #[tokio::test]
    async fn handle_approval_falls_back_to_system_message_injection() {
        let (core, trigger, task, _temp_dir) = setup_trigger_with_background_agent().await;

        let handled = trigger
            .handle_background_agent_approval(&task.id, true)
            .await
            .expect("approval handling failed");
        assert!(handled);

        let messages = core
            .storage
            .background_agents
            .list_background_agent_messages(&task.id, 10)
            .expect("failed to list background messages");

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].source, BackgroundMessageSource::System);
        assert!(messages[0].message.contains("approved"));
    }
}
