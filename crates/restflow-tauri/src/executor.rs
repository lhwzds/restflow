use crate::daemon_manager::DaemonManager;
use anyhow::Result;
use restflow_core::auth::{AuthProfile, AuthProvider, Credential, CredentialSource, ProfileUpdate};
use restflow_core::daemon::{IpcRequest, IpcResponse};
use restflow_core::memory::ExportResult;
use restflow_core::models::{
    AgentExecuteResponse, AgentNode, AgentTask, ChatMessage, ChatRole, ChatSession,
    ChatSessionSummary, MemoryChunk, MemorySearchResult, MemoryStats, Skill, TaskEvent,
    TaskSchedule,
};
use restflow_core::storage::SystemConfig;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct TauriExecutor {
    daemon: Arc<Mutex<DaemonManager>>,
}

impl TauriExecutor {
    pub fn new(daemon: Arc<Mutex<DaemonManager>>) -> Self {
        Self { daemon }
    }

    async fn request<T: DeserializeOwned>(&self, request: IpcRequest) -> Result<T> {
        let mut daemon = self.daemon.lock().await;
        let client = daemon.ensure_connected().await?;
        let response = client.request(request).await?;
        decode_response(response)
    }

    async fn request_optional<T: DeserializeOwned>(
        &self,
        request: IpcRequest,
    ) -> Result<Option<T>> {
        let mut daemon = self.daemon.lock().await;
        let client = daemon.ensure_connected().await?;
        match client.request(request).await? {
            IpcResponse::Success(value) => Ok(Some(serde_json::from_value(value)?)),
            IpcResponse::Error { code: 404, .. } => Ok(None),
            IpcResponse::Error { code, message } => {
                anyhow::bail!("IPC error {}: {}", code, message)
            }
            IpcResponse::Pong => anyhow::bail!("Unexpected Pong response"),
        }
    }

    pub async fn list_agents(&self) -> Result<Vec<restflow_core::storage::agent::StoredAgent>> {
        self.request(IpcRequest::ListAgents).await
    }

    pub async fn get_agent(
        &self,
        id: String,
    ) -> Result<restflow_core::storage::agent::StoredAgent> {
        self.request(IpcRequest::GetAgent { id }).await
    }

    pub async fn create_agent(
        &self,
        name: String,
        agent: AgentNode,
    ) -> Result<restflow_core::storage::agent::StoredAgent> {
        self.request(IpcRequest::CreateAgent { name, agent })
            .await
    }

    pub async fn update_agent(
        &self,
        id: String,
        name: Option<String>,
        agent: Option<AgentNode>,
    ) -> Result<restflow_core::storage::agent::StoredAgent> {
        self.request(IpcRequest::UpdateAgent { id, name, agent })
            .await
    }

    pub async fn delete_agent(&self, id: String) -> Result<()> {
        let _: Value = self.request(IpcRequest::DeleteAgent { id }).await?;
        Ok(())
    }

    pub async fn list_skills(&self) -> Result<Vec<Skill>> {
        self.request(IpcRequest::ListSkills).await
    }

    pub async fn get_skill(&self, id: String) -> Result<Option<Skill>> {
        self.request_optional(IpcRequest::GetSkill { id }).await
    }

    pub async fn create_skill(&self, skill: Skill) -> Result<()> {
        let _: Value = self.request(IpcRequest::CreateSkill { skill }).await?;
        Ok(())
    }

    pub async fn update_skill(&self, id: String, skill: Skill) -> Result<()> {
        let _: Value = self.request(IpcRequest::UpdateSkill { id, skill }).await?;
        Ok(())
    }

    pub async fn delete_skill(&self, id: String) -> Result<()> {
        let _: Value = self.request(IpcRequest::DeleteSkill { id }).await?;
        Ok(())
    }

    pub async fn list_tasks(&self) -> Result<Vec<AgentTask>> {
        self.request(IpcRequest::ListTasks).await
    }

    pub async fn list_tasks_by_status(&self, status: String) -> Result<Vec<AgentTask>> {
        self.request(IpcRequest::ListTasksByStatus { status }).await
    }

    pub async fn get_task(&self, id: String) -> Result<Option<AgentTask>> {
        self.request_optional(IpcRequest::GetTask { id }).await
    }

