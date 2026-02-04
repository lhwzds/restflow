use super::ipc_protocol::{IpcRequest, IpcResponse, MAX_MESSAGE_SIZE};
use crate::auth::{AuthProfile, AuthProvider, Credential, CredentialSource, ProfileUpdate};
use crate::memory::ExportResult;
use crate::models::{
    AgentTask, ChatMessage, ChatRole, ChatSession, ChatSessionSummary, MemoryChunk,
    MemorySearchResult, MemoryStats, TaskEvent,
};
use anyhow::{Context, Result, bail};
use serde::de::DeserializeOwned;
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

pub struct IpcClient {
    stream: UnixStream,
}

impl IpcClient {
    pub async fn connect(socket_path: &Path) -> Result<Self> {
        let stream = UnixStream::connect(socket_path)
            .await
            .context("Failed to connect to daemon. Is it running?")?;
        Ok(Self { stream })
    }

    pub async fn request(&mut self, req: IpcRequest) -> Result<IpcResponse> {
        let json = serde_json::to_vec(&req)?;
        self.stream
            .write_all(&(json.len() as u32).to_le_bytes())
            .await?;
        self.stream.write_all(&json).await?;

        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await?;
        let len = u32::from_le_bytes(len_buf) as usize;
        if len > MAX_MESSAGE_SIZE {
            anyhow::bail!("Response too large");
        }

        let mut buf = vec![0u8; len];
        self.stream.read_exact(&mut buf).await?;
        Ok(serde_json::from_slice(&buf)?)
    }

    pub async fn ping(&mut self) -> bool {
        matches!(self.request(IpcRequest::Ping).await, Ok(IpcResponse::Pong))
    }

    async fn request_typed<T: DeserializeOwned>(&mut self, req: IpcRequest) -> Result<T> {
        match self.request(req).await? {
            IpcResponse::Success(value) => {
                serde_json::from_value(value).context("Failed to deserialize response")
            }
            IpcResponse::Pong => bail!("Unexpected Pong response"),
            IpcResponse::Error { code, message } => bail!("IPC error {}: {}", code, message),
        }
    }

    pub async fn search_memory(
        &mut self,
        query: String,
        agent_id: Option<String>,
        limit: Option<u32>,
    ) -> Result<MemorySearchResult> {
        self.request_typed(IpcRequest::SearchMemory {
            query,
            agent_id,
            limit,
        })
        .await
    }

    pub async fn list_memory(
        &mut self,
        agent_id: Option<String>,
        tag: Option<String>,
    ) -> Result<Vec<MemoryChunk>> {
        self.request_typed(IpcRequest::ListMemory { agent_id, tag })
            .await
    }

    pub async fn add_memory(
        &mut self,
        content: String,
        agent_id: Option<String>,
        tags: Vec<String>,
    ) -> Result<String> {
        #[derive(serde::Deserialize)]
        struct AddMemoryResponse {
            id: String,
        }
        let resp: AddMemoryResponse = self
            .request_typed(IpcRequest::AddMemory {
                content,
                agent_id,
                tags,
            })
            .await?;
        Ok(resp.id)
    }

    pub async fn delete_memory(&mut self, id: String) -> Result<bool> {
        #[derive(serde::Deserialize)]
        struct DeleteResponse {
            deleted: bool,
        }
        let resp: DeleteResponse = self
            .request_typed(IpcRequest::DeleteMemory { id })
            .await?;
        Ok(resp.deleted)
    }

    pub async fn clear_memory(&mut self, agent_id: Option<String>) -> Result<u32> {
        #[derive(serde::Deserialize)]
        struct ClearResponse {
            deleted: u32,
        }
        let resp: ClearResponse = self
            .request_typed(IpcRequest::ClearMemory { agent_id })
            .await?;
        Ok(resp.deleted)
    }

    pub async fn get_memory_stats(&mut self, agent_id: Option<String>) -> Result<MemoryStats> {
        self.request_typed(IpcRequest::GetMemoryStats { agent_id })
            .await
    }

    pub async fn export_memory(&mut self, agent_id: Option<String>) -> Result<ExportResult> {
        self.request_typed(IpcRequest::ExportMemory { agent_id })
            .await
    }

    pub async fn list_sessions(&mut self) -> Result<Vec<ChatSessionSummary>> {
        self.request_typed(IpcRequest::ListSessions).await
    }

