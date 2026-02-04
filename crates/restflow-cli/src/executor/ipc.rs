use anyhow::{Result, bail};
use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::executor::{CommandExecutor, CreateTaskInput};
use restflow_core::daemon::{IpcClient, IpcRequest, IpcResponse};
use restflow_core::memory::ExportResult;
use restflow_core::models::{
    AgentExecuteResponse, AgentNode, AgentTask, AgentTaskStatus, ChatSession,
    ChatSessionSummary, MemoryChunk, MemorySearchResult, MemoryStats, Secret, Skill,
    TaskEvent,
};
use restflow_core::storage::SystemConfig;
use restflow_core::storage::agent::StoredAgent;
use restflow_core::AppCore;

pub struct IpcExecutor {
    client: Mutex<IpcClient>,
}

impl IpcExecutor {
    pub async fn connect(socket_path: &Path) -> Result<Self> {
        let client = IpcClient::connect(socket_path).await?;
        Ok(Self {
            client: Mutex::new(client),
        })
    }

    async fn request(&self, req: IpcRequest) -> Result<IpcResponse> {
        let mut client = self.client.lock().await;
        client.request(req).await
    }

    fn decode_response<T: serde::de::DeserializeOwned>(&self, response: IpcResponse) -> Result<T> {
        match response {
            IpcResponse::Success(value) => Ok(serde_json::from_value(value)?),
            IpcResponse::Error { message, .. } => bail!(message),
            IpcResponse::Pong => bail!("Unexpected Pong response"),
        }
    }

    fn decode_response_optional<T: serde::de::DeserializeOwned>(
        &self,
        response: IpcResponse,
    ) -> Result<Option<T>> {
        match response {
            IpcResponse::Success(value) => Ok(Some(serde_json::from_value(value)?)),
            IpcResponse::Error { code: 404, .. } => Ok(None),
            IpcResponse::Error { message, .. } => bail!(message),
            IpcResponse::Pong => bail!("Unexpected Pong response"),
        }
    }
}

#[async_trait]
impl CommandExecutor for IpcExecutor {
    fn core(&self) -> Option<Arc<AppCore>> {
        None
    }

    async fn list_agents(&self) -> Result<Vec<StoredAgent>> {
        let response = self.request(IpcRequest::ListAgents).await?;
        self.decode_response(response)
    }

    async fn get_agent(&self, id: &str) -> Result<StoredAgent> {
        let response = self
            .request(IpcRequest::GetAgent { id: id.to_string() })
            .await?;
        self.decode_response(response)
    }

    async fn create_agent(&self, name: String, agent: AgentNode) -> Result<StoredAgent> {
        let response = self
            .request(IpcRequest::CreateAgent { name, agent })
            .await?;
        self.decode_response(response)
    }

    async fn update_agent(
        &self,
        id: &str,
        name: Option<String>,
        agent: Option<AgentNode>,
    ) -> Result<StoredAgent> {
        let response = self
            .request(IpcRequest::UpdateAgent {
                id: id.to_string(),
                name,
                agent,
            })
            .await?;
        self.decode_response(response)
    }

    async fn delete_agent(&self, id: &str) -> Result<()> {
        let response = self
            .request(IpcRequest::DeleteAgent { id: id.to_string() })
            .await?;
        self.decode_response::<serde_json::Value>(response).map(|_| ())
    }

    async fn execute_agent(
        &self,
        id: &str,
        input: String,
        session_id: Option<String>,
    ) -> Result<AgentExecuteResponse> {
        let response = self
            .request(IpcRequest::ExecuteAgent {
                id: id.to_string(),
                input,
                session_id,
            })
            .await?;
        self.decode_response(response)
    }

    async fn list_skills(&self) -> Result<Vec<Skill>> {
        let response = self.request(IpcRequest::ListSkills).await?;
        self.decode_response(response)
    }

    async fn get_skill(&self, id: &str) -> Result<Option<Skill>> {
        let response = self
            .request(IpcRequest::GetSkill { id: id.to_string() })
            .await?;
        self.decode_response_optional(response)
    }

    async fn create_skill(&self, skill: Skill) -> Result<()> {
        let response = self.request(IpcRequest::CreateSkill { skill }).await?;
        self.decode_response::<serde_json::Value>(response).map(|_| ())
    }

    async fn update_skill(&self, id: &str, skill: Skill) -> Result<()> {
        let response = self
            .request(IpcRequest::UpdateSkill {
                id: id.to_string(),
                skill,
            })
            .await?;
        self.decode_response::<serde_json::Value>(response).map(|_| ())
    }

    async fn delete_skill(&self, id: &str) -> Result<()> {
        let response = self
            .request(IpcRequest::DeleteSkill { id: id.to_string() })
            .await?;
        self.decode_response::<serde_json::Value>(response).map(|_| ())
    }

    async fn list_tasks(&self) -> Result<Vec<AgentTask>> {
        let response = self.request(IpcRequest::ListTasks).await?;
        self.decode_response(response)
    }

    async fn list_tasks_by_status(&self, status: AgentTaskStatus) -> Result<Vec<AgentTask>> {
        let status_str = match status {
            AgentTaskStatus::Active => "active",
            AgentTaskStatus::Paused => "paused",
            AgentTaskStatus::Running => "running",
            AgentTaskStatus::Completed => "completed",
            AgentTaskStatus::Failed => "failed",
        };
        let response = self
            .request(IpcRequest::ListTasksByStatus {
                status: status_str.to_string(),
            })
            .await?;
        self.decode_response(response)
    }

    async fn get_task(&self, id: &str) -> Result<Option<AgentTask>> {
        let response = self
            .request(IpcRequest::GetTask { id: id.to_string() })
            .await?;
        self.decode_response_optional(response)
    }