    pub async fn create_task(
        &self,
        name: String,
        agent_id: String,
        schedule: TaskSchedule,
    ) -> Result<AgentTask> {
        self.request(IpcRequest::CreateTask {
            name,
            agent_id,
            schedule,
        })
        .await
    }

    pub async fn update_task(&self, task: AgentTask) -> Result<AgentTask> {
        self.request(IpcRequest::UpdateTask { task }).await
    }

    pub async fn delete_task(&self, id: String) -> Result<bool> {
        #[derive(serde::Deserialize)]
        struct DeleteResponse {
            deleted: bool,
        }
        let response: DeleteResponse = self.request(IpcRequest::DeleteTask { id }).await?;
        Ok(response.deleted)
    }

    pub async fn pause_task(&self, id: String) -> Result<AgentTask> {
        self.request(IpcRequest::PauseTask { id }).await
    }

    pub async fn resume_task(&self, id: String) -> Result<AgentTask> {
        self.request(IpcRequest::ResumeTask { id }).await
    }

    pub async fn get_task_history(&self, id: String) -> Result<Vec<TaskEvent>> {
        self.request(IpcRequest::GetTaskHistory { id }).await
    }

    pub async fn search_memory(
        &self,
        query: String,
        agent_id: Option<String>,
        limit: Option<u32>,
    ) -> Result<MemorySearchResult> {
        self.request(IpcRequest::SearchMemory {
            query,
            agent_id,
            limit,
        })
        .await
    }

    pub async fn list_memory(
        &self,
        agent_id: Option<String>,
        tag: Option<String>,
    ) -> Result<Vec<MemoryChunk>> {
        self.request(IpcRequest::ListMemory { agent_id, tag })
            .await
    }

    pub async fn add_memory(
        &self,
        content: String,
        agent_id: Option<String>,
        tags: Vec<String>,
    ) -> Result<String> {
        #[derive(serde::Deserialize)]
        struct AddResponse {
            id: String,
        }
        let response: AddResponse = self
            .request(IpcRequest::AddMemory {
                content,
                agent_id,
                tags,
            })
            .await?;
        Ok(response.id)
    }

    pub async fn delete_memory(&self, id: String) -> Result<bool> {
        #[derive(serde::Deserialize)]
        struct DeleteResponse {
            deleted: bool,
        }
        let response: DeleteResponse = self.request(IpcRequest::DeleteMemory { id }).await?;
        Ok(response.deleted)
    }

    pub async fn clear_memory(&self, agent_id: Option<String>) -> Result<u32> {
        #[derive(serde::Deserialize)]
        struct ClearResponse {
            deleted: u32,
        }
        let response: ClearResponse = self.request(IpcRequest::ClearMemory { agent_id }).await?;
        Ok(response.deleted)
    }

    pub async fn get_memory_stats(&self, agent_id: Option<String>) -> Result<MemoryStats> {
        self.request(IpcRequest::GetMemoryStats { agent_id }).await
    }

    pub async fn export_memory(&self, agent_id: Option<String>) -> Result<ExportResult> {
        self.request(IpcRequest::ExportMemory { agent_id }).await
    }

    pub async fn list_sessions(&self) -> Result<Vec<ChatSessionSummary>> {
        self.request(IpcRequest::ListSessions).await
    }

    pub async fn get_session(&self, id: String) -> Result<ChatSession> {
        self.request(IpcRequest::GetSession { id }).await
    }

    pub async fn create_session(
        &self,
        agent_id: Option<String>,
        model: Option<String>,
    ) -> Result<ChatSession> {
        self.request(IpcRequest::CreateSession { agent_id, model })
            .await
    }

    pub async fn delete_session(&self, id: String) -> Result<bool> {
        #[derive(serde::Deserialize)]
        struct DeleteResponse {
            deleted: bool,
        }
        let response: DeleteResponse = self.request(IpcRequest::DeleteSession { id }).await?;
        Ok(response.deleted)
    }

    pub async fn search_sessions(&self, query: String) -> Result<Vec<ChatSessionSummary>> {
        self.request(IpcRequest::SearchSessions { query }).await
    }

    pub async fn add_message(
        &self,
        session_id: String,
        role: ChatRole,
        content: String,
    ) -> Result<ChatSession> {
        self.request(IpcRequest::AddMessage {
            session_id,
            role,
            content,
        })
        .await
    }

