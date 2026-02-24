use anyhow::{Result, bail};
use async_trait::async_trait;
use std::path::Path;
use tokio::sync::Mutex;

use crate::executor::CommandExecutor;
use restflow_core::daemon::{IpcClient, IpcRequest, IpcResponse};
use restflow_core::memory::ExportResult;
use restflow_core::models::{
    AgentNode, BackgroundAgent, BackgroundAgentControlAction, BackgroundAgentPatch,
    BackgroundAgentSpec, BackgroundProgress, ChatSession, ChatSessionSummary, Deliverable,
    ItemQuery, MemoryChunk, MemorySearchResult, MemoryStats, Secret, SharedEntry, Skill, WorkItem,
    WorkItemPatch, WorkItemSpec,
};
use restflow_core::storage::SystemConfig;
use restflow_core::storage::agent::StoredAgent;

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

    async fn request(&self, req: IpcRequest) -> Result<IpcResponse> {
        let mut client = self.client.lock().await;
        client.request(req).await
    }

    fn decode_response<T: serde::de::DeserializeOwned>(&self, response: IpcResponse) -> Result<T> {
        match response {
            IpcResponse::Success(value) => Ok(serde_json::from_value(value)?),
            IpcResponse::Error { message, .. } => bail!(message),
            IpcResponse::Pong => bail!("Unexpected Pong response"),
        }
    }

    fn decode_response_optional<T: serde::de::DeserializeOwned>(
        &self,
        response: IpcResponse,
    ) -> Result<Option<T>> {
        match response {
            IpcResponse::Success(value) => Ok(Some(serde_json::from_value(value)?)),
            IpcResponse::Error { code: 404, .. } => Ok(None),
            IpcResponse::Error { message, .. } => bail!(message),
            IpcResponse::Pong => bail!("Unexpected Pong response"),
        }
    }
}

#[async_trait]
impl CommandExecutor for IpcExecutor {
    async fn list_agents(&self) -> Result<Vec<StoredAgent>> {
        let response = self.request(IpcRequest::ListAgents).await?;
        self.decode_response(response)
    }

    async fn get_agent(&self, id: &str) -> Result<StoredAgent> {
        let response = self
            .request(IpcRequest::GetAgent { id: id.to_string() })
            .await?;
        self.decode_response(response)
    }

    async fn create_agent(&self, name: String, agent: AgentNode) -> Result<StoredAgent> {
        let response = self
            .request(IpcRequest::CreateAgent { name, agent })
            .await?;
        self.decode_response(response)
    }

    async fn update_agent(
        &self,
        id: &str,
        name: Option<String>,
        agent: Option<AgentNode>,
    ) -> Result<StoredAgent> {
        let response = self
            .request(IpcRequest::UpdateAgent {
                id: id.to_string(),
                name,
                agent,
            })
            .await?;
        self.decode_response(response)
    }

    async fn delete_agent(&self, id: &str) -> Result<()> {
        let response = self
            .request(IpcRequest::DeleteAgent { id: id.to_string() })
            .await?;
        self.decode_response::<serde_json::Value>(response)
            .map(|_| ())
    }

    async fn list_skills(&self) -> Result<Vec<Skill>> {
        let response = self.request(IpcRequest::ListSkills).await?;
        self.decode_response(response)
    }

    async fn get_skill(&self, id: &str) -> Result<Option<Skill>> {
        let response = self
            .request(IpcRequest::GetSkill { id: id.to_string() })
            .await?;
        self.decode_response_optional(response)
    }

    async fn create_skill(&self, skill: Skill) -> Result<()> {
        let response = self.request(IpcRequest::CreateSkill { skill }).await?;
        self.decode_response::<serde_json::Value>(response)
            .map(|_| ())
    }

    async fn update_skill(&self, id: &str, skill: Skill) -> Result<()> {
        let response = self
            .request(IpcRequest::UpdateSkill {
                id: id.to_string(),
                skill,
            })
            .await?;
        self.decode_response::<serde_json::Value>(response)
            .map(|_| ())
    }

    async fn delete_skill(&self, id: &str) -> Result<()> {
        let response = self
            .request(IpcRequest::DeleteSkill { id: id.to_string() })
            .await?;
        self.decode_response::<serde_json::Value>(response)
            .map(|_| ())
    }

    async fn search_memory(
        &self,
        query: String,
        agent_id: Option<String>,
        limit: Option<u32>,
    ) -> Result<MemorySearchResult> {
        let response = self
            .request(IpcRequest::SearchMemory {
                query,
                agent_id,
                limit,
            })
            .await?;
        self.decode_response(response)
    }

    async fn list_memory(
        &self,
        agent_id: Option<String>,
        tag: Option<String>,
    ) -> Result<Vec<MemoryChunk>> {
        let response = self
            .request(IpcRequest::ListMemory { agent_id, tag })
            .await?;
        self.decode_response(response)
    }

    async fn clear_memory(&self, agent_id: Option<String>) -> Result<u32> {
        let response = self.request(IpcRequest::ClearMemory { agent_id }).await?;
        #[derive(serde::Deserialize)]
        struct ClearResponse {
            deleted: u32,
        }
        let resp: ClearResponse = self.decode_response(response)?;
        Ok(resp.deleted)
    }

    async fn get_memory_stats(&self, agent_id: Option<String>) -> Result<MemoryStats> {
        let response = self
            .request(IpcRequest::GetMemoryStats { agent_id })
            .await?;
        self.decode_response(response)
    }

    async fn export_memory(&self, agent_id: Option<String>) -> Result<ExportResult> {
        let response = self.request(IpcRequest::ExportMemory { agent_id }).await?;
        self.decode_response(response)
    }

