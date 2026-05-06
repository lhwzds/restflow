use anyhow::Result;
use async_trait::async_trait;
use restflow_contracts::{CleanupReportResponse, request::TaskFromSessionRequest};
use restflow_core::daemon::is_daemon_available;
use restflow_core::models::{
    AgentNode, ChatSession, ChatSessionSummary, ExecutionTimeline, RunListQuery, RunSummary,
    Secret, Skill, Task, TaskControlAction, TaskConversionResult, TaskPatch, TaskProgress,
    TaskSpec,
};
use restflow_core::paths;
use restflow_core::storage::SystemConfig;
use restflow_core::storage::agent::StoredAgent;
use std::sync::Arc;

#[cfg(test)]
pub mod direct;
pub mod ipc;

#[async_trait]
pub trait CommandExecutor: Send + Sync {
    async fn list_agents(&self) -> Result<Vec<StoredAgent>>;
    async fn get_agent(&self, id: &str) -> Result<StoredAgent>;
    async fn create_agent(&self, name: String, agent: AgentNode) -> Result<StoredAgent>;
    async fn update_agent(
        &self,
        id: &str,
        name: Option<String>,
        agent: Option<AgentNode>,
    ) -> Result<StoredAgent>;
    async fn delete_agent(&self, id: &str) -> Result<()>;

    async fn list_skills(&self) -> Result<Vec<Skill>>;
    async fn get_skill(&self, id: &str) -> Result<Option<Skill>>;

    async fn list_secrets(&self) -> Result<Vec<Secret>>;
    async fn set_secret(&self, key: &str, value: &str, description: Option<String>) -> Result<()>;
    #[allow(dead_code)]
    async fn create_secret(
        &self,
        key: &str,
        value: &str,
        description: Option<String>,
    ) -> Result<()>;
    #[allow(dead_code)]
    async fn update_secret(
        &self,
        key: &str,
        value: &str,
        description: Option<String>,
    ) -> Result<()>;
    async fn delete_secret(&self, key: &str) -> Result<()>;
    async fn has_secret(&self, key: &str) -> Result<bool>;

    async fn get_config(&self) -> Result<SystemConfig>;
    async fn get_global_config(&self) -> Result<SystemConfig>;
    async fn set_config(&self, config: SystemConfig) -> Result<()>;

    async fn run_cleanup(&self) -> Result<CleanupReportResponse>;

    // Session operations
    async fn list_sessions(&self) -> Result<Vec<ChatSessionSummary>>;
    async fn list_full_sessions(&self) -> Result<Vec<ChatSession>>;
    async fn get_session(&self, id: &str) -> Result<ChatSession>;
    async fn create_session(
        &self,
        agent_id: Option<String>,
        model: Option<String>,
        name: Option<String>,
        skill_id: Option<String>,
    ) -> Result<ChatSession>;
    async fn delete_session(&self, id: &str) -> Result<bool>;

    // Task operations
    async fn list_tasks(&self, status: Option<String>) -> Result<Vec<Task>>;
    async fn get_task(&self, id: &str) -> Result<Task>;
    async fn create_task(&self, spec: TaskSpec) -> Result<Task>;
    async fn convert_session_to_task(
        &self,
        request: TaskFromSessionRequest,
    ) -> Result<TaskConversionResult>;
    async fn update_task(&self, id: &str, patch: TaskPatch) -> Result<Task>;
    async fn delete_task(&self, id: &str) -> Result<restflow_contracts::DeleteWithIdResponse>;
    async fn control_task(&self, id: &str, action: TaskControlAction) -> Result<Task>;
    async fn get_task_progress(&self, id: &str, event_limit: Option<usize>)
    -> Result<TaskProgress>;
    async fn send_task_message(&self, id: &str, message: &str) -> Result<()>;
    async fn list_runs(&self, query: RunListQuery) -> Result<Vec<RunSummary>>;
    async fn get_execution_run_timeline(&self, run_id: &str) -> Result<ExecutionTimeline>;
}

pub async fn create(db_path: Option<String>) -> Result<Arc<dyn CommandExecutor>> {
    if let Some(db_path) = db_path {
        anyhow::bail!(
            "The --db-path flag is only supported for daemon lifecycle commands. Commands routed through the daemon must target the running daemon instance instead of selecting a database path directly: {}",
            db_path
        );
    }

    // This is the only production executor entrypoint for daemon-routed commands.
    let socket_path = paths::socket_path()?;
    if is_daemon_available(&socket_path).await {
        let executor = ipc::IpcExecutor::connect(&socket_path).await?;
        return Ok(Arc::new(executor));
    }

    anyhow::bail!("RestFlow daemon is not running. Start it with 'restflow daemon start'.")
}

#[cfg(test)]
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::test_support::env_lock()
    }

    #[tokio::test]
    async fn create_requires_running_daemon() {
        let _guard = env_lock();
        let temp = tempdir().expect("tempdir");
        let prev = std::env::var_os("RESTFLOW_DIR");
        unsafe { std::env::set_var("RESTFLOW_DIR", temp.path()) };

        let err = match create(None).await {
            Ok(_) => panic!("create should fail without daemon"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("daemon is not running"));
        assert!(
            err.to_string()
                .contains("Start it with 'restflow daemon start'")
        );

        match prev {
            Some(value) => unsafe { std::env::set_var("RESTFLOW_DIR", value) },
            None => unsafe { std::env::remove_var("RESTFLOW_DIR") },
        }
    }

    #[tokio::test]
    async fn create_rejects_db_path_for_executor_commands() {
        let err = match create(Some("/tmp/restflow.db".to_string())).await {
            Ok(_) => panic!("create should reject db_path for daemon-routed commands"),
            Err(err) => err,
        };
        assert!(
            err.to_string()
                .contains("only supported for daemon lifecycle commands")
        );
    }
}
