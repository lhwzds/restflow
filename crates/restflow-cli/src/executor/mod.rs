use anyhow::Result;
use async_trait::async_trait;
use restflow_core::daemon::is_daemon_available;
use restflow_core::memory::ExportResult;
use restflow_core::models::{
    AgentExecuteResponse, AgentNode, AgentTask, AgentTaskStatus, BackgroundAgentControlAction,
    BackgroundAgentPatch, BackgroundAgentSpec, BackgroundMessage, BackgroundMessageSource,
    BackgroundProgress, ChatSession, ChatSessionSummary, MemoryChunk, MemorySearchResult,
    MemoryStats, Secret, Skill, TaskEvent,
};
use restflow_core::paths;
use restflow_core::storage::SystemConfig;
use restflow_core::storage::agent::StoredAgent;
use std::sync::Arc;

pub mod direct;
pub mod ipc;

#[derive(Debug, Clone)]
pub struct CreateBackgroundAgentInput {
    pub spec: BackgroundAgentSpec,
}

#[derive(Debug, Clone)]
pub struct UpdateBackgroundAgentInput {
    pub id: String,
    pub patch: BackgroundAgentPatch,
}

#[derive(Debug, Clone)]
pub struct ControlBackgroundAgentInput {
    pub id: String,
    pub action: BackgroundAgentControlAction,
}

#[derive(Debug, Clone)]
pub struct BackgroundProgressInput {
    pub id: String,
    pub event_limit: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct SendBackgroundMessageInput {
    pub id: String,
    pub message: String,
    pub source: Option<BackgroundMessageSource>,
}

#[derive(Debug, Clone)]
pub struct ListBackgroundMessageInput {
    pub id: String,
    pub limit: Option<usize>,
}

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
    async fn execute_agent(
        &self,
        id: &str,
        input: String,
        session_id: Option<String>,
    ) -> Result<AgentExecuteResponse>;

    async fn list_skills(&self) -> Result<Vec<Skill>>;
    async fn get_skill(&self, id: &str) -> Result<Option<Skill>>;
    async fn create_skill(&self, skill: Skill) -> Result<()>;
    async fn update_skill(&self, id: &str, skill: Skill) -> Result<()>;
    async fn delete_skill(&self, id: &str) -> Result<()>;

    async fn list_tasks(&self) -> Result<Vec<AgentTask>>;
    async fn list_tasks_by_status(&self, status: AgentTaskStatus) -> Result<Vec<AgentTask>>;
    async fn get_task(&self, id: &str) -> Result<Option<AgentTask>>;
    async fn get_task_history(&self, id: &str) -> Result<Vec<TaskEvent>>;
    async fn create_background_agent(&self, input: CreateBackgroundAgentInput)
    -> Result<AgentTask>;
    async fn update_background_agent(&self, input: UpdateBackgroundAgentInput)
    -> Result<AgentTask>;
    async fn delete_background_agent(&self, id: &str) -> Result<bool>;
    async fn control_background_agent(
        &self,
        input: ControlBackgroundAgentInput,
    ) -> Result<AgentTask>;
    async fn get_background_progress(
        &self,
        input: BackgroundProgressInput,
    ) -> Result<BackgroundProgress>;
    async fn send_background_message(
        &self,
        input: SendBackgroundMessageInput,
    ) -> Result<BackgroundMessage>;
    async fn list_background_messages(
        &self,
        input: ListBackgroundMessageInput,
    ) -> Result<Vec<BackgroundMessage>>;

    async fn search_memory(
        &self,
        query: String,
        agent_id: Option<String>,
        limit: Option<u32>,
    ) -> Result<MemorySearchResult>;
    async fn list_memory(
        &self,
        agent_id: Option<String>,
        tag: Option<String>,
    ) -> Result<Vec<MemoryChunk>>;
    async fn clear_memory(&self, agent_id: Option<String>) -> Result<u32>;
    async fn get_memory_stats(&self, agent_id: Option<String>) -> Result<MemoryStats>;
    async fn export_memory(&self, agent_id: Option<String>) -> Result<ExportResult>;

    async fn list_sessions(&self) -> Result<Vec<ChatSessionSummary>>;
    async fn get_session(&self, id: &str) -> Result<ChatSession>;
    async fn create_session(&self, agent_id: String, model: String) -> Result<ChatSession>;
    async fn delete_session(&self, id: &str) -> Result<bool>;
    async fn search_sessions(&self, query: String) -> Result<Vec<ChatSessionSummary>>;

    async fn list_secrets(&self) -> Result<Vec<Secret>>;
    async fn set_secret(&self, key: &str, value: &str, description: Option<String>) -> Result<()>;
    async fn delete_secret(&self, key: &str) -> Result<()>;
    async fn has_secret(&self, key: &str) -> Result<bool>;

    async fn get_config(&self) -> Result<SystemConfig>;
    async fn set_config(&self, config: SystemConfig) -> Result<()>;
}

pub async fn create(db_path: Option<String>) -> Result<Arc<dyn CommandExecutor>> {
    let socket_path = paths::socket_path()?;
    if is_daemon_available(&socket_path).await {
        let executor = ipc::IpcExecutor::connect(&socket_path).await?;
        Ok(Arc::new(executor))
    } else {
        let executor = direct::DirectExecutor::connect(db_path).await?;
        Ok(Arc::new(executor))
    }
}