    async fn store_memory(
        &self,
        agent_id: &str,
        content: &str,
        tags: Vec<String>,
    ) -> Result<String> {
        #[derive(serde::Deserialize)]
        struct StoreResponse {
            id: String,
        }
        let response = self
            .request(IpcRequest::AddMemory {
                content: content.to_string(),
                agent_id: Some(agent_id.to_string()),
                tags,
            })
            .await?;
        let resp: StoreResponse = self.decode_response(response)?;
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
        let response = self.request(IpcRequest::ListWorkItems { query }).await?;
        self.decode_response(response)
    }

    async fn get_note(&self, id: &str) -> Result<Option<WorkItem>> {
        let response = self
            .request(IpcRequest::GetWorkItem { id: id.to_string() })
            .await?;
        self.decode_response_optional(response)
    }

    async fn create_note(&self, spec: WorkItemSpec) -> Result<WorkItem> {
        let response = self.request(IpcRequest::CreateWorkItem { spec }).await?;
        self.decode_response(response)
    }

    async fn update_note(&self, id: &str, patch: WorkItemPatch) -> Result<WorkItem> {
        let response = self
            .request(IpcRequest::UpdateWorkItem {
                id: id.to_string(),
                patch,
            })
            .await?;
        self.decode_response(response)
    }

    async fn delete_note(&self, id: &str) -> Result<()> {
        let response = self
            .request(IpcRequest::DeleteWorkItem { id: id.to_string() })
            .await?;
        self.decode_response::<serde_json::Value>(response)
            .map(|_| ())
    }

    async fn list_note_folders(&self) -> Result<Vec<String>> {
        let response = self.request(IpcRequest::ListWorkItemFolders).await?;
        self.decode_response(response)
    }

    async fn list_secrets(&self) -> Result<Vec<Secret>> {
        let response = self.request(IpcRequest::ListSecrets).await?;
        self.decode_response(response)
    }

    async fn set_secret(&self, key: &str, value: &str, description: Option<String>) -> Result<()> {
        let response = self
            .request(IpcRequest::SetSecret {
                key: key.to_string(),
                value: value.to_string(),
                description,
            })
            .await?;
        self.decode_response::<serde_json::Value>(response)
            .map(|_| ())
    }

    async fn create_secret(
        &self,
        key: &str,
        value: &str,
        description: Option<String>,
    ) -> Result<()> {
        let response = self
            .request(IpcRequest::CreateSecret {
                key: key.to_string(),
                value: value.to_string(),
                description,
            })
            .await?;
        self.decode_response::<serde_json::Value>(response)
            .map(|_| ())
    }

    async fn update_secret(
        &self,
        key: &str,
        value: &str,
        description: Option<String>,
    ) -> Result<()> {
        let response = self
            .request(IpcRequest::UpdateSecret {
                key: key.to_string(),
                value: value.to_string(),
                description,
            })
            .await?;
        self.decode_response::<serde_json::Value>(response)
            .map(|_| ())
    }

    async fn delete_secret(&self, key: &str) -> Result<()> {
        let response = self
            .request(IpcRequest::DeleteSecret {
                key: key.to_string(),
            })
            .await?;
        self.decode_response::<serde_json::Value>(response)
            .map(|_| ())
    }

    async fn has_secret(&self, key: &str) -> Result<bool> {
        let response = self
            .request(IpcRequest::GetSecret {
                key: key.to_string(),
            })
            .await?;
        match response {
            IpcResponse::Success(_) => Ok(true),
            IpcResponse::Error { code: 404, .. } => Ok(false),
            IpcResponse::Error { message, .. } => bail!(message),
            IpcResponse::Pong => bail!("Unexpected Pong response"),
        }
    }

    async fn get_config(&self) -> Result<SystemConfig> {
        let response = self.request(IpcRequest::GetConfig).await?;
        self.decode_response(response)
    }

    async fn set_config(&self, config: SystemConfig) -> Result<()> {
        let response = self.request(IpcRequest::SetConfig { config }).await?;
        self.decode_response::<serde_json::Value>(response)
            .map(|_| ())
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

    async fn create_background_agent(&self, spec: BackgroundAgentSpec) -> Result<BackgroundAgent> {
        let mut client = self.client.lock().await;
        client.create_background_agent(spec).await
    }

    async fn update_background_agent(
        &self,
        id: &str,
        patch: BackgroundAgentPatch,
    ) -> Result<BackgroundAgent> {
        let mut client = self.client.lock().await;
        client.update_background_agent(id.to_string(), patch).await
    }

    async fn delete_background_agent(&self, id: &str) -> Result<()> {
        let mut client = self.client.lock().await;
        client.delete_background_agent(id.to_string()).await?;
        Ok(())
    }

    async fn control_background_agent(
        &self,
        id: &str,
        action: BackgroundAgentControlAction,
    ) -> Result<()> {
        let mut client = self.client.lock().await;
        client
            .control_background_agent(id.to_string(), action)
            .await?;
        Ok(())
    }

    async fn get_background_agent_progress(
        &self,
        id: &str,
        event_limit: Option<usize>,
    ) -> Result<BackgroundProgress> {
        let response = self
            .request(IpcRequest::GetBackgroundAgentProgress {
                id: id.to_string(),
                event_limit,
            })
            .await?;
        self.decode_response(response)
    }

    async fn send_background_agent_message(&self, id: &str, message: &str) -> Result<()> {
        let response = self
            .request(IpcRequest::SendBackgroundAgentMessage {
                id: id.to_string(),
                message: message.to_string(),
                source: None,
            })
            .await?;
        self.decode_response::<serde_json::Value>(response)
            .map(|_| ())
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
