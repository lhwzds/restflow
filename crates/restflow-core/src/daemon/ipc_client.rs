use super::ipc_protocol::{IpcRequest, IpcResponse, MAX_MESSAGE_SIZE};
use crate::auth::{AuthProfile, AuthProvider, Credential, CredentialSource, ProfileUpdate};
use crate::memory::ExportResult;
use crate::models::{
    AgentNode, AgentTask, ChatMessage, ChatRole, ChatSession, ChatSessionSummary, ChatSessionUpdate,
    MemoryChunk, MemorySearchResult, MemorySession, MemoryStats, TaskEvent, TerminalSession,
};
use anyhow::{Context, Result, bail};
use serde::de::DeserializeOwned;
use std::path::Path;

#[cfg(unix)]
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(unix)]
use tokio::net::UnixStream;

#[cfg(unix)]
pub struct IpcClient {
    stream: UnixStream,
}

#[cfg(unix)]
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

    pub async fn search_memory_ranked(
        &mut self,
        query: crate::models::memory::MemorySearchQuery,
        min_score: Option<f64>,
        scoring_preset: Option<String>,
    ) -> Result<crate::memory::RankedSearchResult> {
        self.request_typed(IpcRequest::SearchMemoryRanked {
            query,
            min_score,
            scoring_preset,
        })
        .await
    }

    pub async fn get_memory_chunk(&mut self, id: String) -> Result<Option<MemoryChunk>> {
        match self.request(IpcRequest::GetMemoryChunk { id }).await? {
            IpcResponse::Success(value) => Ok(Some(serde_json::from_value(value)?)),
            IpcResponse::Error { code: 404, .. } => Ok(None),
            IpcResponse::Error { code, message } => {
                bail!("IPC error {}: {}", code, message)
            }
            IpcResponse::Pong => bail!("Unexpected Pong response"),
        }
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

    pub async fn create_memory_chunk(&mut self, chunk: MemoryChunk) -> Result<MemoryChunk> {
        self.request_typed(IpcRequest::CreateMemoryChunk { chunk })
            .await
    }

    pub async fn list_memory_by_session(&mut self, session_id: String) -> Result<Vec<MemoryChunk>> {
        self.request_typed(IpcRequest::ListMemoryBySession { session_id })
            .await
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

    pub async fn export_memory_session(&mut self, session_id: String) -> Result<ExportResult> {
        self.request_typed(IpcRequest::ExportMemorySession { session_id })
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn export_memory_advanced(
        &mut self,
        agent_id: String,
        session_id: Option<String>,
        preset: Option<String>,
        include_metadata: Option<bool>,
        include_timestamps: Option<bool>,
        include_source: Option<bool>,
        include_tags: Option<bool>,
    ) -> Result<ExportResult> {
        self.request_typed(IpcRequest::ExportMemoryAdvanced {
            agent_id,
            session_id,
            preset,
            include_metadata,
            include_timestamps,
            include_source,
            include_tags,
        })
        .await
    }

    pub async fn get_memory_session(&mut self, session_id: String) -> Result<Option<MemorySession>> {
        match self.request(IpcRequest::GetMemorySession { session_id }).await? {
            IpcResponse::Success(value) => Ok(Some(serde_json::from_value(value)?)),
            IpcResponse::Error { code: 404, .. } => Ok(None),
            IpcResponse::Error { code, message } => {
                bail!("IPC error {}: {}", code, message)
            }
            IpcResponse::Pong => bail!("Unexpected Pong response"),
        }
    }

    pub async fn list_memory_sessions(&mut self, agent_id: String) -> Result<Vec<MemorySession>> {
        self.request_typed(IpcRequest::ListMemorySessions { agent_id })
            .await
    }

    pub async fn create_memory_session(&mut self, session: MemorySession) -> Result<MemorySession> {
        self.request_typed(IpcRequest::CreateMemorySession { session })
            .await
    }

    pub async fn delete_memory_session(
        &mut self,
        session_id: String,
        delete_chunks: bool,
    ) -> Result<bool> {
        #[derive(serde::Deserialize)]
        struct DeleteResponse {
            deleted: bool,
        }
        let resp: DeleteResponse = self
            .request_typed(IpcRequest::DeleteMemorySession {
                session_id,
                delete_chunks,
            })
            .await?;
        Ok(resp.deleted)
    }

    pub async fn list_sessions(&mut self) -> Result<Vec<ChatSessionSummary>> {
        self.request_typed(IpcRequest::ListSessions).await
    }

    pub async fn list_full_sessions(&mut self) -> Result<Vec<ChatSession>> {
        self.request_typed(IpcRequest::ListFullSessions).await
    }

    pub async fn list_sessions_by_agent(&mut self, agent_id: String) -> Result<Vec<ChatSession>> {
        self.request_typed(IpcRequest::ListSessionsByAgent { agent_id })
            .await
    }

    pub async fn list_sessions_by_skill(&mut self, skill_id: String) -> Result<Vec<ChatSession>> {
        self.request_typed(IpcRequest::ListSessionsBySkill { skill_id })
            .await
    }

    pub async fn count_sessions(&mut self) -> Result<usize> {
        self.request_typed(IpcRequest::CountSessions).await
    }

    pub async fn delete_sessions_older_than(&mut self, older_than_ms: i64) -> Result<usize> {
        self.request_typed(IpcRequest::DeleteSessionsOlderThan { older_than_ms })
            .await
    }

    pub async fn get_session(&mut self, id: String) -> Result<ChatSession> {
        self.request_typed(IpcRequest::GetSession { id }).await
    }

    pub async fn create_session(
        &mut self,
        agent_id: Option<String>,
        model: Option<String>,
        name: Option<String>,
        skill_id: Option<String>,
    ) -> Result<ChatSession> {
        self.request_typed(IpcRequest::CreateSession {
            agent_id,
            model,
            name,
            skill_id,
        })
        .await
    }

    pub async fn update_session(
        &mut self,
        id: String,
        updates: ChatSessionUpdate,
    ) -> Result<ChatSession> {
        self.request_typed(IpcRequest::UpdateSession { id, updates })
            .await
    }

    pub async fn rename_session(&mut self, id: String, name: String) -> Result<ChatSession> {
        self.request_typed(IpcRequest::RenameSession { id, name }).await
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

    pub async fn append_message(
        &mut self,
        session_id: String,
        message: ChatMessage,
    ) -> Result<ChatSession> {
        self.request_typed(IpcRequest::AppendMessage { session_id, message })
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

    pub async fn list_terminal_sessions(&mut self) -> Result<Vec<TerminalSession>> {
        self.request_typed(IpcRequest::ListTerminalSessions).await
    }

    pub async fn get_terminal_session(&mut self, id: String) -> Result<TerminalSession> {
        self.request_typed(IpcRequest::GetTerminalSession { id }).await
    }

    pub async fn create_terminal_session(&mut self) -> Result<TerminalSession> {
        self.request_typed(IpcRequest::CreateTerminalSession).await
    }

    pub async fn rename_terminal_session(
        &mut self,
        id: String,
        name: String,
    ) -> Result<TerminalSession> {
        self.request_typed(IpcRequest::RenameTerminalSession { id, name })
            .await
    }

    pub async fn update_terminal_session(
        &mut self,
        id: String,
        name: Option<String>,
        working_directory: Option<String>,
        startup_command: Option<String>,
    ) -> Result<TerminalSession> {
        self.request_typed(IpcRequest::UpdateTerminalSession {
            id,
            name,
            working_directory,
            startup_command,
        })
        .await
    }

    pub async fn save_terminal_session(
        &mut self,
        session: TerminalSession,
    ) -> Result<TerminalSession> {
        self.request_typed(IpcRequest::SaveTerminalSession { session })
            .await
    }

    pub async fn delete_terminal_session(&mut self, id: String) -> Result<()> {
        let _: serde_json::Value = self
            .request_typed(IpcRequest::DeleteTerminalSession { id })
            .await?;
        Ok(())
    }

    pub async fn mark_all_terminal_sessions_stopped(&mut self) -> Result<usize> {
        self.request_typed(IpcRequest::MarkAllTerminalSessionsStopped)
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

    pub async fn enable_auth_profile(&mut self, id: String) -> Result<()> {
        let _: serde_json::Value = self.request_typed(IpcRequest::EnableAuthProfile { id }).await?;
        Ok(())
    }

    pub async fn disable_auth_profile(&mut self, id: String, reason: String) -> Result<()> {
        let _: serde_json::Value = self
            .request_typed(IpcRequest::DisableAuthProfile { id, reason })
            .await?;
        Ok(())
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

    pub async fn get_api_key_for_profile(&mut self, id: String) -> Result<String> {
        #[derive(serde::Deserialize)]
        struct ApiKeyResponse {
            api_key: String,
        }
        let resp: ApiKeyResponse = self
            .request_typed(IpcRequest::GetApiKeyForProfile { id })
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

    pub async fn mark_auth_success(&mut self, id: String) -> Result<()> {
        let _: serde_json::Value = self.request_typed(IpcRequest::MarkAuthSuccess { id }).await?;
        Ok(())
    }

    pub async fn mark_auth_failure(&mut self, id: String) -> Result<()> {
        let _: serde_json::Value = self.request_typed(IpcRequest::MarkAuthFailure { id }).await?;
        Ok(())
    }

    pub async fn clear_auth_profiles(&mut self) -> Result<()> {
        let _: serde_json::Value = self.request_typed(IpcRequest::ClearAuthProfiles).await?;
        Ok(())
    }

    pub async fn list_tasks(&mut self) -> Result<Vec<AgentTask>> {
        self.request_typed(IpcRequest::ListTasks).await
    }

    pub async fn get_task(&mut self, id: String) -> Result<Option<AgentTask>> {
        match self.request(IpcRequest::GetTask { id }).await? {
            IpcResponse::Success(value) => Ok(Some(serde_json::from_value(value)?)),
            IpcResponse::Error { code: 404, .. } => Ok(None),
            IpcResponse::Error { code, message } => {
                bail!("IPC error {}: {}", code, message)
            }
            IpcResponse::Pong => bail!("Unexpected Pong response"),
        }
    }

    pub async fn create_task(
        &mut self,
        name: String,
        agent_id: String,
        schedule: crate::models::TaskSchedule,
    ) -> Result<AgentTask> {
        self.request_typed(IpcRequest::CreateTask {
            name,
            agent_id,
            schedule,
        })
        .await
    }

    pub async fn update_task(&mut self, task: AgentTask) -> Result<AgentTask> {
        self.request_typed(IpcRequest::UpdateTask { task }).await
    }

    pub async fn delete_task(&mut self, id: String) -> Result<bool> {
        #[derive(serde::Deserialize)]
        struct DeleteResponse {
            deleted: bool,
        }
        let resp: DeleteResponse = self
            .request_typed(IpcRequest::DeleteTask { id })
            .await?;
        Ok(resp.deleted)
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

    pub async fn build_agent_system_prompt(&mut self, agent_node: AgentNode) -> Result<String> {
        #[derive(serde::Deserialize)]
        struct PromptResponse {
            prompt: String,
        }
        let resp: PromptResponse = self
            .request_typed(IpcRequest::BuildAgentSystemPrompt { agent_node })
            .await?;
        Ok(resp.prompt)
    }

    pub async fn init_python(&mut self) -> Result<bool> {
        #[derive(serde::Deserialize)]
        struct InitResponse {
            ready: bool,
        }
        let resp: InitResponse = self.request_typed(IpcRequest::InitPython).await?;
        Ok(resp.ready)
    }
}

#[cfg(not(unix))]
pub struct IpcClient;

#[cfg(not(unix))]
impl IpcClient {
    pub async fn connect(_socket_path: &Path) -> Result<Self> {
        bail!("IPC is not supported on this platform")
    }

    pub async fn request(&mut self, _req: IpcRequest) -> Result<IpcResponse> {
        bail!("IPC is not supported on this platform")
    }

    pub async fn ping(&mut self) -> bool {
        false
    }

    async fn request_typed<T: DeserializeOwned>(&mut self, _req: IpcRequest) -> Result<T> {
        bail!("IPC is not supported on this platform")
    }

    pub async fn search_memory(
        &mut self,
        _query: String,
        _agent_id: Option<String>,
        _limit: Option<u32>,
    ) -> Result<MemorySearchResult> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn search_memory_ranked(
        &mut self,
        _query: crate::models::memory::MemorySearchQuery,
        _min_score: Option<f64>,
        _scoring_preset: Option<String>,
    ) -> Result<crate::memory::RankedSearchResult> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn get_memory_chunk(&mut self, _id: String) -> Result<Option<MemoryChunk>> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn list_memory(
        &mut self,
        _agent_id: Option<String>,
        _tag: Option<String>,
    ) -> Result<Vec<MemoryChunk>> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn add_memory(
        &mut self,
        _content: String,
        _agent_id: Option<String>,
        _tags: Vec<String>,
    ) -> Result<String> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn create_memory_chunk(&mut self, _chunk: MemoryChunk) -> Result<MemoryChunk> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn list_memory_by_session(&mut self, _session_id: String) -> Result<Vec<MemoryChunk>> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn delete_memory(&mut self, _id: String) -> Result<bool> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn clear_memory(&mut self, _agent_id: Option<String>) -> Result<u32> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn get_memory_stats(&mut self, _agent_id: Option<String>) -> Result<MemoryStats> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn export_memory(&mut self, _agent_id: Option<String>) -> Result<ExportResult> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn export_memory_session(&mut self, _session_id: String) -> Result<ExportResult> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn export_memory_advanced(
        &mut self,
        _agent_id: String,
        _session_id: Option<String>,
        _preset: Option<String>,
        _include_metadata: Option<bool>,
        _include_timestamps: Option<bool>,
        _include_source: Option<bool>,
        _include_tags: Option<bool>,
    ) -> Result<ExportResult> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn get_memory_session(&mut self, _session_id: String) -> Result<Option<MemorySession>> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn list_memory_sessions(&mut self, _agent_id: String) -> Result<Vec<MemorySession>> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn create_memory_session(&mut self, _session: MemorySession) -> Result<MemorySession> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn delete_memory_session(
        &mut self,
        _session_id: String,
        _delete_chunks: bool,
    ) -> Result<bool> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn list_sessions(&mut self) -> Result<Vec<ChatSessionSummary>> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn list_full_sessions(&mut self) -> Result<Vec<ChatSession>> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn list_sessions_by_agent(&mut self, _agent_id: String) -> Result<Vec<ChatSession>> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn list_sessions_by_skill(&mut self, _skill_id: String) -> Result<Vec<ChatSession>> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn count_sessions(&mut self) -> Result<usize> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn delete_sessions_older_than(&mut self, _older_than_ms: i64) -> Result<usize> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn get_session(&mut self, _id: String) -> Result<ChatSession> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn create_session(
        &mut self,
        _agent_id: Option<String>,
        _model: Option<String>,
        _name: Option<String>,
        _skill_id: Option<String>,
    ) -> Result<ChatSession> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn update_session(
        &mut self,
        _id: String,
        _updates: ChatSessionUpdate,
    ) -> Result<ChatSession> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn rename_session(&mut self, _id: String, _name: String) -> Result<ChatSession> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn delete_session(&mut self, _id: String) -> Result<bool> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn search_sessions(&mut self, _query: String) -> Result<Vec<ChatSessionSummary>> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn add_message(
        &mut self,
        _session_id: String,
        _role: ChatRole,
        _content: String,
    ) -> Result<ChatSession> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn append_message(
        &mut self,
        _session_id: String,
        _message: ChatMessage,
    ) -> Result<ChatSession> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn get_session_messages(
        &mut self,
        _session_id: String,
        _limit: Option<usize>,
    ) -> Result<Vec<ChatMessage>> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn list_terminal_sessions(&mut self) -> Result<Vec<TerminalSession>> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn get_terminal_session(&mut self, _id: String) -> Result<TerminalSession> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn create_terminal_session(&mut self) -> Result<TerminalSession> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn rename_terminal_session(
        &mut self,
        _id: String,
        _name: String,
    ) -> Result<TerminalSession> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn update_terminal_session(
        &mut self,
        _id: String,
        _name: Option<String>,
        _working_directory: Option<String>,
        _startup_command: Option<String>,
    ) -> Result<TerminalSession> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn save_terminal_session(&mut self, _session: TerminalSession) -> Result<TerminalSession> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn delete_terminal_session(&mut self, _id: String) -> Result<()> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn mark_all_terminal_sessions_stopped(&mut self) -> Result<usize> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn list_auth_profiles(&mut self) -> Result<Vec<AuthProfile>> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn get_auth_profile(&mut self, _id: String) -> Result<AuthProfile> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn add_auth_profile(
        &mut self,
        _name: String,
        _credential: Credential,
        _source: CredentialSource,
        _provider: AuthProvider,
    ) -> Result<AuthProfile> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn remove_auth_profile(&mut self, _id: String) -> Result<AuthProfile> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn update_auth_profile(
        &mut self,
        _id: String,
        _updates: ProfileUpdate,
    ) -> Result<AuthProfile> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn discover_auth(&mut self) -> Result<crate::auth::DiscoverySummary> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn enable_auth_profile(&mut self, _id: String) -> Result<()> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn disable_auth_profile(&mut self, _id: String, _reason: String) -> Result<()> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn get_api_key(&mut self, _provider: AuthProvider) -> Result<String> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn get_api_key_for_profile(&mut self, _id: String) -> Result<String> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn test_auth_profile(&mut self, _id: String) -> Result<bool> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn mark_auth_success(&mut self, _id: String) -> Result<()> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn mark_auth_failure(&mut self, _id: String) -> Result<()> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn clear_auth_profiles(&mut self) -> Result<()> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn list_tasks(&mut self) -> Result<Vec<AgentTask>> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn get_task(&mut self, _id: String) -> Result<Option<AgentTask>> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn create_task(
        &mut self,
        _name: String,
        _agent_id: String,
        _schedule: crate::models::TaskSchedule,
    ) -> Result<AgentTask> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn update_task(&mut self, _task: AgentTask) -> Result<AgentTask> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn delete_task(&mut self, _id: String) -> Result<bool> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn pause_task(&mut self, _id: String) -> Result<AgentTask> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn resume_task(&mut self, _id: String) -> Result<AgentTask> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn list_tasks_by_status(&mut self, _status: String) -> Result<Vec<AgentTask>> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn get_task_history(&mut self, _id: String) -> Result<Vec<TaskEvent>> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn build_agent_system_prompt(&mut self, _agent_node: AgentNode) -> Result<String> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn init_python(&mut self) -> Result<bool> {
        self.request_typed(IpcRequest::Ping).await
    }
}

#[cfg(unix)]
pub async fn is_daemon_available(socket_path: &Path) -> bool {
    if !socket_path.exists() {
        return false;
    }
    match IpcClient::connect(socket_path).await {
        Ok(mut client) => client.ping().await,
        Err(_) => false,
    }
}

#[cfg(not(unix))]
pub async fn is_daemon_available(_socket_path: &Path) -> bool {
    false
}
