use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;
use tokio::sync::Mutex;
use types::{CleanupReportResponse, OkResponse};

use crate::executor::CommandExecutor;
use runtime::Secret;
use runtime::StoredAgent;
use runtime::daemon::request_mapper::to_contract;
use runtime::daemon::{IpcClient, IpcRequest};
use runtime::storage::SystemConfig;
use types::{AgentNode, ChatSession, ChatSessionSummary, Skill};

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
        self.request_typed(IpcRequest::CreateAgent { name, agent })
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
            .request_optional::<types::SecretResponse>(IpcRequest::GetSecret {
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

    async fn run_cleanup(&self) -> Result<CleanupReportResponse> {
        self.request_typed(IpcRequest::RunCleanup).await
    }

    async fn list_sessions(&self) -> Result<Vec<ChatSessionSummary>> {
        let mut client = self.client.lock().await;
        client.list_sessions().await
    }

    async fn get_session(&self, id: &str) -> Result<ChatSession> {
        let mut client = self.client.lock().await;
        client.get_session(id.to_string()).await
    }

    async fn search_sessions(
        &self,
        query: &str,
        agent_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<ChatSessionSummary>> {
        let mut client = self.client.lock().await;
        client
            .search_sessions(
                query.to_string(),
                agent_id.map(ToOwned::to_owned),
                Some(limit.max(1)),
            )
            .await
    }

    async fn create_session(
        &self,
        agent_id: Option<String>,
        model: Option<String>,
        name: Option<String>,
        skill_id: Option<String>,
    ) -> Result<ChatSession> {
        let mut client = self.client.lock().await;
        client.create_session(agent_id, model, name, skill_id).await
    }

    async fn delete_session(&self, id: &str) -> Result<bool> {
        let mut client = self.client.lock().await;
        client.delete_session(id.to_string()).await
    }
}
