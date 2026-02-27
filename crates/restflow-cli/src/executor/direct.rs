use anyhow::{Result, bail};
use async_trait::async_trait;
use std::sync::Arc;

use crate::executor::CommandExecutor;
use crate::setup;
use restflow_core::memory::{ExportResult, MemoryExporter};
use restflow_core::models::{
    AgentNode, BackgroundAgent, BackgroundAgentControlAction, BackgroundAgentPatch,
    BackgroundAgentSpec, BackgroundProgress, Deliverable, SharedEntry, ToolTrace,
};
use restflow_core::services::{
    agent as agent_service, config as config_service, secrets as secrets_service,
    skills as skills_service,
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
        self.core.storage.chat_sessions.delete(id)
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

    async fn set_config(&self, config: SystemConfig) -> Result<()> {
        config_service::update_config(&self.core, config).await
    }

    // Background Agent operations - require daemon
    async fn list_background_agents(
        &self,
        _status: Option<String>,
    ) -> Result<Vec<BackgroundAgent>> {
        bail!("Background agent operations require daemon mode. Use 'restflow daemon start' first.")
    }

    async fn get_background_agent(&self, _id: &str) -> Result<BackgroundAgent> {
        bail!("Background agent operations require daemon mode. Use 'restflow daemon start' first.")
    }

    async fn create_background_agent(&self, _spec: BackgroundAgentSpec) -> Result<BackgroundAgent> {
        bail!("Background agent operations require daemon mode. Use 'restflow daemon start' first.")
    }

    async fn update_background_agent(
        &self,
        _id: &str,
        _patch: BackgroundAgentPatch,
    ) -> Result<BackgroundAgent> {
        bail!("Background agent operations require daemon mode. Use 'restflow daemon start' first.")
    }

    async fn delete_background_agent(&self, _id: &str) -> Result<()> {
        bail!("Background agent operations require daemon mode. Use 'restflow daemon start' first.")
    }

    async fn control_background_agent(
        &self,
        _id: &str,
        _action: BackgroundAgentControlAction,
    ) -> Result<()> {
        bail!("Background agent operations require daemon mode. Use 'restflow daemon start' first.")
    }

    async fn get_background_agent_progress(
        &self,
        _id: &str,
        _event_limit: Option<usize>,
    ) -> Result<BackgroundProgress> {
        bail!("Background agent operations require daemon mode. Use 'restflow daemon start' first.")
    }

    async fn send_background_agent_message(&self, _id: &str, _message: &str) -> Result<()> {
        bail!("Background agent operations require daemon mode. Use 'restflow daemon start' first.")
    }

    async fn list_tool_traces(
        &self,
        _session_id: &str,
        _turn_id: Option<String>,
        _limit: Option<usize>,
    ) -> Result<Vec<ToolTrace>> {
        bail!("Background agent operations require daemon mode. Use 'restflow daemon start' first.")
    }

    // Shared Space operations - require daemon
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
