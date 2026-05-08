use anyhow::{Result, bail};
use async_trait::async_trait;
use std::sync::Arc;

use crate::executor::CommandExecutor;
use crate::setup;
use restflow_core::models::{
    AgentNode, ChatSession, ChatSessionSummary, ExecutionTimeline, ExecutionTraceQuery,
    RunListQuery, RunSummary, Task, TaskControlAction, TaskConversionResult, TaskPatch,
    TaskProgress, TaskSpec,
};
use restflow_core::services::{
    agent as agent_service, config as config_service, execution_console::ExecutionConsoleService,
    secrets as secrets_service, session::SessionService, skills as skills_service,
};
use restflow_core::storage::SystemConfig;
use restflow_core::storage::agent::StoredAgent;
use restflow_core::{
    AppCore,
    models::{Secret, Skill},
};
use restflow_traits::{CleanupReportResponse, request::TaskFromSessionRequest};
/// Test-only executor used by command unit tests.
pub struct DirectExecutor {
    core: Arc<AppCore>,
}

impl DirectExecutor {
    pub async fn connect(db_path: Option<String>) -> Result<Self> {
        let core = setup::prepare_core(db_path).await?;
        Ok(Self { core })
    }
}

#[async_trait]
impl CommandExecutor for DirectExecutor {
    async fn list_agents(&self) -> Result<Vec<StoredAgent>> {
        agent_service::list_agents(&self.core).await
    }

    async fn get_agent(&self, id: &str) -> Result<StoredAgent> {
        agent_service::get_agent(&self.core, id).await
    }

    async fn create_agent(&self, name: String, agent: AgentNode) -> Result<StoredAgent> {
        agent_service::create_agent(&self.core, name, agent).await
    }

    async fn update_agent(
        &self,
        id: &str,
        name: Option<String>,
        agent: Option<AgentNode>,
    ) -> Result<StoredAgent> {
        agent_service::update_agent(&self.core, id, name, agent).await
    }

    async fn delete_agent(&self, id: &str) -> Result<()> {
        agent_service::delete_agent(&self.core, id).await
    }

    async fn list_skills(&self) -> Result<Vec<Skill>> {
        skills_service::list_skills(&self.core).await
    }

    async fn get_skill(&self, id: &str) -> Result<Option<Skill>> {
        skills_service::get_skill(&self.core, id).await
    }

    async fn list_secrets(&self) -> Result<Vec<Secret>> {
        secrets_service::list_secrets(&self.core).await
    }

    async fn set_secret(&self, key: &str, value: &str, description: Option<String>) -> Result<()> {
        secrets_service::set_secret(&self.core, key, value, description).await
    }

    async fn create_secret(
        &self,
        key: &str,
        value: &str,
        description: Option<String>,
    ) -> Result<()> {
        secrets_service::create_secret(&self.core, key, value, description).await
    }

    async fn update_secret(
        &self,
        key: &str,
        value: &str,
        description: Option<String>,
    ) -> Result<()> {
        secrets_service::update_secret(&self.core, key, value, description).await
    }

    async fn delete_secret(&self, key: &str) -> Result<()> {
        secrets_service::delete_secret(&self.core, key).await
    }

    async fn has_secret(&self, key: &str) -> Result<bool> {
        Ok(secrets_service::get_secret(&self.core, key)
            .await?
            .is_some())
    }

    async fn get_config(&self) -> Result<SystemConfig> {
        config_service::get_config(&self.core).await
    }

    async fn get_global_config(&self) -> Result<SystemConfig> {
        config_service::get_global_config(&self.core).await
    }

    async fn set_config(&self, config: SystemConfig) -> Result<()> {
        config_service::update_config(&self.core, config).await
    }

    async fn run_cleanup(&self) -> Result<CleanupReportResponse> {
        let report = restflow_core::services::cleanup::run_cleanup(&self.core).await?;
        Ok(CleanupReportResponse {
            chat_sessions: report.chat_sessions,
            tasks: report.tasks,
            audit_events: report.audit_events,
            telemetry_metric_samples: 0,
            daemon_log_files: report.daemon_log_files,
        })
    }

