use anyhow::{Result, bail};
use async_trait::async_trait;
use std::sync::Arc;

use crate::executor::CommandExecutor;
use crate::setup;
use restflow_contracts::{
    AllowedPeerResponse, CleanupReportResponse, PairingApprovalResponse, PairingOwnerResponse,
    PairingRequestResponse, PairingStateResponse, RouteBindingResponse,
    SessionSourceMigrationResponse, request::TaskFromSessionRequest,
};
use restflow_core::channel::pairing::PairingManager;
use restflow_core::channel::route_binding::{RouteBindingType, RouteResolver};
use restflow_core::memory::{ExportResult, MemoryExporter};
use restflow_core::models::{
    AgentNode, Deliverable, ExecutionTimeline, ExecutionTraceQuery, Hook, RunListQuery, RunSummary,
    SharedEntry, Task, TaskControlAction, TaskConversionResult, TaskPatch, TaskProgress, TaskSpec,
};
use restflow_core::services::{
    agent as agent_service, config as config_service, execution_console::ExecutionConsoleService,
    secrets as secrets_service, session::SessionService, skills as skills_service,
};
use restflow_core::storage::SystemConfig;
use restflow_core::storage::agent::StoredAgent;
use restflow_core::{
    AppCore,
    models::{
        ChatSession, ChatSessionSource, ChatSessionSummary, ItemQuery, MemoryChunk,
        MemorySearchResult, MemoryStats, Secret, Skill, WorkItem, WorkItemPatch, WorkItemSpec,
    },
};
use restflow_storage::PairingStorage;

const TELEGRAM_CHAT_ID_SECRET: &str = "TELEGRAM_CHAT_ID";
const TELEGRAM_DEFAULT_CHAT_ID_SECRET: &str = "TELEGRAM_DEFAULT_CHAT_ID";
const HOOK_DAEMON_MODE_MESSAGE: &str =
    "Hook operations require daemon mode. Use 'restflow daemon start' first.";