    async fn create_task(&self, input: CreateTaskInput) -> Result<AgentTask> {
        let response = self
            .request(IpcRequest::CreateTask {
                name: input.name,
                agent_id: input.agent_id,
                schedule: input.schedule,
            })
            .await?;
        let mut task: AgentTask = self.decode_response(response)?;

        // Note: IPC protocol doesn't support input yet, so we can't set it remotely
        // This is a known limitation that may require protocol extension
        if input.input.is_some() {
            tracing::warn!("Task input cannot be set via IPC; task created without input");
        }
        task.input = input.input;

        Ok(task)
    }

    async fn pause_task(&self, id: &str) -> Result<AgentTask> {
        let response = self
            .request(IpcRequest::PauseTask { id: id.to_string() })
            .await?;
        self.decode_response(response)
    }

    async fn resume_task(&self, id: &str) -> Result<AgentTask> {
        let response = self
            .request(IpcRequest::ResumeTask { id: id.to_string() })
            .await?;
        self.decode_response(response)
    }

    async fn delete_task(&self, id: &str) -> Result<bool> {
        let response = self
            .request(IpcRequest::StopTask { id: id.to_string() })
            .await?;
        // StopTask returns an error for IPC, so treat any success as deleted
        match response {
            IpcResponse::Success(_) => Ok(true),
            IpcResponse::Error { code: 404, .. } => Ok(false),
            IpcResponse::Error { message, .. } => bail!(message),
            IpcResponse::Pong => bail!("Unexpected Pong response"),
        }
    }

    async fn get_task_history(&self, id: &str) -> Result<Vec<TaskEvent>> {
        let response = self
            .request(IpcRequest::GetTaskHistory { id: id.to_string() })
            .await?;
        self.decode_response(response)
    }

    async fn search_memory(
        &self,
        query: String,
        agent_id: Option<String>,
        limit: Option<u32>,
    ) -> Result<MemorySearchResult> {
        let response = self
            .request(IpcRequest::SearchMemory {
                query,
                agent_id,
                limit,
            })
            .await?;
        self.decode_response(response)
    }

    async fn list_memory(
        &self,
        agent_id: Option<String>,
        tag: Option<String>,
    ) -> Result<Vec<MemoryChunk>> {
        let response = self
            .request(IpcRequest::ListMemory { agent_id, tag })
            .await?;
        self.decode_response(response)
    }

    async fn clear_memory(&self, agent_id: Option<String>) -> Result<u32> {
        let response = self
            .request(IpcRequest::ClearMemory { agent_id })
            .await?;
        #[derive(serde::Deserialize)]
        struct ClearResponse {
            deleted: u32,
        }
        let resp: ClearResponse = self.decode_response(response)?;
        Ok(resp.deleted)
    }

    async fn get_memory_stats(&self, agent_id: Option<String>) -> Result<MemoryStats> {
        let response = self
            .request(IpcRequest::GetMemoryStats { agent_id })
            .await?;
        self.decode_response(response)
    }

    async fn export_memory(&self, agent_id: Option<String>) -> Result<ExportResult> {
        let response = self
            .request(IpcRequest::ExportMemory { agent_id })
            .await?;
        self.decode_response(response)
    }

    async fn list_sessions(&self) -> Result<Vec<ChatSessionSummary>> {
        let mut client = self.client.lock().await;
        client.list_sessions().await
    }

    async fn get_session(&self, id: &str) -> Result<ChatSession> {
        let mut client = self.client.lock().await;
        client.get_session(id.to_string()).await
    }

    async fn create_session(&self, agent_id: String, model: String) -> Result<ChatSession> {
        let mut client = self.client.lock().await;
        client.create_session(Some(agent_id), Some(model)).await
    }

    async fn delete_session(&self, id: &str) -> Result<bool> {
        let mut client = self.client.lock().await;
        client.delete_session(id.to_string()).await
    }

    async fn search_sessions(&self, query: String) -> Result<Vec<ChatSessionSummary>> {
        let mut client = self.client.lock().await;
        client.search_sessions(query).await
    }

    async fn list_secrets(&self) -> Result<Vec<Secret>> {
        let response = self.request(IpcRequest::ListSecrets).await?;
        self.decode_response(response)
    }

    async fn set_secret(
        &self,
        key: &str,
        value: &str,
        description: Option<String>,
    ) -> Result<()> {
        let response = self
            .request(IpcRequest::SetSecret {
                key: key.to_string(),
                value: value.to_string(),
                description,
            })
            .await?;
        self.decode_response::<serde_json::Value>(response).map(|_| ())
    }

    async fn delete_secret(&self, key: &str) -> Result<()> {
        let response = self
            .request(IpcRequest::DeleteSecret {
                key: key.to_string(),
            })
            .await?;
        self.decode_response::<serde_json::Value>(response).map(|_| ())
    }

    async fn has_secret(&self, key: &str) -> Result<bool> {
        let response = self
            .request(IpcRequest::GetSecret {
                key: key.to_string(),
            })
            .await?;
        match response {
            IpcResponse::Success(_) => Ok(true),
            IpcResponse::Error { code: 404, .. } => Ok(false),
            IpcResponse::Error { message, .. } => bail!(message),
            IpcResponse::Pong => bail!("Unexpected Pong response"),
        }
    }

    async fn get_config(&self) -> Result<SystemConfig> {
        let response = self.request(IpcRequest::GetConfig).await?;
        self.decode_response(response)
    }

    async fn set_config(&self, config: SystemConfig) -> Result<()> {
        let response = self.request(IpcRequest::SetConfig { config }).await?;
        self.decode_response::<serde_json::Value>(response).map(|_| ())
    }
}
