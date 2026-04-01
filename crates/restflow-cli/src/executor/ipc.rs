use anyhow::{Result, bail};
use async_trait::async_trait;
use restflow_contracts::{
    CleanupReportResponse, ClearResponse, IdResponse, OkResponse, PairingApprovalResponse,
    PairingOwnerResponse, PairingStateResponse, RouteBindingResponse,
    SessionSourceMigrationResponse,
};
use std::path::Path;
use tokio::sync::Mutex;

use crate::executor::CommandExecutor;
use restflow_core::daemon::request_mapper::to_contract;
use restflow_core::daemon::{IpcClient, IpcRequest};
use restflow_core::memory::ExportResult;
use restflow_core::models::{
    AgentNode, BackgroundAgent, BackgroundAgentControlAction, BackgroundAgentConversionResult,
    BackgroundAgentPatch, BackgroundAgentSpec, BackgroundMessage, BackgroundProgress, ChatSession,
    ChatSessionSummary, Deliverable, ExecutionSessionListQuery, ExecutionSessionSummary,
    ExecutionTimeline, ItemQuery, MemoryChunk, MemorySearchResult, MemoryStats, Secret,
    SharedEntry, Skill, WorkItem, WorkItemPatch, WorkItemSpec,
};
use restflow_core::storage::SystemConfig;
use restflow_core::storage::agent::StoredAgent;
use restflow_traits::BackgroundAgentCommandOutcome;
use restflow_traits::store::BackgroundAgentConvertSessionRequest;

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

    async fn request_typed<T: serde::de::DeserializeOwned>(&self, req: IpcRequest) -> Result<T> {
        let mut client = self.client.lock().await;
        client.request_typed(req).await
    }

    async fn request_optional<T: serde::de::DeserializeOwned>(
        &self,
        req: IpcRequest,
    ) -> Result<Option<T>> {
        let mut client = self.client.lock().await;
        client.request_optional(req).await
    }
}

#[async_trait]
impl CommandExecutor for IpcExecutor {
    async fn list_agents(&self) -> Result<Vec<StoredAgent>> {
        self.request_typed(IpcRequest::ListAgents).await
    }

    async fn get_agent(&self, id: &str) -> Result<StoredAgent> {
        self.request_typed(IpcRequest::GetAgent { id: id.to_string() })
            .await
    }

    async fn create_agent(&self, name: String, agent: AgentNode) -> Result<StoredAgent> {
        let agent = to_contract(agent)?;
        self.request_typed(IpcRequest::CreateAgent {
            name,
            agent,
            preview: false,
            confirmation_token: None,
        })
        .await
    }

    async fn update_agent(
        &self,
        id: &str,
        name: Option<String>,
        agent: Option<AgentNode>,
    ) -> Result<StoredAgent> {
        let agent = agent.map(to_contract).transpose()?;
        self.request_typed(IpcRequest::UpdateAgent {
            id: id.to_string(),
            name,
            agent,
            preview: false,
            confirmation_token: None,
        })
        .await
    }

    async fn delete_agent(&self, id: &str) -> Result<()> {
        let _: OkResponse = self
            .request_typed(IpcRequest::DeleteAgent { id: id.to_string() })
            .await?;
        Ok(())
    }

    async fn list_skills(&self) -> Result<Vec<Skill>> {
        self.request_typed(IpcRequest::ListSkills).await
    }

    async fn get_skill(&self, id: &str) -> Result<Option<Skill>> {
        self.request_optional(IpcRequest::GetSkill { id: id.to_string() })
            .await
    }

    async fn create_skill(&self, skill: Skill) -> Result<()> {
        let skill = to_contract(skill)?;
        let _: OkResponse = self
            .request_typed(IpcRequest::CreateSkill { skill })
            .await?;
        Ok(())
    }

    async fn update_skill(&self, id: &str, skill: Skill) -> Result<()> {
        let skill = to_contract(skill)?;
        let _: OkResponse = self
            .request_typed(IpcRequest::UpdateSkill {
                id: id.to_string(),
                skill,
            })
            .await?;
        Ok(())
    }

    async fn delete_skill(&self, id: &str) -> Result<()> {
        let _: OkResponse = self
            .request_typed(IpcRequest::DeleteSkill { id: id.to_string() })
            .await?;
        Ok(())
    }