    async fn list_sessions(&self) -> Result<Vec<ChatSessionSummary>> {
        Ok(SessionService::from_storage(&self.core.storage)
            .list_session_views(None, None, false)?
            .iter()
            .map(ChatSessionSummary::from)
            .collect())
    }

    async fn get_session(&self, id: &str) -> Result<ChatSession> {
        SessionService::from_storage(&self.core.storage)
            .get_session_view(id)?
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", id))
    }

    async fn search_sessions(
        &self,
        query: &str,
        agent_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<ChatSessionSummary>> {
        Ok(SessionService::from_storage(&self.core.storage)
            .search_session_views(query, agent_id, None, false, limit.max(1))?
            .iter()
            .map(ChatSessionSummary::from)
            .collect())
    }

    async fn create_session(
        &self,
        agent_id: Option<String>,
        model: Option<String>,
        name: Option<String>,
        skill_id: Option<String>,
    ) -> Result<ChatSession> {
        let agent_id = resolve_agent_id(&self.core, agent_id).await?;
        let model = model.unwrap_or_else(|| "gpt-5.4".to_string());
        SessionService::from_storage(&self.core.storage)
            .create_workspace_session(agent_id, model, name, skill_id, None)
    }

    async fn delete_session(&self, id: &str) -> Result<bool> {
        SessionService::from_storage(&self.core.storage).delete_session(id)
    }

    // Task operations - require daemon
    async fn list_tasks(&self, _status: Option<String>) -> Result<Vec<Task>> {
        bail!("Task operations require daemon mode. Use 'restflow daemon start' first.")
    }

    async fn get_task(&self, _id: &str) -> Result<Task> {
        bail!("Task operations require daemon mode. Use 'restflow daemon start' first.")
    }

    async fn create_task(&self, _spec: TaskSpec) -> Result<Task> {
        bail!("Task operations require daemon mode. Use 'restflow daemon start' first.")
    }

    async fn convert_session_to_task(
        &self,
        _request: TaskFromSessionRequest,
    ) -> Result<TaskConversionResult> {
        bail!("Task operations require daemon mode. Use 'restflow daemon start' first.")
    }

    async fn update_task(&self, _id: &str, _patch: TaskPatch) -> Result<Task> {
        bail!("Task operations require daemon mode. Use 'restflow daemon start' first.")
    }

    async fn delete_task(
        &self,
        _id: &str,
        _approval_id: Option<&str>,
    ) -> Result<restflow_traits::DeleteWithIdResponse> {
        bail!("Task operations require daemon mode. Use 'restflow daemon start' first.")
    }

    async fn control_task(
        &self,
        _id: &str,
        _action: TaskControlAction,
        _approval_id: Option<&str>,
    ) -> Result<Task> {
        bail!("Task operations require daemon mode. Use 'restflow daemon start' first.")
    }

    async fn get_task_progress(
        &self,
        _id: &str,
        _event_limit: Option<usize>,
    ) -> Result<TaskProgress> {
        bail!("Task operations require daemon mode. Use 'restflow daemon start' first.")
    }

    async fn send_task_message(&self, _id: &str, _message: &str) -> Result<()> {
        bail!("Task operations require daemon mode. Use 'restflow daemon start' first.")
    }

    async fn list_runs(&self, query: RunListQuery) -> Result<Vec<RunSummary>> {
        ExecutionConsoleService::from_storage(&self.core.storage).list_runs(&query)
    }

    async fn get_execution_run_timeline(&self, run_id: &str) -> Result<ExecutionTimeline> {
        restflow_core::telemetry::get_execution_timeline(
            &self.core.storage.execution_traces,
            &ExecutionTraceQuery {
                task_id: None,
                run_id: Some(run_id.to_string()),
                parent_run_id: None,
                session_id: None,
                turn_id: None,
                agent_id: None,
                category: None,
                source: None,
                from_timestamp: None,
                to_timestamp: None,
                limit: Some(200),
                offset: Some(0),
            },
        )
    }
}

async fn resolve_agent_id(core: &Arc<AppCore>, agent_id: Option<String>) -> Result<String> {
    if let Some(agent_id) = agent_id {
        return Ok(agent_id);
    }

    let agents = agent_service::list_agents(core).await?;
    if agents.is_empty() {
        bail!("No agents available");
    }

    Ok(agents[0].id.clone())
}
