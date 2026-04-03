use anyhow::Result;
use async_trait::async_trait;
use restflow_contracts::{
    CleanupReportResponse, PairingApprovalResponse, PairingOwnerResponse, PairingStateResponse,
    RouteBindingResponse, SessionSourceMigrationResponse,
    request::BackgroundAgentConvertSessionRequest,
};
use restflow_core::daemon::is_daemon_available;
use restflow_core::memory::ExportResult;
use restflow_core::models::{
    AgentNode, BackgroundAgent, BackgroundAgentControlAction, BackgroundAgentConversionResult,
    BackgroundAgentPatch, BackgroundAgentSpec, BackgroundProgress, ChatSession, ChatSessionSummary,
    Deliverable, ExecutionSessionListQuery, ExecutionSessionSummary, ExecutionTimeline, Hook,
    ItemQuery, MemoryChunk, MemorySearchResult, MemoryStats, Secret, SharedEntry, Skill, WorkItem,
    WorkItemPatch, WorkItemSpec,
};
use restflow_core::paths;
use restflow_core::storage::SystemConfig;
use restflow_core::storage::agent::StoredAgent;
use std::sync::Arc;

// DirectExecutor exists only for isolated command tests. Production CLI commands always
// reach hook/runtime mutations through the daemon-backed IpcExecutor returned by create().
#[cfg(test)]
pub mod direct;
pub mod ipc;

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

    async fn list_skills(&self) -> Result<Vec<Skill>>;
    async fn get_skill(&self, id: &str) -> Result<Option<Skill>>;
    async fn create_skill(&self, skill: Skill) -> Result<()>;
    async fn update_skill(&self, id: &str, skill: Skill) -> Result<()>;
    async fn delete_skill(&self, id: &str) -> Result<()>;

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
    async fn store_memory(
        &self,
        agent_id: &str,
        content: &str,
        tags: Vec<String>,
    ) -> Result<String>;

    async fn list_sessions(&self) -> Result<Vec<ChatSessionSummary>>;
    async fn get_session(&self, id: &str) -> Result<ChatSession>;
    async fn create_session(&self, agent_id: String, model: String) -> Result<ChatSession>;
    async fn delete_session(&self, id: &str) -> Result<bool>;
    async fn search_sessions(&self, query: String) -> Result<Vec<ChatSessionSummary>>;

    async fn list_notes(&self, query: ItemQuery) -> Result<Vec<WorkItem>>;
    async fn get_note(&self, id: &str) -> Result<Option<WorkItem>>;
    async fn create_note(&self, spec: WorkItemSpec) -> Result<WorkItem>;
    async fn update_note(&self, id: &str, patch: WorkItemPatch) -> Result<WorkItem>;
    async fn delete_note(&self, id: &str) -> Result<()>;
    async fn list_note_folders(&self) -> Result<Vec<String>>;

    async fn list_secrets(&self) -> Result<Vec<Secret>>;
    async fn set_secret(&self, key: &str, value: &str, description: Option<String>) -> Result<()>;
    #[allow(dead_code)]
    async fn create_secret(
        &self,
        key: &str,
        value: &str,
        description: Option<String>,
    ) -> Result<()>;
    #[allow(dead_code)]
    async fn update_secret(
        &self,
        key: &str,
        value: &str,
        description: Option<String>,
    ) -> Result<()>;
    async fn delete_secret(&self, key: &str) -> Result<()>;
    async fn has_secret(&self, key: &str) -> Result<bool>;

    async fn get_config(&self) -> Result<SystemConfig>;
    async fn get_global_config(&self) -> Result<SystemConfig>;
    async fn set_config(&self, config: SystemConfig) -> Result<()>;

    async fn list_hooks(&self) -> Result<Vec<Hook>>;
    async fn create_hook(&self, hook: Hook) -> Result<Hook>;
    async fn update_hook(&self, id: &str, hook: Hook) -> Result<Hook>;
    async fn delete_hook(&self, id: &str) -> Result<bool>;
    async fn test_hook(&self, id: &str) -> Result<()>;

    async fn list_pairing_state(&self) -> Result<PairingStateResponse>;
    async fn approve_pairing(&self, code: &str) -> Result<PairingApprovalResponse>;
    async fn deny_pairing(&self, code: &str) -> Result<()>;
    async fn revoke_paired_peer(&self, peer_id: &str) -> Result<bool>;
    async fn get_pairing_owner(&self) -> Result<PairingOwnerResponse>;
    async fn set_pairing_owner(&self, chat_id: &str) -> Result<PairingOwnerResponse>;

    async fn list_route_bindings(&self) -> Result<Vec<RouteBindingResponse>>;
    async fn bind_route(
        &self,
        binding_type: &str,
        target_id: &str,
        agent_id: &str,
    ) -> Result<RouteBindingResponse>;
    async fn unbind_route(&self, id: &str) -> Result<bool>;

    async fn run_cleanup(&self) -> Result<CleanupReportResponse>;
    async fn migrate_session_sources(
        &self,
        dry_run: bool,
    ) -> Result<SessionSourceMigrationResponse>;

    // Background Agent operations
    async fn list_background_agents(&self, status: Option<String>) -> Result<Vec<BackgroundAgent>>;
    async fn get_background_agent(&self, id: &str) -> Result<BackgroundAgent>;
    async fn create_background_agent(&self, spec: BackgroundAgentSpec) -> Result<BackgroundAgent>;
    async fn convert_session_to_background_agent(
        &self,
        request: BackgroundAgentConvertSessionRequest,
    ) -> Result<BackgroundAgentConversionResult>;
    async fn update_background_agent(
        &self,
        id: &str,
        patch: BackgroundAgentPatch,
    ) -> Result<BackgroundAgent>;
    async fn delete_background_agent(
        &self,
        id: &str,
    ) -> Result<restflow_contracts::DeleteWithIdResponse>;
    async fn control_background_agent(
        &self,
        id: &str,
        action: BackgroundAgentControlAction,
    ) -> Result<BackgroundAgent>;
    async fn get_background_agent_progress(
        &self,
        id: &str,
        event_limit: Option<usize>,
    ) -> Result<BackgroundProgress>;
    async fn send_background_agent_message(&self, id: &str, message: &str) -> Result<()>;
    async fn list_execution_sessions(
        &self,
        query: ExecutionSessionListQuery,
    ) -> Result<Vec<ExecutionSessionSummary>>;
    async fn get_execution_run_timeline(&self, run_id: &str) -> Result<ExecutionTimeline>;

    // Shared Space operations
    async fn list_kv_store(&self, namespace: Option<&str>) -> Result<Vec<SharedEntry>>;
    async fn get_kv_store(&self, key: &str) -> Result<Option<SharedEntry>>;
    async fn set_kv_store(&self, key: &str, value: &str, visibility: &str) -> Result<SharedEntry>;
    async fn delete_kv_store(&self, key: &str) -> Result<bool>;

    // Deliverable operations
    async fn list_deliverables(&self, task_id: &str) -> Result<Vec<Deliverable>>;
}