    pub async fn get_session(&mut self, id: String) -> Result<ChatSession> {
        self.request_typed(IpcRequest::GetSession { id }).await
    }

    pub async fn create_session(
        &mut self,
        agent_id: Option<String>,
        model: Option<String>,
    ) -> Result<ChatSession> {
        self.request_typed(IpcRequest::CreateSession { agent_id, model })
            .await
    }

    pub async fn delete_session(&mut self, id: String) -> Result<bool> {
        #[derive(serde::Deserialize)]
        struct DeleteResponse {
            deleted: bool,
        }
        let resp: DeleteResponse = self
            .request_typed(IpcRequest::DeleteSession { id })
            .await?;
        Ok(resp.deleted)
    }

    pub async fn search_sessions(&mut self, query: String) -> Result<Vec<ChatSessionSummary>> {
        self.request_typed(IpcRequest::SearchSessions { query })
            .await
    }

    pub async fn add_message(
        &mut self,
        session_id: String,
        role: ChatRole,
        content: String,
    ) -> Result<ChatSession> {
        self.request_typed(IpcRequest::AddMessage {
            session_id,
            role,
            content,
        })
        .await
    }

    pub async fn get_session_messages(
        &mut self,
        session_id: String,
        limit: Option<usize>,
    ) -> Result<Vec<ChatMessage>> {
        self.request_typed(IpcRequest::GetSessionMessages { session_id, limit })
            .await
    }

    pub async fn list_auth_profiles(&mut self) -> Result<Vec<AuthProfile>> {
        self.request_typed(IpcRequest::ListAuthProfiles).await
    }

    pub async fn get_auth_profile(&mut self, id: String) -> Result<AuthProfile> {
        self.request_typed(IpcRequest::GetAuthProfile { id }).await
    }

    pub async fn add_auth_profile(
        &mut self,
        name: String,
        credential: Credential,
        source: CredentialSource,
        provider: AuthProvider,
    ) -> Result<AuthProfile> {
        self.request_typed(IpcRequest::AddAuthProfile {
            name,
            credential,
            source,
            provider,
        })
        .await
    }

    pub async fn remove_auth_profile(&mut self, id: String) -> Result<AuthProfile> {
        self.request_typed(IpcRequest::RemoveAuthProfile { id })
            .await
    }

    pub async fn update_auth_profile(
        &mut self,
        id: String,
        updates: ProfileUpdate,
    ) -> Result<AuthProfile> {
        self.request_typed(IpcRequest::UpdateAuthProfile { id, updates })
            .await
    }

    pub async fn discover_auth(&mut self) -> Result<crate::auth::DiscoverySummary> {
        self.request_typed(IpcRequest::DiscoverAuth).await
    }

    pub async fn get_api_key(&mut self, provider: AuthProvider) -> Result<String> {
        #[derive(serde::Deserialize)]
        struct ApiKeyResponse {
            api_key: String,
        }
        let resp: ApiKeyResponse = self
            .request_typed(IpcRequest::GetApiKey { provider })
            .await?;
        Ok(resp.api_key)
    }

    pub async fn test_auth_profile(&mut self, id: String) -> Result<bool> {
        #[derive(serde::Deserialize)]
        struct TestResponse {
            ok: bool,
        }
        let resp: TestResponse = self
            .request_typed(IpcRequest::TestAuthProfile { id })
            .await?;
        Ok(resp.ok)
    }

    pub async fn pause_task(&mut self, id: String) -> Result<AgentTask> {
        self.request_typed(IpcRequest::PauseTask { id }).await
    }

    pub async fn resume_task(&mut self, id: String) -> Result<AgentTask> {
        self.request_typed(IpcRequest::ResumeTask { id }).await
    }

    pub async fn list_tasks_by_status(&mut self, status: String) -> Result<Vec<AgentTask>> {
        self.request_typed(IpcRequest::ListTasksByStatus { status })
            .await
    }

    pub async fn get_task_history(&mut self, id: String) -> Result<Vec<TaskEvent>> {
        self.request_typed(IpcRequest::GetTaskHistory { id }).await
    }
}

pub async fn is_daemon_available(socket_path: &Path) -> bool {
    if !socket_path.exists() {
        return false;
    }
    match IpcClient::connect(socket_path).await {
        Ok(mut client) => client.ping().await,
        Err(_) => false,
    }
}
