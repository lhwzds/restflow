use ai::agent::{SubagentConfig, SubagentTracker};
use anyhow::Result;
use async_trait::async_trait;
use runtime::AppCore;
use runtime::auth::{AuthManagerConfig, AuthProfileManager};
use runtime::daemon::publish_task_event;
use runtime::process::ProcessRegistry;
use runtime::runtime::task_runtime::TaskReplySenderFactory;
use runtime::runtime::{
    AgentRuntimeExecutor, NoopHeartbeatEmitter, OrchestratingAgentExecutor,
    StorageBackedSubagentLookup, TaskRunner, TaskRunnerConfig, TaskRunnerHandle,
};
use runtime::runtime::{TaskEventEmitter, TaskStreamEvent};
use runtime::services::session::SessionService;
use runtime::steer::SteerRegistry;
use runtime::storage::{AgentDefaults, AuthProfileStorage, SecretStorage, SystemConfig};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

struct TaskIpcEventEmitter;

#[async_trait]
impl TaskEventEmitter for TaskIpcEventEmitter {
    async fn emit(&self, event: TaskStreamEvent) {
        publish_task_event(event);
    }
}

pub struct CliTaskRunner {
    core: Arc<AppCore>,
    handle: Arc<RwLock<Option<Arc<TaskRunnerHandle>>>>,
    runner: Arc<RwLock<Option<Arc<TaskRunner>>>>,
}

fn create_auth_manager(
    secrets: Arc<SecretStorage>,
    profile_storage: AuthProfileStorage,
) -> Result<AuthProfileManager> {
    let config = AuthManagerConfig::default();
    Ok(AuthProfileManager::with_storage(
        config,
        secrets,
        Some(profile_storage),
    ))
}

impl CliTaskRunner {
    pub fn new(core: Arc<AppCore>) -> Self {
        Self {
            core,
            handle: Arc::new(RwLock::new(None)),
            runner: Arc::new(RwLock::new(None)),
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

        let auth_manager = Arc::new(create_auth_manager(
            secrets.clone(),
            AuthProfileStorage::new_namespace(storage.namespace())?,
        )?);
        auth_manager.initialize().await?;

        // Create task runtime components
        let (completion_tx, completion_rx) = tokio::sync::mpsc::channel(100);
        let subagent_tracker = Arc::new(SubagentTracker::new(completion_tx, completion_rx));
        let subagent_definitions =
            Arc::new(StorageBackedSubagentLookup::new(storage.agents.clone()));
        let task_config = build_task_config(&system_config.agent);
        let event_emitter: Arc<dyn TaskEventEmitter> = Arc::new(TaskIpcEventEmitter);

        let reply_sender_factory = Arc::new(TaskReplySenderFactory::new(
            Arc::new(storage.tasks.clone()),
            event_emitter.clone(),
        ));

        let executor = AgentRuntimeExecutor::new(
            storage.clone(),
            process_registry,
            auth_manager.clone(),
            subagent_tracker.clone(),
            subagent_definitions.clone(),
            task_config.clone(),
        )
        .with_reply_sender_factory(reply_sender_factory);
        let steer_registry = Arc::new(SteerRegistry::new());
        let runner = Arc::new(
            TaskRunner::with_heartbeat_emitter(
                Arc::new(storage.tasks.clone()),
                Arc::new(OrchestratingAgentExecutor::from_runtime_executor(executor)),
                build_runner_config(&system_config),
                Arc::new(NoopHeartbeatEmitter),
                steer_registry,
            )
            .with_event_emitter(event_emitter)
            .with_session_service(SessionService::from_storage(&storage)),
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

        Ok(())
    }
}

fn build_task_config(defaults: &AgentDefaults) -> SubagentConfig {
    SubagentConfig {
        max_parallel_agents: defaults.max_parallel_subagents,
        subagent_timeout_secs: defaults.subagent_timeout_secs,
        max_iterations: defaults.max_iterations,
        max_depth: defaults.max_depth,
    }
}

fn build_runner_config(system_config: &SystemConfig) -> TaskRunnerConfig {
    TaskRunnerConfig {
        poll_interval_ms: system_config.runtime_defaults.task_runner_poll_interval_ms,
        max_concurrent_tasks: system_config
            .runtime_defaults
            .task_runner_max_concurrent_tasks,
        worker_count: system_config.worker_count,
        task_timeout_secs: system_config.task_api_timeout_seconds,
        stall_timeout_secs: Some(system_config.stall_timeout_seconds),
    }
}

#[cfg(test)]
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::*;
    use runtime::storage::RuntimeDefaults;

    #[test]
    fn build_task_config_maps_max_iterations_from_agent_defaults() {
        let defaults = AgentDefaults {
            max_parallel_subagents: 20,
            subagent_timeout_secs: 1800,
            max_iterations: 99,
            max_depth: 3,
            ..AgentDefaults::default()
        };

        let config = build_task_config(&defaults);

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
            task_api_timeout_seconds: Some(1800),
            runtime_defaults: RuntimeDefaults {
                task_runner_poll_interval_ms: 12_000,
                task_runner_max_concurrent_tasks: 4,
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
}