    async fn search_memory(
        &self,
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

    async fn list_memory(
        &self,
        agent_id: Option<String>,
        tag: Option<String>,
    ) -> Result<Vec<MemoryChunk>> {
        self.request_typed(IpcRequest::ListMemory { agent_id, tag })
            .await
    }

    async fn clear_memory(&self, agent_id: Option<String>) -> Result<u32> {
        let resp: ClearResponse = self
            .request_typed(IpcRequest::ClearMemory { agent_id })
            .await?;
        Ok(resp.deleted)
    }

    async fn get_memory_stats(&self, agent_id: Option<String>) -> Result<MemoryStats> {
        self.request_typed(IpcRequest::GetMemoryStats { agent_id })
            .await
    }

    async fn export_memory(&self, agent_id: Option<String>) -> Result<ExportResult> {
        self.request_typed(IpcRequest::ExportMemory { agent_id })
            .await
    }

    async fn store_memory(
        &self,
        agent_id: &str,
        content: &str,
        tags: Vec<String>,
    ) -> Result<String> {
        let resp: IdResponse = self
            .request_typed(IpcRequest::AddMemory {
                content: content.to_string(),
                agent_id: Some(agent_id.to_string()),
                tags,
            })
            .await?;
        Ok(resp.id)
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
        client
            .create_session(Some(agent_id), Some(model), None, None)
            .await
    }

    async fn delete_session(&self, id: &str) -> Result<bool> {
        let mut client = self.client.lock().await;
        client.delete_session(id.to_string()).await
    }

    async fn search_sessions(&self, query: String) -> Result<Vec<ChatSessionSummary>> {
        let mut client = self.client.lock().await;
        client.search_sessions(query).await
    }

    async fn list_notes(&self, query: ItemQuery) -> Result<Vec<WorkItem>> {
        let query = to_contract(query)?;
        self.request_typed(IpcRequest::ListWorkItems { query })
            .await
    }

    async fn get_note(&self, id: &str) -> Result<Option<WorkItem>> {
        self.request_optional(IpcRequest::GetWorkItem { id: id.to_string() })
            .await
    }

    async fn create_note(&self, spec: WorkItemSpec) -> Result<WorkItem> {
        let spec = to_contract(spec)?;
        self.request_typed(IpcRequest::CreateWorkItem { spec })
            .await
    }

    async fn update_note(&self, id: &str, patch: WorkItemPatch) -> Result<WorkItem> {
        let patch = to_contract(patch)?;
        self.request_typed(IpcRequest::UpdateWorkItem {
            id: id.to_string(),
            patch,
        })
        .await
    }

    async fn delete_note(&self, id: &str) -> Result<()> {
        let _: OkResponse = self
            .request_typed(IpcRequest::DeleteWorkItem { id: id.to_string() })
            .await?;
        Ok(())
    }

    async fn list_note_folders(&self) -> Result<Vec<String>> {
        self.request_typed(IpcRequest::ListWorkItemFolders).await
    }

    async fn list_secrets(&self) -> Result<Vec<Secret>> {
        self.request_typed(IpcRequest::ListSecrets).await
    }

    async fn set_secret(&self, key: &str, value: &str, description: Option<String>) -> Result<()> {
        let _: OkResponse = self
            .request_typed(IpcRequest::SetSecret {
                key: key.to_string(),
                value: value.to_string(),
                description,
            })
            .await?;
        Ok(())
    }

    async fn create_secret(
        &self,
        key: &str,
        value: &str,
        description: Option<String>,
    ) -> Result<()> {
        let _: OkResponse = self
            .request_typed(IpcRequest::CreateSecret {
                key: key.to_string(),
                value: value.to_string(),
                description,
            })
            .await?;
        Ok(())
    }

    async fn update_secret(
        &self,
        key: &str,
        value: &str,
        description: Option<String>,
    ) -> Result<()> {
        let _: OkResponse = self
            .request_typed(IpcRequest::UpdateSecret {
                key: key.to_string(),
                value: value.to_string(),
                description,
            })
            .await?;
        Ok(())
    }

    async fn delete_secret(&self, key: &str) -> Result<()> {
        let _: OkResponse = self
            .request_typed(IpcRequest::DeleteSecret {
                key: key.to_string(),
            })
            .await?;
        Ok(())
    }

    async fn has_secret(&self, key: &str) -> Result<bool> {
        let response = self
            .request_optional::<restflow_contracts::SecretResponse>(IpcRequest::GetSecret {
                key: key.to_string(),
            })
            .await?;
        Ok(response.is_some())
    }

    async fn get_config(&self) -> Result<SystemConfig> {
        self.request_typed(IpcRequest::GetConfig).await
    }

    async fn get_global_config(&self) -> Result<SystemConfig> {
        self.request_typed(IpcRequest::GetGlobalConfig).await
    }

    async fn set_config(&self, config: SystemConfig) -> Result<()> {
        let config = to_contract(config)?;
        let _: OkResponse = self.request_typed(IpcRequest::SetConfig { config }).await?;
        Ok(())
    }

    async fn list_hooks(&self) -> Result<Vec<restflow_core::models::Hook>> {
        self.request_typed(IpcRequest::ListHooks).await
    }

    async fn create_hook(
        &self,
        hook: restflow_core::models::Hook,
    ) -> Result<restflow_core::models::Hook> {
        let hook = to_contract(hook)?;
        self.request_typed(IpcRequest::CreateHook { hook }).await
    }

    async fn update_hook(
        &self,
        id: &str,
        hook: restflow_core::models::Hook,
    ) -> Result<restflow_core::models::Hook> {
        let hook = to_contract(hook)?;
        self.request_typed(IpcRequest::UpdateHook {
            id: id.to_string(),
            hook,
        })
        .await
    }

    async fn delete_hook(&self, id: &str) -> Result<bool> {
        let resp: restflow_contracts::DeleteResponse = self
            .request_typed(IpcRequest::DeleteHook { id: id.to_string() })
            .await?;
        Ok(resp.deleted)
    }

    async fn test_hook(&self, id: &str) -> Result<()> {
        let _: OkResponse = self
            .request_typed(IpcRequest::TestHook { id: id.to_string() })
            .await?;
        Ok(())
    }

    async fn list_pairing_state(&self) -> Result<PairingStateResponse> {
        self.request_typed(IpcRequest::ListPairingState).await
    }

    async fn approve_pairing(&self, code: &str) -> Result<PairingApprovalResponse> {
        self.request_typed(IpcRequest::ApprovePairing {
            code: code.to_string(),
        })
        .await
    }

    async fn deny_pairing(&self, code: &str) -> Result<()> {
        let _: OkResponse = self
            .request_typed(IpcRequest::DenyPairing {
                code: code.to_string(),
            })
            .await?;
        Ok(())
    }

    async fn revoke_paired_peer(&self, peer_id: &str) -> Result<bool> {
        let resp: restflow_contracts::DeleteResponse = self
            .request_typed(IpcRequest::RevokePairedPeer {
                peer_id: peer_id.to_string(),
            })
            .await?;
        Ok(resp.deleted)
    }

    async fn get_pairing_owner(&self) -> Result<PairingOwnerResponse> {
        self.request_typed(IpcRequest::GetPairingOwner).await
    }

    async fn set_pairing_owner(&self, chat_id: &str) -> Result<PairingOwnerResponse> {
        self.request_typed(IpcRequest::SetPairingOwner {
            chat_id: chat_id.to_string(),
        })
        .await
    }

    async fn list_route_bindings(&self) -> Result<Vec<RouteBindingResponse>> {
        self.request_typed(IpcRequest::ListRouteBindings).await
    }

    async fn bind_route(
        &self,
        binding_type: &str,
        target_id: &str,
        agent_id: &str,
    ) -> Result<RouteBindingResponse> {
        self.request_typed(IpcRequest::BindRoute {
            binding_type: binding_type.to_string(),
            target_id: target_id.to_string(),
            agent_id: agent_id.to_string(),
        })
        .await
    }

    async fn unbind_route(&self, id: &str) -> Result<bool> {
        let resp: restflow_contracts::DeleteResponse = self
            .request_typed(IpcRequest::UnbindRoute { id: id.to_string() })
            .await?;
        Ok(resp.deleted)
    }

    async fn run_cleanup(&self) -> Result<CleanupReportResponse> {
        self.request_typed(IpcRequest::RunCleanup).await
    }

    async fn migrate_session_sources(
        &self,
        dry_run: bool,
    ) -> Result<SessionSourceMigrationResponse> {
        self.request_typed(IpcRequest::MigrateSessionSources { dry_run })
            .await
    }

    // Background Agent operations - use IPC client methods
    async fn list_background_agents(&self, status: Option<String>) -> Result<Vec<BackgroundAgent>> {
        let mut client = self.client.lock().await;
        client.list_background_agents(status).await
    }

    async fn get_background_agent(&self, id: &str) -> Result<BackgroundAgent> {
        let mut client = self.client.lock().await;
        client
            .get_background_agent(id.to_string())
            .await?
            .ok_or_else(|| anyhow::anyhow!("Background agent not found: {}", id))
    }

    async fn create_background_agent(
        &self,
        spec: BackgroundAgentSpec,
        preview: bool,
        confirmation_token: Option<String>,
    ) -> Result<BackgroundAgentCommandOutcome<BackgroundAgent>> {
        let mut client = self.client.lock().await;
        client
            .create_background_agent(spec, preview, confirmation_token)
            .await
    }

    async fn convert_session_to_background_agent(
        &self,
        request: BackgroundAgentConvertSessionRequest,
    ) -> Result<BackgroundAgentCommandOutcome<BackgroundAgentConversionResult>> {
        let mut client = self.client.lock().await;
        client.convert_session_to_background_agent(request).await
    }

    async fn update_background_agent(
        &self,
        id: &str,
        patch: BackgroundAgentPatch,
        preview: bool,
        confirmation_token: Option<String>,
    ) -> Result<BackgroundAgentCommandOutcome<BackgroundAgent>> {
        let mut client = self.client.lock().await;
        client
            .update_background_agent(id.to_string(), patch, preview, confirmation_token)
            .await
    }

    async fn delete_background_agent(
        &self,
        id: &str,
        preview: bool,
        confirmation_token: Option<String>,
    ) -> Result<BackgroundAgentCommandOutcome<restflow_contracts::DeleteWithIdResponse>> {
        let mut client = self.client.lock().await;
        client
            .delete_background_agent(id.to_string(), preview, confirmation_token)
            .await
    }

    async fn control_background_agent(
        &self,
        id: &str,
        action: BackgroundAgentControlAction,
        preview: bool,
        confirmation_token: Option<String>,
    ) -> Result<BackgroundAgentCommandOutcome<BackgroundAgent>> {
        let mut client = self.client.lock().await;
        client
            .control_background_agent(id.to_string(), action, preview, confirmation_token)
            .await
    }

    async fn get_background_agent_progress(
        &self,
        id: &str,
        event_limit: Option<usize>,
    ) -> Result<BackgroundProgress> {
        self.request_typed(IpcRequest::GetBackgroundAgentProgress {
            id: id.to_string(),
            event_limit,
        })
        .await
    }

    async fn send_background_agent_message(&self, id: &str, message: &str) -> Result<()> {
        let _: BackgroundMessage = self
            .request_typed(IpcRequest::SendBackgroundAgentMessage {
                id: id.to_string(),
                message: message.to_string(),
                source: None::<String>,
            })
            .await?;
        Ok(())
    }

    async fn list_execution_sessions(
        &self,
        query: ExecutionSessionListQuery,
    ) -> Result<Vec<ExecutionSessionSummary>> {
        let mut client = self.client.lock().await;
        client.list_execution_sessions(query).await
    }

    async fn get_execution_run_timeline(&self, run_id: &str) -> Result<ExecutionTimeline> {
        let mut client = self.client.lock().await;
        client.get_execution_run_timeline(run_id.to_string()).await
    }

    // Shared Space operations - not yet in IPC protocol
    async fn list_kv_store(&self, _namespace: Option<&str>) -> Result<Vec<SharedEntry>> {
        bail!("Shared space operations require daemon mode. Use 'restflow daemon start' first.")
    }

    async fn get_kv_store(&self, _key: &str) -> Result<Option<SharedEntry>> {
        bail!("Shared space operations require daemon mode. Use 'restflow daemon start' first.")
    }

    async fn set_kv_store(
        &self,
        _key: &str,
        _value: &str,
        _visibility: &str,
    ) -> Result<SharedEntry> {
        bail!("Shared space operations require daemon mode. Use 'restflow daemon start' first.")
    }

    async fn delete_kv_store(&self, _key: &str) -> Result<bool> {
        bail!("Shared space operations require daemon mode. Use 'restflow daemon start' first.")
    }

    // Deliverable operations
    async fn list_deliverables(&self, _task_id: &str) -> Result<Vec<Deliverable>> {
        bail!("Deliverable operations are not yet available via CLI. Use MCP tools instead.")
    }
}