    pub async fn get_session_messages(
        &self,
        session_id: String,
        limit: Option<usize>,
    ) -> Result<Vec<ChatMessage>> {
        self.request(IpcRequest::GetSessionMessages { session_id, limit })
            .await
    }

    pub async fn list_auth_profiles(&self) -> Result<Vec<AuthProfile>> {
        self.request(IpcRequest::ListAuthProfiles).await
    }

    pub async fn get_auth_profile(&self, id: String) -> Result<AuthProfile> {
        self.request(IpcRequest::GetAuthProfile { id }).await
    }

    pub async fn add_auth_profile(
        &self,
        name: String,
        credential: Credential,
        source: CredentialSource,
        provider: AuthProvider,
    ) -> Result<AuthProfile> {
        self.request(IpcRequest::AddAuthProfile {
            name,
            credential,
            source,
            provider,
        })
        .await
    }

    pub async fn remove_auth_profile(&self, id: String) -> Result<AuthProfile> {
        self.request(IpcRequest::RemoveAuthProfile { id }).await
    }

    pub async fn update_auth_profile(
        &self,
        id: String,
        updates: ProfileUpdate,
    ) -> Result<AuthProfile> {
        self.request(IpcRequest::UpdateAuthProfile { id, updates })
            .await
    }

    pub async fn discover_auth(&self) -> Result<restflow_core::auth::DiscoverySummary> {
        self.request(IpcRequest::DiscoverAuth).await
    }

    pub async fn get_api_key(&self, provider: AuthProvider) -> Result<String> {
        #[derive(serde::Deserialize)]
        struct ApiKeyResponse {
            api_key: String,
        }
        let response: ApiKeyResponse = self.request(IpcRequest::GetApiKey { provider }).await?;
        Ok(response.api_key)
    }

    pub async fn test_auth_profile(&self, id: String) -> Result<bool> {
        #[derive(serde::Deserialize)]
        struct TestResponse {
            ok: bool,
        }
        let response: TestResponse = self.request(IpcRequest::TestAuthProfile { id }).await?;
        Ok(response.ok)
    }

    pub async fn list_secrets(&self) -> Result<Vec<restflow_core::models::Secret>> {
        self.request(IpcRequest::ListSecrets).await
    }

    pub async fn get_secret(&self, key: String) -> Result<Option<String>> {
        #[derive(serde::Deserialize)]
        struct SecretResponse {
            value: Option<String>,
        }
        let response: SecretResponse = self.request(IpcRequest::GetSecret { key }).await?;
        Ok(response.value)
    }

    pub async fn set_secret(
        &self,
        key: String,
        value: String,
        description: Option<String>,
    ) -> Result<()> {
        let _: Value = self
            .request(IpcRequest::SetSecret {
                key,
                value,
                description,
            })
            .await?;
        Ok(())
    }

    pub async fn delete_secret(&self, key: String) -> Result<()> {
        let _: Value = self.request(IpcRequest::DeleteSecret { key }).await?;
        Ok(())
    }

    pub async fn get_config(&self) -> Result<SystemConfig> {
        self.request(IpcRequest::GetConfig).await
    }

    pub async fn set_config(&self, config: SystemConfig) -> Result<()> {
        let _: Value = self.request(IpcRequest::SetConfig { config }).await?;
        Ok(())
    }

    pub async fn get_system_info(&self) -> Result<Value> {
        self.request(IpcRequest::GetSystemInfo).await
    }

    pub async fn get_available_models(&self) -> Result<Vec<String>> {
        self.request(IpcRequest::GetAvailableModels).await
    }

    pub async fn get_available_tools(&self) -> Result<Vec<String>> {
        self.request(IpcRequest::GetAvailableTools).await
    }

    pub async fn execute_agent(
        &self,
        id: String,
        input: String,
        session_id: Option<String>,
    ) -> Result<AgentExecuteResponse> {
        self.request(IpcRequest::ExecuteAgent {
            id,
            input,
            session_id,
        })
        .await
    }
}

fn decode_response<T: DeserializeOwned>(response: IpcResponse) -> Result<T> {
    match response {
        IpcResponse::Success(value) => Ok(serde_json::from_value(value)?),
        IpcResponse::Error { code, message } => {
            anyhow::bail!("IPC error {}: {}", code, message)
        }
        IpcResponse::Pong => anyhow::bail!("Unexpected Pong response"),
    }
}
