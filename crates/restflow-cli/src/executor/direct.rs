use anyhow::{Result, bail};
use async_trait::async_trait;
use std::sync::Arc;

use crate::executor::CommandExecutor;
use crate::setup;
use restflow_contracts::{
    AllowedPeerResponse, CleanupReportResponse, PairingApprovalResponse, PairingOwnerResponse,
    PairingRequestResponse, PairingStateResponse, RouteBindingResponse,
    request::TaskFromSessionRequest,
};
use restflow_core::channel::pairing::PairingManager;
use restflow_core::channel::route_binding::{RouteBindingType, RouteResolver};
use restflow_core::memory::{ExportResult, MemoryExporter};
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
    models::{MemoryChunk, MemorySearchResult, MemoryStats, Secret, Skill},
};
use restflow_storage::PairingStorage;

const TELEGRAM_CHAT_ID_SECRET: &str = "TELEGRAM_CHAT_ID";
const TELEGRAM_DEFAULT_CHAT_ID_SECRET: &str = "TELEGRAM_DEFAULT_CHAT_ID";
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

    async fn search_memory(
        &self,
        query: String,
        agent_id: Option<String>,
        _limit: Option<u32>,
    ) -> Result<MemorySearchResult> {
        let agent_id = resolve_agent_id(&self.core, agent_id).await?;
        let search =
            restflow_core::models::memory::MemorySearchQuery::new(agent_id).with_query(query);
        let results = self.core.storage.memory.search(&search)?;
        Ok(results)
    }

    async fn list_memory(
        &self,
        agent_id: Option<String>,
        tag: Option<String>,
    ) -> Result<Vec<MemoryChunk>> {
        match (agent_id, tag) {
            (Some(agent_id), Some(tag)) => Ok(self
                .core
                .storage
                .memory
                .list_chunks(&agent_id)?
                .into_iter()
                .filter(|chunk| chunk.tags.iter().any(|value| value == &tag))
                .collect()),
            (Some(agent_id), None) => self.core.storage.memory.list_chunks(&agent_id),
            (None, Some(tag)) => self.core.storage.memory.list_chunks_by_tag(&tag),
            (None, None) => {
                let agent_id = resolve_agent_id(&self.core, None).await?;
                self.core.storage.memory.list_chunks(&agent_id)
            }
        }
    }

    async fn clear_memory(&self, agent_id: Option<String>) -> Result<u32> {
        let agent_id = resolve_agent_id(&self.core, agent_id).await?;
        self.core.storage.memory.delete_chunks_for_agent(&agent_id)
    }

    async fn get_memory_stats(&self, agent_id: Option<String>) -> Result<MemoryStats> {
        let agent_id = resolve_agent_id(&self.core, agent_id).await?;
        self.core.storage.memory.get_stats(&agent_id)
    }

    async fn export_memory(&self, agent_id: Option<String>) -> Result<ExportResult> {
        let agent_id = resolve_agent_id(&self.core, agent_id).await?;
        let exporter = MemoryExporter::new(self.core.storage.memory.clone());
        exporter.export_agent(&agent_id)
    }

    async fn store_memory(
        &self,
        agent_id: &str,
        content: &str,
        tags: Vec<String>,
    ) -> Result<String> {
        use restflow_core::models::{MemoryChunk, MemorySource};
        let mut chunk = MemoryChunk::new(agent_id.to_string(), content.to_string())
            .with_source(MemorySource::ManualNote);
        if !tags.is_empty() {
            chunk = chunk.with_tags(tags);
        }
        let id = self.core.storage.memory.store_chunk(&chunk)?;
        Ok(id)
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

    async fn list_pairing_state(&self) -> Result<PairingStateResponse> {
        let manager = pairing_manager(&self.core)?;
        Ok(PairingStateResponse {
            allowed_peers: manager
                .list_allowed()?
                .into_iter()
                .map(|peer| AllowedPeerResponse {
                    peer_id: peer.peer_id,
                    peer_name: peer.peer_name,
                    approved_at: peer.approved_at,
                    approved_by: peer.approved_by,
                })
                .collect(),
            pending_requests: manager
                .list_pending()?
                .into_iter()
                .map(|request| PairingRequestResponse {
                    code: request.code,
                    peer_id: request.peer_id,
                    peer_name: request.peer_name,
                    chat_id: request.chat_id,
                    created_at: request.created_at,
                    expires_at: request.expires_at,
                })
                .collect(),
        })
    }

    async fn approve_pairing(&self, code: &str) -> Result<PairingApprovalResponse> {
        let manager = pairing_manager(&self.core)?;
        let (peer, request) = manager.approve_with_request(code, "cli")?;
        let owner_auto_bound =
            auto_bind_owner_chat_id_if_missing(&self.core.storage.secrets, &request.chat_id)?;
        let owner = resolve_owner_chat_id(&self.core.storage.secrets)?;
        Ok(PairingApprovalResponse {
            approved: true,
            peer_id: peer.peer_id,
            peer_name: peer.peer_name,
            owner_chat_id: owner.map(|value| value.0),
            owner_auto_bound,
        })
    }

    async fn deny_pairing(&self, code: &str) -> Result<()> {
        pairing_manager(&self.core)?.deny(code)
    }

    async fn revoke_paired_peer(&self, peer_id: &str) -> Result<bool> {
        pairing_manager(&self.core)?.revoke(peer_id)
    }

    async fn get_pairing_owner(&self) -> Result<PairingOwnerResponse> {
        let owner = resolve_owner_chat_id(&self.core.storage.secrets)?;
        Ok(PairingOwnerResponse {
            owner_chat_id: owner.as_ref().map(|value| value.0.clone()),
            source: owner.map(|value| value.1),
        })
    }

    async fn set_pairing_owner(&self, chat_id: &str) -> Result<PairingOwnerResponse> {
        let normalized_chat_id = chat_id.trim();
        if normalized_chat_id.is_empty() {
            bail!("chat_id cannot be empty");
        }
        self.core
            .storage
            .secrets
            .set_secret(TELEGRAM_CHAT_ID_SECRET, normalized_chat_id, None)?;
        Ok(PairingOwnerResponse {
            owner_chat_id: Some(normalized_chat_id.to_string()),
            source: Some(TELEGRAM_CHAT_ID_SECRET.to_string()),
        })
    }

    async fn list_route_bindings(&self) -> Result<Vec<RouteBindingResponse>> {
        route_resolver(&self.core)?
            .list()?
            .into_iter()
            .map(route_binding_response)
            .collect()
    }

    async fn bind_route(
        &self,
        binding_type: &str,
        target_id: &str,
        agent_id: &str,
    ) -> Result<RouteBindingResponse> {
        let binding_type = normalize_route_binding_type(binding_type, target_id);
        let binding = route_resolver(&self.core)?.bind(binding_type, target_id, agent_id)?;
        route_binding_response(binding)
    }

    async fn unbind_route(&self, id: &str) -> Result<bool> {
        route_resolver(&self.core)?.unbind(id)
    }

    async fn run_cleanup(&self) -> Result<CleanupReportResponse> {
        let report = restflow_core::services::cleanup::run_cleanup(&self.core).await?;
        Ok(CleanupReportResponse {
            chat_sessions: report.chat_sessions,
            tasks: report.tasks,
            checkpoints: report.checkpoints,
            memory_chunks: report.memory_chunks,
            audit_events: report.audit_events,
            telemetry_metric_samples: 0,
            memory_sessions: report.memory_sessions,
            vector_orphans: report.vector_orphans,
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

    async fn list_full_sessions(&self) -> Result<Vec<ChatSession>> {
        SessionService::from_storage(&self.core.storage).list_session_views(None, None, false)
    }

    async fn get_session(&self, id: &str) -> Result<ChatSession> {
        SessionService::from_storage(&self.core.storage)
            .get_session_view(id)?
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", id))
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

    async fn delete_task(&self, _id: &str) -> Result<restflow_contracts::DeleteWithIdResponse> {
        bail!("Task operations require daemon mode. Use 'restflow daemon start' first.")
    }

    async fn control_task(&self, _id: &str, _action: TaskControlAction) -> Result<Task> {
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

fn pairing_manager(core: &Arc<AppCore>) -> Result<PairingManager> {
    let storage = Arc::new(PairingStorage::new(core.storage.get_db())?);
    Ok(PairingManager::new(storage))
}

fn route_resolver(core: &Arc<AppCore>) -> Result<RouteResolver> {
    let storage = Arc::new(PairingStorage::new(core.storage.get_db())?);
    Ok(RouteResolver::new(storage))
}

fn resolve_owner_chat_id(
    secrets: &restflow_core::storage::SecretStorage,
) -> Result<Option<(String, String)>> {
    if let Some(value) = secrets.get_non_empty(TELEGRAM_CHAT_ID_SECRET)? {
        return Ok(Some((value, TELEGRAM_CHAT_ID_SECRET.to_string())));
    }

    if let Some(value) = secrets.get_non_empty(TELEGRAM_DEFAULT_CHAT_ID_SECRET)? {
        return Ok(Some((value, TELEGRAM_DEFAULT_CHAT_ID_SECRET.to_string())));
    }

    Ok(None)
}

fn auto_bind_owner_chat_id_if_missing(
    secrets: &restflow_core::storage::SecretStorage,
    chat_id: &str,
) -> Result<bool> {
    if resolve_owner_chat_id(secrets)?.is_some() {
        return Ok(false);
    }
    secrets.set_secret(TELEGRAM_CHAT_ID_SECRET, chat_id, None)?;
    Ok(true)
}

fn normalize_route_binding_type(binding_type: &str, _target_id: &str) -> RouteBindingType {
    match binding_type {
        "peer" => RouteBindingType::Peer,
        "account" => RouteBindingType::Account,
        "channel" => RouteBindingType::Channel,
        "default" => RouteBindingType::Default,
        _ => RouteBindingType::Channel,
    }
}

fn route_binding_response(
    binding: restflow_core::channel::route_binding::RouteBinding,
) -> Result<RouteBindingResponse> {
    Ok(RouteBindingResponse {
        id: binding.id,
        binding_type: binding.binding_type.to_string(),
        target_id: binding.target_id,
        agent_id: binding.agent_id,
        created_at: binding.created_at,
        priority: binding.priority,
    })
}
