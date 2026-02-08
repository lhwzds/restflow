use super::ipc_protocol::{
    IpcRequest, IpcResponse, StreamFrame, ToolDefinition, ToolExecutionResult, MAX_MESSAGE_SIZE,
};
use crate::auth::{AuthProfile, AuthProvider, Credential, CredentialSource, ProfileUpdate};
use crate::memory::ExportResult;
use crate::models::{
    AgentNode, BackgroundAgent, BackgroundAgentControlAction, BackgroundAgentEvent,
    BackgroundAgentPatch, BackgroundAgentSpec, ChatMessage, ChatRole, ChatSession,
    ChatSessionSummary, ChatSessionUpdate, MemoryChunk, MemorySearchResult, MemorySession,
    MemoryStats, Skill, TerminalSession,
};
use crate::storage::agent::StoredAgent;
use anyhow::{bail, Context, Result};
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

    async fn send_request_frame(&mut self, req: &IpcRequest) -> Result<()> {
        let json = serde_json::to_vec(&req)?;
        self.stream
            .write_all(&(json.len() as u32).to_le_bytes())
            .await?;
        self.stream.write_all(&json).await?;
        Ok(())
    }

    async fn read_raw_frame(&mut self) -> Result<Vec<u8>> {
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await?;
        let len = u32::from_le_bytes(len_buf) as usize;
        if len > MAX_MESSAGE_SIZE {
            anyhow::bail!("Response too large");
        }

        let mut buf = vec![0u8; len];
        self.stream.read_exact(&mut buf).await?;
        Ok(buf)
    }

    pub async fn request(&mut self, req: IpcRequest) -> Result<IpcResponse> {
        self.send_request_frame(&req).await?;
        let buf = self.read_raw_frame().await?;
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

    async fn request_optional<T: DeserializeOwned>(
        &mut self,
        req: IpcRequest,
    ) -> Result<Option<T>> {
        match self.request(req).await? {
            IpcResponse::Success(value) => Ok(Some(serde_json::from_value(value)?)),
            IpcResponse::Error { code: 404, .. } => Ok(None),
            IpcResponse::Error { code, message } => bail!("IPC error {}: {}", code, message),
            IpcResponse::Pong => bail!("Unexpected Pong response"),
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

    pub async fn list_skills(&mut self) -> Result<Vec<Skill>> {
        self.request_typed(IpcRequest::ListSkills).await
    }

    pub async fn get_skill(&mut self, id: String) -> Result<Option<Skill>> {
        self.request_optional(IpcRequest::GetSkill { id }).await
    }

    pub async fn create_skill(&mut self, skill: Skill) -> Result<()> {
        let _: serde_json::Value = self
            .request_typed(IpcRequest::CreateSkill { skill })
            .await?;
        Ok(())
    }

    pub async fn update_skill(&mut self, id: String, skill: Skill) -> Result<()> {
        let _: serde_json::Value = self
            .request_typed(IpcRequest::UpdateSkill { id, skill })
            .await?;
        Ok(())
    }

    pub async fn delete_skill(&mut self, id: String) -> Result<()> {
        let _: serde_json::Value = self.request_typed(IpcRequest::DeleteSkill { id }).await?;
        Ok(())
    }

    pub async fn list_agents(&mut self) -> Result<Vec<StoredAgent>> {
        self.request_typed(IpcRequest::ListAgents).await
    }

    pub async fn get_agent(&mut self, id: String) -> Result<StoredAgent> {
        self.request_typed(IpcRequest::GetAgent { id }).await
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
        let resp: DeleteResponse = self.request_typed(IpcRequest::DeleteMemory { id }).await?;
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

    pub async fn get_memory_session(
        &mut self,
        session_id: String,
    ) -> Result<Option<MemorySession>> {
        match self
            .request(IpcRequest::GetMemorySession { session_id })
            .await?
        {
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
        self.request_typed(IpcRequest::RenameSession { id, name })
            .await
    }

    pub async fn delete_session(&mut self, id: String) -> Result<bool> {
        #[derive(serde::Deserialize)]
        struct DeleteResponse {
            deleted: bool,
        }
        let resp: DeleteResponse = self.request_typed(IpcRequest::DeleteSession { id }).await?;
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
        self.request_typed(IpcRequest::AppendMessage {
            session_id,
            message,
        })
        .await
    }

    pub async fn execute_chat_session(
        &mut self,
        session_id: String,
        user_input: Option<String>,
    ) -> Result<ChatSession> {
        self.request_typed(IpcRequest::ExecuteChatSession {
            session_id,
            user_input,
        })
        .await
    }

    pub async fn execute_chat_session_stream<F>(
        &mut self,
        session_id: String,
        user_input: Option<String>,
        stream_id: String,
        mut on_frame: F,
    ) -> Result<()>
    where
        F: FnMut(StreamFrame) -> Result<()>,
    {
        self.send_request_frame(&IpcRequest::ExecuteChatSessionStream {
            session_id,
            user_input,
            stream_id,
        })
        .await?;

        loop {
            let buf = self.read_raw_frame().await?;

            if let Ok(frame) = serde_json::from_slice::<StreamFrame>(&buf) {
                let terminal =
                    matches!(frame, StreamFrame::Done { .. } | StreamFrame::Error { .. });
                on_frame(frame)?;
                if terminal {
                    break;
                }
                continue;
            }

            let response: IpcResponse = serde_json::from_slice(&buf)
                .context("Failed to deserialize streaming IPC frame")?;
            match response {
                IpcResponse::Error { code, message } => {
                    bail!("IPC error {}: {}", code, message);
                }
                IpcResponse::Success(_) => {
                    bail!("Unexpected success response while reading stream")
                }
                IpcResponse::Pong => {
                    bail!("Unexpected Pong response while reading stream")
                }
            }
        }

        Ok(())
    }

    pub async fn cancel_chat_session_stream(&mut self, stream_id: String) -> Result<bool> {
        #[derive(serde::Deserialize)]
        struct CancelResponse {
            canceled: bool,
        }
        let resp: CancelResponse = self
            .request_typed(IpcRequest::CancelChatSessionStream { stream_id })
            .await?;
        Ok(resp.canceled)
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
        self.request_typed(IpcRequest::GetTerminalSession { id })
            .await
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
        let _: serde_json::Value = self
            .request_typed(IpcRequest::EnableAuthProfile { id })
            .await?;
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
        let _: serde_json::Value = self
            .request_typed(IpcRequest::MarkAuthSuccess { id })
            .await?;
        Ok(())
    }

    pub async fn mark_auth_failure(&mut self, id: String) -> Result<()> {
        let _: serde_json::Value = self
            .request_typed(IpcRequest::MarkAuthFailure { id })
            .await?;
        Ok(())
    }

    pub async fn clear_auth_profiles(&mut self) -> Result<()> {
        let _: serde_json::Value = self.request_typed(IpcRequest::ClearAuthProfiles).await?;
        Ok(())
    }

    pub async fn list_background_agents(
        &mut self,
        status: Option<String>,
    ) -> Result<Vec<BackgroundAgent>> {
        self.request_typed(IpcRequest::ListBackgroundAgents { status })
            .await
    }

    pub async fn get_background_agent(&mut self, id: String) -> Result<Option<BackgroundAgent>> {
        match self.request(IpcRequest::GetBackgroundAgent { id }).await? {
            IpcResponse::Success(value) => Ok(Some(serde_json::from_value(value)?)),
            IpcResponse::Error { code: 404, .. } => Ok(None),
            IpcResponse::Error { code, message } => {
                bail!("IPC error {}: {}", code, message)
            }
            IpcResponse::Pong => bail!("Unexpected Pong response"),
        }
    }

    pub async fn create_background_agent(
        &mut self,
        spec: BackgroundAgentSpec,
    ) -> Result<BackgroundAgent> {
        self.request_typed(IpcRequest::CreateBackgroundAgent { spec })
            .await
    }

    pub async fn update_background_agent(
        &mut self,
        id: String,
        patch: BackgroundAgentPatch,
    ) -> Result<BackgroundAgent> {
        self.request_typed(IpcRequest::UpdateBackgroundAgent { id, patch })
            .await
    }

    pub async fn delete_background_agent(&mut self, id: String) -> Result<bool> {
        #[derive(serde::Deserialize)]
        struct DeleteResponse {
            deleted: bool,
        }
        let resp: DeleteResponse = self
            .request_typed(IpcRequest::DeleteBackgroundAgent { id })
            .await?;
        Ok(resp.deleted)
    }

    pub async fn control_background_agent(
        &mut self,
        id: String,
        action: BackgroundAgentControlAction,
    ) -> Result<BackgroundAgent> {
        self.request_typed(IpcRequest::ControlBackgroundAgent { id, action })
            .await
    }

    pub async fn get_background_agent_history(
        &mut self,
        id: String,
    ) -> Result<Vec<BackgroundAgentEvent>> {
        self.request_typed(IpcRequest::GetBackgroundAgentHistory { id })
            .await
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

    pub async fn get_available_tool_definitions(&mut self) -> Result<Vec<ToolDefinition>> {
        self.request_typed(IpcRequest::GetAvailableToolDefinitions)
            .await
    }

    pub async fn execute_tool(
        &mut self,
        name: String,
        input: serde_json::Value,
    ) -> Result<ToolExecutionResult> {
        self.request_typed(IpcRequest::ExecuteTool { name, input })
            .await
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

    pub async fn list_skills(&mut self) -> Result<Vec<Skill>> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn get_skill(&mut self, _id: String) -> Result<Option<Skill>> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn create_skill(&mut self, _skill: Skill) -> Result<()> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn update_skill(&mut self, _id: String, _skill: Skill) -> Result<()> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn delete_skill(&mut self, _id: String) -> Result<()> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn list_agents(&mut self) -> Result<Vec<StoredAgent>> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn get_agent(&mut self, _id: String) -> Result<StoredAgent> {
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

    pub async fn list_memory_by_session(
        &mut self,
        _session_id: String,
    ) -> Result<Vec<MemoryChunk>> {
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

    pub async fn get_memory_session(
        &mut self,
        _session_id: String,
    ) -> Result<Option<MemorySession>> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn list_memory_sessions(&mut self, _agent_id: String) -> Result<Vec<MemorySession>> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn create_memory_session(
        &mut self,
        _session: MemorySession,
    ) -> Result<MemorySession> {
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

    pub async fn execute_chat_session(
        &mut self,
        _session_id: String,
        _user_input: Option<String>,
    ) -> Result<ChatSession> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn execute_chat_session_stream<F>(
        &mut self,
        _session_id: String,
        _user_input: Option<String>,
        _stream_id: String,
        _on_frame: F,
    ) -> Result<()>
    where
        F: FnMut(StreamFrame) -> Result<()>,
    {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn cancel_chat_session_stream(&mut self, _stream_id: String) -> Result<bool> {
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

    pub async fn save_terminal_session(
        &mut self,
        _session: TerminalSession,
    ) -> Result<TerminalSession> {
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

    pub async fn list_background_agents(
        &mut self,
        _status: Option<String>,
    ) -> Result<Vec<BackgroundAgent>> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn get_background_agent(&mut self, _id: String) -> Result<Option<BackgroundAgent>> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn create_background_agent(
        &mut self,
        _spec: BackgroundAgentSpec,
    ) -> Result<BackgroundAgent> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn update_background_agent(
        &mut self,
        _id: String,
        _patch: BackgroundAgentPatch,
    ) -> Result<BackgroundAgent> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn delete_background_agent(&mut self, _id: String) -> Result<bool> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn control_background_agent(
        &mut self,
        _id: String,
        _action: BackgroundAgentControlAction,
    ) -> Result<BackgroundAgent> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn get_background_agent_history(
        &mut self,
        _id: String,
    ) -> Result<Vec<BackgroundAgentEvent>> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn build_agent_system_prompt(&mut self, _agent_node: AgentNode) -> Result<String> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn init_python(&mut self) -> Result<bool> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn get_available_tool_definitions(&mut self) -> Result<Vec<ToolDefinition>> {
        self.request_typed(IpcRequest::Ping).await
    }

    pub async fn execute_tool(
        &mut self,
        _name: String,
        _input: serde_json::Value,
    ) -> Result<ToolExecutionResult> {
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
