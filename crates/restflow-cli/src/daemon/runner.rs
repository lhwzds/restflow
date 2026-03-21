use anyhow::Result;
use async_trait::async_trait;
use restflow_core::AppCore;
use restflow_core::auth::{AuthManagerConfig, AuthProfileManager};
use restflow_core::channel::{ChannelRouter, PairingManager};
use restflow_core::daemon::publish_background_event;
use restflow_core::hooks::HookExecutor;
use restflow_core::models::{
    BackgroundAgent, BackgroundAgentControlAction, BackgroundAgentStatus, BackgroundMessageSource,
};
use restflow_core::paths;
use restflow_core::process::ProcessRegistry;
use restflow_core::runtime::background_agent::BackgroundReplySenderFactory;
use restflow_core::runtime::channel::start_message_handler_with_pairing;
use restflow_core::runtime::{
    AgentRuntimeExecutor, BackgroundAgentRunner, BackgroundAgentTrigger, ChatDispatcher,
    ChatDispatcherConfig, ChatSessionManager, MessageDebouncer, MessageHandlerConfig,
    MessageHandlerHandle, NoopHeartbeatEmitter, OrchestratingAgentExecutor, RunnerConfig,
    RunnerHandle, StorageBackedSubagentLookup, SubagentConfig, SubagentTracker, SystemStatus,
    TelegramNotifier,
};
use restflow_core::runtime::{TaskEventEmitter, TaskStreamEvent};
use restflow_core::steer::SteerRegistry;
use restflow_core::storage::{SecretStorage, SystemConfig};
use restflow_storage::{AgentDefaults, AuthProfileStorage};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use super::{discord, slack, telegram};

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
    message_handler: Option<MessageHandlerHandle>,
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
            message_handler: None,
        }
    }

    pub async fn start(&mut self) -> Result<()> {
        if self.handle.read().await.is_some() {
            anyhow::bail!("Runner already started");
        }

        let storage = self.core.storage.clone();
        let secrets = Arc::new(self.core.storage.secrets.clone());
        let system_config = storage.config.get_effective_config()?;
        let process_registry = Arc::new(
            ProcessRegistry::new().with_ttl_seconds(system_config.agent.process_session_ttl_secs),
        );

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
        let subagent_tracker = Arc::new(SubagentTracker::new(completion_tx, completion_rx));
        subagent_tracker.set_telemetry_sink(restflow_core::telemetry::build_core_telemetry_sink(
            storage.as_ref(),
        ));
        let subagent_definitions =
            Arc::new(StorageBackedSubagentLookup::new(storage.agents.clone()));
        let subagent_config = build_subagent_config(&system_config.agent);
        let event_emitter: Arc<dyn TaskEventEmitter> = Arc::new(DaemonIpcEventEmitter);
        let channel_router = Arc::new(RwLock::new(None));

        let reply_sender_factory = Arc::new(BackgroundReplySenderFactory::new(
            Arc::new(storage.background_agents.clone()),
            event_emitter.clone(),
            channel_router.clone(),
        ));

        let executor = AgentRuntimeExecutor::new(
            storage.clone(),
            process_registry,
            auth_manager.clone(),
            subagent_tracker.clone(),
            subagent_definitions.clone(),
            subagent_config.clone(),
        )
        .with_reply_sender_factory(reply_sender_factory);
        let notifier = TelegramNotifier::new(secrets);
        let steer_registry = Arc::new(SteerRegistry::new());
        let hook_executor = Arc::new(HookExecutor::with_storage(storage.hooks.clone()));

        let runner = Arc::new(
            BackgroundAgentRunner::with_memory_persistence(
                Arc::new(storage.background_agents.clone()),
                Arc::new(OrchestratingAgentExecutor::from_runtime_executor(executor)),
                Arc::new(notifier),
                build_runner_config(&system_config),
                Arc::new(NoopHeartbeatEmitter),
                storage.memory.clone(),
                steer_registry,
            )
            .with_event_emitter(event_emitter)
            .with_channel_router_handle(channel_router.clone())
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

        // Build a shared ChannelRouter from all configured channels
        let mut channel_router = ChannelRouter::new();
        let mut any_channel_configured = false;

        // Try Telegram
        if let Some((tg_channel, default_chat_id)) = telegram::setup_telegram_channel(
            &self.core.storage.secrets,
            &self.core.storage.daemon_state,
            &system_config.channel_defaults,
        )? {
            if let Some(chat_id) = default_chat_id {
                channel_router.register_with_default(tg_channel, chat_id);
            } else {
                warn!(
                    "Telegram channel registered without default chat ID. Set TELEGRAM_CHAT_ID for reliable notifications."
                );
                channel_router.register(tg_channel);
            }
            any_channel_configured = true;
            info!("Telegram channel configured");
        }

        // Try Discord
        if let Some((dc_channel, default_channel_id)) =
            discord::setup_discord_channel(&self.core.storage.secrets)?
        {
            if let Some(channel_id) = default_channel_id {
                channel_router.register_with_default(dc_channel, channel_id);
            } else {
                channel_router.register(dc_channel);
            }
            any_channel_configured = true;
            info!("Discord channel configured");
        }

        // Try Slack
        if let Some((sk_channel, default_channel_id)) =
            slack::setup_slack_channel(&self.core.storage.secrets)?
        {
            if let Some(channel_id) = default_channel_id {
                channel_router.register_with_default(sk_channel, channel_id);
            } else {
                channel_router.register(sk_channel);
            }
            any_channel_configured = true;
            info!("Slack channel configured");
        }

        if any_channel_configured {
            let router = Arc::new(channel_router);

            let trigger = Arc::new(CliBackgroundAgentTrigger::new(
                self.core.clone(),
                self.handle.clone(),
                self.runner.clone(),
            ));

            // Create ChatDispatcher for AI conversations
            let default_chat_agent_id = storage.agents.resolve_default_agent_id()?;
            let session_manager = Arc::new(
                ChatSessionManager::new(
                    storage.clone(),
                    system_config.runtime_defaults.chat_max_session_history,
                )
                .with_default_agent(default_chat_agent_id),
            );
            let debouncer = Arc::new(MessageDebouncer::default_timeout());
            let chat_dispatcher_config = ChatDispatcherConfig {
                max_session_history: system_config.runtime_defaults.chat_max_session_history,
                response_timeout_secs: system_config.chat_response_timeout_seconds,
                ..ChatDispatcherConfig::default()
            };
            let chat_dispatcher = Arc::new(ChatDispatcher::new(
                session_manager,
                storage.clone(),
                auth_manager.clone(),
                debouncer,
                router.clone(),
                chat_dispatcher_config,
                subagent_tracker.clone(),
                subagent_definitions.clone(),
                subagent_config.clone(),
            ));

            let pairing_manager = Arc::new(PairingManager::new(Arc::new(storage.pairing.clone())));
            bootstrap_default_chat_pairing(&storage.secrets, pairing_manager.as_ref())?;

            let msg_handle = start_message_handler_with_pairing(
                router.clone(),
                trigger,
                Some(chat_dispatcher),
                pairing_manager,
                MessageHandlerConfig {
                    pairing_enabled: true,
                    ..MessageHandlerConfig::default()
                },
            );
            self.message_handler = Some(msg_handle);

            if let Some(ref runner) = *self.runner.read().await {
                runner.set_channel_router(router.clone()).await;
            }

            let mut router_guard = self.router.write().await;
            *router_guard = Some(router);
            info!("Channel message handler started with pairing access control");
        }

        info!("Task runner started");
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        if let Some(msg_handle) = self.message_handler.take() {
            msg_handle.shutdown();
            info!("Message handler stopped");
        }

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
}

fn build_subagent_config(defaults: &AgentDefaults) -> SubagentConfig {
    SubagentConfig {
        max_parallel_agents: defaults.max_parallel_subagents,
        subagent_timeout_secs: defaults.subagent_timeout_secs,
        max_iterations: defaults.max_iterations,
        max_depth: defaults.max_depth,
    }
}

fn build_runner_config(system_config: &SystemConfig) -> RunnerConfig {
    RunnerConfig {
        poll_interval_ms: system_config
            .runtime_defaults
            .background_runner_poll_interval_ms,
        max_concurrent_tasks: system_config
            .runtime_defaults
            .background_runner_max_concurrent_tasks,
        worker_count: system_config.worker_count,
        task_timeout_secs: system_config.background_api_timeout_seconds,
        stall_timeout_secs: Some(system_config.stall_timeout_seconds),
    }
}

fn bootstrap_default_chat_pairing(
    secrets: &SecretStorage,
    pairing_manager: &PairingManager,
) -> Result<()> {
    let default_chat_id = secrets
        .get_non_empty("TELEGRAM_CHAT_ID")?
        .or(secrets.get_non_empty("TELEGRAM_DEFAULT_CHAT_ID")?);

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
        let stop_requested = match self.handle.read().await.as_ref() {
            Some(handle) => match handle.stop_task(task_id.to_string()).await {
                Ok(()) => true,
                Err(e) => {
                    error!("Failed to request stop for task {}: {}", task_id, e);
                    false
                }
            },
            None => false,
        };

        if let Ok(Some(task)) = self.core.storage.background_agents.get_task(task_id)
            && (task.status != BackgroundAgentStatus::Running || !stop_requested)
        {
            self.core
                .storage
                .background_agents
                .control_background_agent(task_id, BackgroundAgentControlAction::Stop)?;
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
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::*;
    use restflow_core::models::{
        BackgroundAgentSpec, MemoryConfig, NotificationConfig, TaskSchedule,
    };
    use restflow_storage::RuntimeDefaults;
    use std::ffi::OsString;
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    #[test]
    fn build_subagent_config_maps_max_iterations_from_agent_defaults() {
        let defaults = AgentDefaults {
            max_parallel_subagents: 20,
            subagent_timeout_secs: 1800,
            max_iterations: 99,
            max_depth: 3,
            ..AgentDefaults::default()
        };

        let config = build_subagent_config(&defaults);

        assert_eq!(config.max_parallel_agents, 20);
        assert_eq!(config.subagent_timeout_secs, 1800);
        assert_eq!(config.max_iterations, 99);
        assert_eq!(config.max_depth, 3);
    }

    #[test]
    fn build_runner_config_maps_worker_and_stall_limits() {
        let system_config = SystemConfig {
            worker_count: 6,
            stall_timeout_seconds: 900,
            background_api_timeout_seconds: Some(1800),
            runtime_defaults: RuntimeDefaults {
                background_runner_poll_interval_ms: 12_000,
                background_runner_max_concurrent_tasks: 4,
                ..RuntimeDefaults::default()
            },
            ..SystemConfig::default()
        };

        let config = build_runner_config(&system_config);

        assert_eq!(config.poll_interval_ms, 12_000);
        assert_eq!(config.max_concurrent_tasks, 4);
        assert_eq!(config.worker_count, 6);
        assert_eq!(config.task_timeout_secs, Some(1800));
        assert_eq!(config.stall_timeout_secs, Some(900));
    }

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    struct EnvGuard {
        key: &'static str,
        original: Option<OsString>,
    }

    impl EnvGuard {
        fn set_path(key: &'static str, value: &std::path::Path) -> Self {
            let original = std::env::var_os(key);
            unsafe { std::env::set_var(key, value) };
            Self { key, original }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.original {
                Some(value) => unsafe { std::env::set_var(self.key, value) },
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
    }

    async fn setup_trigger_with_background_agent() -> (
        Arc<AppCore>,
        CliBackgroundAgentTrigger,
        BackgroundAgent,
        tempfile::TempDir,
        EnvGuard,
        EnvGuard,
        std::sync::MutexGuard<'static, ()>,
    ) {
        let env_lock = env_lock();
        let temp_dir = tempdir().expect("failed to create temp dir");
        let restflow_dir_guard = EnvGuard::set_path("RESTFLOW_DIR", temp_dir.path());
        let agents_dir = temp_dir.path().join("agents");
        std::fs::create_dir_all(&agents_dir).expect("failed to create agents dir");
        let agents_dir_guard = EnvGuard::set_path("RESTFLOW_AGENTS_DIR", &agents_dir);
        let db_path = temp_dir.path().join("runner-test.db");
        let core = Arc::new(
            AppCore::new(db_path.to_str().expect("invalid db path"))
                .await
                .expect("failed to initialize core"),
        );

        let default_agent = core
            .storage
            .agents
            .resolve_default_agent()
            .expect("default agent missing");

        let task = core
            .storage
            .background_agents
            .create_background_agent(BackgroundAgentSpec {
                name: "Background Agent Test".to_string(),
                agent_id: default_agent.id,
                chat_session_id: None,
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

        (
            core,
            trigger,
            task,
            temp_dir,
            restflow_dir_guard,
            agents_dir_guard,
            env_lock,
        )
    }

    #[tokio::test]
    async fn send_input_to_task_enqueues_user_message() {
        let (core, trigger, task, _temp_dir, _restflow_dir_guard, _agents_dir_guard, _env_lock) =
            setup_trigger_with_background_agent().await;

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
        let (core, trigger, task, _temp_dir, _restflow_dir_guard, _agents_dir_guard, _env_lock) =
            setup_trigger_with_background_agent().await;

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