pub async fn create(db_path: Option<String>) -> Result<Arc<dyn CommandExecutor>> {
    if let Some(db_path) = db_path {
        anyhow::bail!(
            "The --db-path flag is only supported for daemon lifecycle commands. Commands routed through the daemon must target the running daemon instance instead of selecting a database path directly: {}",
            db_path
        );
    }

    // This is the only production executor entrypoint for daemon-routed commands.
    let socket_path = paths::socket_path()?;
    if is_daemon_available(&socket_path).await {
        let executor = ipc::IpcExecutor::connect(&socket_path).await?;
        return Ok(Arc::new(executor));
    }

    anyhow::bail!("RestFlow daemon is not running. Start it with 'restflow daemon start'.")
}

#[cfg(test)]
#[allow(clippy::await_holding_lock)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::test_support::env_lock()
    }

    #[tokio::test]
    async fn create_requires_running_daemon() {
        let _guard = env_lock();
        let temp = tempdir().expect("tempdir");
        let prev = std::env::var_os("RESTFLOW_DIR");
        unsafe { std::env::set_var("RESTFLOW_DIR", temp.path()) };

        let err = match create(None).await {
            Ok(_) => panic!("create should fail without daemon"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("daemon is not running"));

        match prev {
            Some(value) => unsafe { std::env::set_var("RESTFLOW_DIR", value) },
            None => unsafe { std::env::remove_var("RESTFLOW_DIR") },
        }
    }

    #[tokio::test]
    async fn create_rejects_db_path_for_executor_commands() {
        let err = match create(Some("/tmp/restflow.db".to_string())).await {
            Ok(_) => panic!("create should reject db_path for daemon-routed commands"),
            Err(err) => err,
        };
        assert!(
            err.to_string()
                .contains("only supported for daemon lifecycle commands")
        );
    }
}