/// Test-only executor used by command unit tests.
///
/// This module is compiled behind `#[cfg(test)]`; production CLI commands use
/// `executor::create()` and mutate hook/runtime state through the daemon-backed
/// `IpcExecutor` instead of calling storage services directly.
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

    async fn create_skill(&self, skill: Skill) -> Result<()> {
        skills_service::create_skill(&self.core, skill).await
    }

    async fn update_skill(&self, id: &str, skill: Skill) -> Result<()> {
        skills_service::update_skill(&self.core, id, &skill).await
    }

    async fn delete_skill(&self, id: &str) -> Result<()> {
        skills_service::delete_skill(&self.core, id).await
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

    async fn list_sessions(&self) -> Result<Vec<ChatSessionSummary>> {
        self.core.storage.chat_sessions.list_summaries()
    }

    async fn get_session(&self, id: &str) -> Result<ChatSession> {
        self.core
            .storage
            .chat_sessions
            .get(id)?
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", id))
    }

    async fn create_session(&self, agent_id: String, model: String) -> Result<ChatSession> {
        let mut session = ChatSession::new(agent_id, model);
        session.source_channel = Some(ChatSessionSource::Workspace);
        self.core.storage.chat_sessions.create(&session)?;
        Ok(session)
    }

    async fn delete_session(&self, id: &str) -> Result<bool> {
        let sessions = SessionService::from_storage(&self.core.storage);
        sessions.delete_workspace_session(id)
    }

    async fn search_sessions(&self, query: String) -> Result<Vec<ChatSessionSummary>> {
        let query = query.to_lowercase();
        let sessions = self.core.storage.chat_sessions.list()?;
        let matches: Vec<ChatSessionSummary> = sessions
            .into_iter()
            .filter(|session| {
                session.name.to_lowercase().contains(&query)
                    || session
                        .messages
                        .iter()
                        .any(|message| message.content.to_lowercase().contains(&query))
            })
            .map(|session| ChatSessionSummary::from(&session))
            .collect();
        Ok(matches)
    }

    async fn list_notes(&self, query: ItemQuery) -> Result<Vec<WorkItem>> {
        self.core.storage.work_items.list_notes(query)
    }

    async fn get_note(&self, id: &str) -> Result<Option<WorkItem>> {
        self.core.storage.work_items.get_note(id)
    }

    async fn create_note(&self, spec: WorkItemSpec) -> Result<WorkItem> {
        self.core.storage.work_items.create_note(spec)
    }

    async fn update_note(&self, id: &str, patch: WorkItemPatch) -> Result<WorkItem> {
        self.core.storage.work_items.update_note(id, patch)
    }

    async fn delete_note(&self, id: &str) -> Result<()> {
        self.core.storage.work_items.delete_note(id)
    }

    async fn list_note_folders(&self) -> Result<Vec<String>> {
        self.core.storage.work_items.list_folders()
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

    async fn list_hooks(&self) -> Result<Vec<Hook>> {
        bail!(HOOK_DAEMON_MODE_MESSAGE)
    }

    async fn create_hook(&self, _hook: Hook) -> Result<Hook> {
        bail!(HOOK_DAEMON_MODE_MESSAGE)
    }

    async fn update_hook(&self, _id: &str, _hook: Hook) -> Result<Hook> {
        bail!(HOOK_DAEMON_MODE_MESSAGE)
    }

    async fn delete_hook(&self, _id: &str) -> Result<bool> {
        bail!(HOOK_DAEMON_MODE_MESSAGE)
    }

    async fn test_hook(&self, _id: &str) -> Result<()> {
        bail!(HOOK_DAEMON_MODE_MESSAGE)
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
            background_tasks: report.background_tasks,
            checkpoints: report.checkpoints,
            memory_chunks: report.memory_chunks,
            memory_sessions: report.memory_sessions,
            vector_orphans: report.vector_orphans,
            daemon_log_files: report.daemon_log_files,
        })
    }

    async fn migrate_session_sources(
        &self,
        dry_run: bool,
    ) -> Result<SessionSourceMigrationResponse> {
        let stats = self
            .core
            .storage
            .chat_sessions
            .migrate_legacy_channel_sources(dry_run)?;
        Ok(SessionSourceMigrationResponse {
            dry_run,
            scanned: stats.scanned,
            migrated: stats.migrated,
            skipped: stats.skipped,
            failed: stats.failed,
        })
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

    async fn list_execution_sessions(&self, query: RunListQuery) -> Result<Vec<RunSummary>> {
        ExecutionConsoleService::from_storage(&self.core.storage).list_execution_sessions(&query)
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

    // Deliverable operations - require daemon
    async fn list_deliverables(&self, _task_id: &str) -> Result<Vec<Deliverable>> {
        bail!("Deliverable operations require daemon mode. Use 'restflow daemon start' first.")
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

fn normalize_route_binding_type(binding_type: &str, target_id: &str) -> RouteBindingType {
    match binding_type {
        "peer" => RouteBindingType::Peer,
        "default" => RouteBindingType::Default,
        "group" => {
            tracing::warn!(
                target_id = %target_id,
                "Using deprecated --group flag, consider using --channel instead"
            );
            RouteBindingType::Channel
        }
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

#[cfg(test)]
mod tests {
    use super::{DirectExecutor, HOOK_DAEMON_MODE_MESSAGE};
    use crate::executor::CommandExecutor;
    use restflow_core::models::{Hook, HookAction, HookEvent};
    use tempfile::tempdir;

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::test_support::env_lock()
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn hook_operations_require_daemon_mode_even_in_direct_executor() {
        let _guard = env_lock();
        let temp = tempdir().expect("tempdir");
        let prev = std::env::var_os("RESTFLOW_DIR");
        unsafe { std::env::set_var("RESTFLOW_DIR", temp.path()) };

        let executor = DirectExecutor::connect(None)
            .await
            .expect("direct executor");
        let hook = Hook::new(
            "notify".to_string(),
            HookEvent::TaskStarted,
            HookAction::Webhook {
                url: "https://example.com/hook".to_string(),
                method: None,
                headers: None,
            },
        );

        let list_err = executor.list_hooks().await.expect_err("list should fail");
        assert_eq!(list_err.to_string(), HOOK_DAEMON_MODE_MESSAGE);

        let create_err = executor
            .create_hook(hook.clone())
            .await
            .expect_err("create should fail");
        assert_eq!(create_err.to_string(), HOOK_DAEMON_MODE_MESSAGE);

        let update_err = executor
            .update_hook("hook-1", hook)
            .await
            .expect_err("update should fail");
        assert_eq!(update_err.to_string(), HOOK_DAEMON_MODE_MESSAGE);

        let delete_err = executor
            .delete_hook("hook-1")
            .await
            .expect_err("delete should fail");
        assert_eq!(delete_err.to_string(), HOOK_DAEMON_MODE_MESSAGE);

        let test_err = executor
            .test_hook("hook-1")
            .await
            .expect_err("test should fail");
        assert_eq!(test_err.to_string(), HOOK_DAEMON_MODE_MESSAGE);

        match prev {
            Some(value) => unsafe { std::env::set_var("RESTFLOW_DIR", value) },
            None => unsafe { std::env::remove_var("RESTFLOW_DIR") },
        }
    }
}
