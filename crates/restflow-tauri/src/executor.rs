use crate::daemon_manager::DaemonManager;
use anyhow::Result;
use restflow_core::auth::{AuthProfile, AuthProvider, Credential, CredentialSource, ProfileUpdate};
use restflow_core::daemon::{IpcRequest, IpcResponse};
use restflow_core::memory::{ExportResult, RankedSearchResult};
use restflow_core::models::{
    AgentNode, BackgroundAgent, BackgroundAgentControlAction, BackgroundAgentEvent,
    BackgroundAgentPatch, BackgroundAgentSpec, BackgroundMessageSource, ChatMessage, ChatRole,
    ChatSession, ChatSessionSummary, ChatSessionUpdate, Hook, MemoryChunk, MemorySearchResult,
    MemorySession, MemoryStats, Skill, TerminalSession,
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
        self.request(IpcRequest::CreateAgent { name, agent }).await
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

    pub async fn list_background_agents(
        &self,
        status: Option<String>,
    ) -> Result<Vec<BackgroundAgent>> {
        self.request(IpcRequest::ListBackgroundAgents { status })
            .await
    }

    pub async fn list_runnable_background_agents(
        &self,
        current_time: Option<i64>,
    ) -> Result<Vec<BackgroundAgent>> {
        self.request(IpcRequest::ListRunnableBackgroundAgents { current_time })
            .await
    }

    pub async fn get_background_agent(&self, id: String) -> Result<Option<BackgroundAgent>> {
        self.request_optional(IpcRequest::GetBackgroundAgent { id })
            .await
    }

    pub async fn list_hooks(&self) -> Result<Vec<Hook>> {
        self.request(IpcRequest::ListHooks).await
    }

    pub async fn create_hook(&self, hook: Hook) -> Result<Hook> {
        self.request(IpcRequest::CreateHook { hook }).await
    }

    pub async fn update_hook(&self, id: String, hook: Hook) -> Result<Hook> {
        self.request(IpcRequest::UpdateHook { id, hook }).await
    }

    pub async fn delete_hook(&self, id: String) -> Result<bool> {
        #[derive(serde::Deserialize)]
        struct DeleteResponse {
            deleted: bool,
        }
        let response: DeleteResponse = self.request(IpcRequest::DeleteHook { id }).await?;
        Ok(response.deleted)
    }

    pub async fn test_hook(&self, id: String) -> Result<()> {
        let _: Value = self.request(IpcRequest::TestHook { id }).await?;
        Ok(())
    }

    pub async fn create_background_agent(
        &self,
        spec: BackgroundAgentSpec,
    ) -> Result<BackgroundAgent> {
        self.request(IpcRequest::CreateBackgroundAgent { spec })
            .await
    }

    pub async fn update_background_agent(
        &self,
        id: String,
        patch: BackgroundAgentPatch,
    ) -> Result<BackgroundAgent> {
        self.request(IpcRequest::UpdateBackgroundAgent { id, patch })
            .await
    }

    pub async fn delete_background_agent(&self, id: String) -> Result<bool> {
        #[derive(serde::Deserialize)]
        struct DeleteResponse {
            deleted: bool,
        }
        let response: DeleteResponse = self
            .request(IpcRequest::DeleteBackgroundAgent { id })
            .await?;
        Ok(response.deleted)
    }

    pub async fn control_background_agent(
        &self,
        id: String,
        action: BackgroundAgentControlAction,
    ) -> Result<BackgroundAgent> {
        self.request(IpcRequest::ControlBackgroundAgent { id, action })
            .await
    }

    pub async fn get_background_agent_history(
        &self,
        id: String,
    ) -> Result<Vec<BackgroundAgentEvent>> {
        self.request(IpcRequest::GetBackgroundAgentHistory { id })
            .await
    }

    pub async fn send_background_agent_message(
        &self,
        id: String,
        message: String,
        source: Option<BackgroundMessageSource>,
    ) -> Result<()> {
        let _: Value = self
            .request(IpcRequest::SendBackgroundAgentMessage {
                id,
                message,
                source,
            })
            .await?;
        Ok(())
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

    pub async fn search_memory_ranked(
        &self,
        query: restflow_core::models::memory::MemorySearchQuery,
        min_score: Option<f64>,
        scoring_preset: Option<String>,
    ) -> Result<RankedSearchResult> {
        self.request(IpcRequest::SearchMemoryRanked {
            query,
            min_score,
            scoring_preset,
        })
        .await
    }

    pub async fn get_memory_chunk(&self, id: String) -> Result<Option<MemoryChunk>> {
        self.request_optional(IpcRequest::GetMemoryChunk { id })
            .await
    }

    pub async fn list_memory(
        &self,
        agent_id: Option<String>,
        tag: Option<String>,
    ) -> Result<Vec<MemoryChunk>> {
        self.request(IpcRequest::ListMemory { agent_id, tag }).await
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

    pub async fn create_memory_chunk(&self, chunk: MemoryChunk) -> Result<MemoryChunk> {
        self.request(IpcRequest::CreateMemoryChunk { chunk }).await
    }

    pub async fn list_memory_by_session(&self, session_id: String) -> Result<Vec<MemoryChunk>> {
        self.request(IpcRequest::ListMemoryBySession { session_id })
            .await
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

    pub async fn export_memory_session(&self, session_id: String) -> Result<ExportResult> {
        self.request(IpcRequest::ExportMemorySession { session_id })
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn export_memory_advanced(
        &self,
        agent_id: String,
        session_id: Option<String>,
        preset: Option<String>,
        include_metadata: Option<bool>,
        include_timestamps: Option<bool>,
        include_source: Option<bool>,
        include_tags: Option<bool>,
    ) -> Result<ExportResult> {
        self.request(IpcRequest::ExportMemoryAdvanced {
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

    pub async fn get_memory_session(&self, session_id: String) -> Result<Option<MemorySession>> {
        self.request_optional(IpcRequest::GetMemorySession { session_id })
            .await
    }

    pub async fn list_memory_sessions(&self, agent_id: String) -> Result<Vec<MemorySession>> {
        self.request(IpcRequest::ListMemorySessions { agent_id })
            .await
    }

    pub async fn create_memory_session(&self, session: MemorySession) -> Result<MemorySession> {
        self.request(IpcRequest::CreateMemorySession { session })
            .await
    }

    pub async fn delete_memory_session(
        &self,
        session_id: String,
        delete_chunks: bool,
    ) -> Result<bool> {
        #[derive(serde::Deserialize)]
        struct DeleteResponse {
            deleted: bool,
        }
        let response: DeleteResponse = self
            .request(IpcRequest::DeleteMemorySession {
                session_id,
                delete_chunks,
            })
            .await?;
        Ok(response.deleted)
    }

    pub async fn list_sessions(&self) -> Result<Vec<ChatSessionSummary>> {
        self.request(IpcRequest::ListSessions).await
    }

    pub async fn list_full_sessions(&self) -> Result<Vec<ChatSession>> {
        self.request(IpcRequest::ListFullSessions).await
    }

    pub async fn list_sessions_by_agent(&self, agent_id: String) -> Result<Vec<ChatSession>> {
        self.request(IpcRequest::ListSessionsByAgent { agent_id })
            .await
    }

    pub async fn list_sessions_by_skill(&self, skill_id: String) -> Result<Vec<ChatSession>> {
        self.request(IpcRequest::ListSessionsBySkill { skill_id })
            .await
    }

    pub async fn count_sessions(&self) -> Result<usize> {
        self.request(IpcRequest::CountSessions).await
    }

    pub async fn delete_sessions_older_than(&self, older_than_ms: i64) -> Result<usize> {
        self.request(IpcRequest::DeleteSessionsOlderThan { older_than_ms })
            .await
    }

    pub async fn get_session(&self, id: String) -> Result<ChatSession> {
        self.request(IpcRequest::GetSession { id }).await
    }

    pub async fn create_session(
        &self,
        agent_id: Option<String>,
        model: Option<String>,
        name: Option<String>,
        skill_id: Option<String>,
    ) -> Result<ChatSession> {
        self.request(IpcRequest::CreateSession {
            agent_id,
            model,
            name,
            skill_id,
        })
        .await
    }

    pub async fn update_session(
        &self,
        id: String,
        updates: ChatSessionUpdate,
    ) -> Result<ChatSession> {
        self.request(IpcRequest::UpdateSession { id, updates })
            .await
    }

    pub async fn rename_session(&self, id: String, name: String) -> Result<ChatSession> {
        self.request(IpcRequest::RenameSession { id, name }).await
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

    pub async fn append_message(
        &self,
        session_id: String,
        message: ChatMessage,
    ) -> Result<ChatSession> {
        self.request(IpcRequest::AppendMessage {
            session_id,
            message,
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

    pub async fn list_terminal_sessions(&self) -> Result<Vec<TerminalSession>> {
        self.request(IpcRequest::ListTerminalSessions).await
    }

    pub async fn get_terminal_session(&self, id: String) -> Result<TerminalSession> {
        self.request(IpcRequest::GetTerminalSession { id }).await
    }

    pub async fn create_terminal_session(&self) -> Result<TerminalSession> {
        self.request(IpcRequest::CreateTerminalSession).await
    }

    pub async fn rename_terminal_session(
        &self,
        id: String,
        name: String,
    ) -> Result<TerminalSession> {
        self.request(IpcRequest::RenameTerminalSession { id, name })
            .await
    }

    pub async fn update_terminal_session(
        &self,
        id: String,
        name: Option<String>,
        working_directory: Option<String>,
        startup_command: Option<String>,
    ) -> Result<TerminalSession> {
        self.request(IpcRequest::UpdateTerminalSession {
            id,
            name,
            working_directory,
            startup_command,
        })
        .await
    }

    pub async fn save_terminal_session(&self, session: TerminalSession) -> Result<TerminalSession> {
        self.request(IpcRequest::SaveTerminalSession { session })
            .await
    }

    pub async fn delete_terminal_session(&self, id: String) -> Result<()> {
        let _: Value = self
            .request(IpcRequest::DeleteTerminalSession { id })
            .await?;
        Ok(())
    }

    pub async fn mark_all_terminal_sessions_stopped(&self) -> Result<usize> {
        self.request(IpcRequest::MarkAllTerminalSessionsStopped)
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

    pub async fn enable_auth_profile(&self, id: String) -> Result<()> {
        let _: Value = self.request(IpcRequest::EnableAuthProfile { id }).await?;
        Ok(())
    }

    pub async fn disable_auth_profile(&self, id: String, reason: String) -> Result<()> {
        let _: Value = self
            .request(IpcRequest::DisableAuthProfile { id, reason })
            .await?;
        Ok(())
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

    pub async fn mark_auth_success(&self, id: String) -> Result<()> {
        let _: Value = self.request(IpcRequest::MarkAuthSuccess { id }).await?;
        Ok(())
    }

    pub async fn mark_auth_failure(&self, id: String) -> Result<()> {
        let _: Value = self.request(IpcRequest::MarkAuthFailure { id }).await?;
        Ok(())
    }

    pub async fn clear_auth_profiles(&self) -> Result<()> {
        let _: Value = self.request(IpcRequest::ClearAuthProfiles).await?;
        Ok(())
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

    pub async fn init_python(&self) -> Result<bool> {
        #[derive(serde::Deserialize)]
        struct InitResponse {
            ready: bool,
        }
        let response: InitResponse = self.request(IpcRequest::InitPython).await?;
        Ok(response.ready)
    }

    pub async fn build_agent_system_prompt(&self, agent_node: AgentNode) -> Result<String> {
        #[derive(serde::Deserialize)]
        struct PromptResponse {
            prompt: String,
        }
        let response: PromptResponse = self
            .request(IpcRequest::BuildAgentSystemPrompt { agent_node })
            .await?;
        Ok(response.prompt)
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
