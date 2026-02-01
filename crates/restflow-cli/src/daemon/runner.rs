use anyhow::Result;
use restflow_core::AppCore;
use restflow_tauri_lib::{
    AgentTaskRunner, RealAgentExecutor, RunnerConfig, RunnerHandle, TelegramNotifier,
};
use std::sync::Arc;

use super::TelegramAgentHandle;

pub struct CliTaskRunner {
    core: Arc<AppCore>,
    handle: Option<RunnerHandle>,
    telegram_handle: Option<TelegramAgentHandle>,
}

impl CliTaskRunner {
    pub fn new(core: Arc<AppCore>) -> Self {
        Self {
            core,
            handle: None,
            telegram_handle: None,
        }
    }

    pub async fn start(&mut self) -> Result<()> {
        if self.handle.is_some() {
            anyhow::bail!("Runner already started");
        }

        let storage = self.core.storage.clone();
        let secrets = Arc::new(self.core.storage.secrets.clone());

        let executor = RealAgentExecutor::new(storage.clone());
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

        let handle = runner.start();
        self.handle = Some(handle);

        if self.telegram_handle.is_none() {
            match super::TelegramAgent::from_storage(self.core.storage.clone()) {
                Ok(Some(agent)) => {
                    self.telegram_handle = Some(agent.start());
                }
                Ok(None) => {
                    tracing::info!("Telegram agent disabled: TELEGRAM_BOT_TOKEN not configured");
                }
                Err(err) => {
                    tracing::warn!("Failed to start Telegram agent: {}", err);
                }
            }
        }

        tracing::info!("Task runner started");
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        if let Some(handle) = self.handle.take() {
            handle.stop().await?;
            tracing::info!("Task runner stopped");
        }

        if let Some(handle) = self.telegram_handle.take() {
            handle.stop().await;
            tracing::info!("Telegram agent stopped");
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub fn is_running(&self) -> bool {
        self.handle.is_some()
    }

    pub async fn run_task_now(&self, task_id: &str) -> Result<()> {
        if let Some(handle) = &self.handle {
            handle.run_task_now(task_id.to_string()).await?;
        } else {
            anyhow::bail!("Runner not started");
        }
        Ok(())
    }
}
