use anyhow::Result;
use tokio::sync::mpsc;
use types::{DEFAULT_TASK_RUNNER_MAX_CONCURRENT_TASKS, DEFAULT_TASK_RUNNER_POLL_INTERVAL_MS};

use crate::models::SteerMessage;
use crate::runtime::task_runtime::outcome::ExecutionOutcome;
use ai::agent::StreamEmitter;

pub type ExecutionResult = ExecutionOutcome;

#[derive(Debug, Clone)]
pub struct TaskRunnerConfig {
    pub poll_interval_ms: u64,
    pub max_concurrent_tasks: usize,
    pub worker_count: usize,
    pub task_timeout_secs: Option<u64>,
    pub stall_timeout_secs: Option<u64>,
}

impl Default for TaskRunnerConfig {
    fn default() -> Self {
        Self {
            poll_interval_ms: DEFAULT_TASK_RUNNER_POLL_INTERVAL_MS,
            max_concurrent_tasks: DEFAULT_TASK_RUNNER_MAX_CONCURRENT_TASKS,
            worker_count: DEFAULT_TASK_RUNNER_MAX_CONCURRENT_TASKS,
            task_timeout_secs: None,
            stall_timeout_secs: None,
        }
    }
}

pub struct TaskRunnerHandle;

impl TaskRunnerHandle {
    pub async fn stop(&self) -> Result<()> {
        Ok(())
    }

    pub async fn check_now(&self) -> Result<()> {
        Ok(())
    }

    pub async fn run_task_now(&self, _task_id: String) -> Result<()> {
        Ok(())
    }

    pub async fn stop_task(&self, _task_id: String) -> Result<()> {
        Ok(())
    }
}

pub struct TaskRunner;

#[async_trait::async_trait]
pub trait AgentExecutor: Send + Sync {
    #[allow(clippy::too_many_arguments)]
    async fn execute(
        &self,
        agent_id: &str,
        task_id: Option<&str>,
        input: Option<&str>,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
    ) -> Result<ExecutionResult>;
}
