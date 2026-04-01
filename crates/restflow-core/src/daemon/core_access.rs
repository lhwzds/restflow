use super::ipc_client::IpcClient;
use super::ipc_protocol::IpcRequest;
use super::launcher::ensure_daemon_running;
use super::request_mapper::to_contract;
use crate::AppCore;
use crate::boundary::background_agent::core_spec_to_contract;
use crate::models::{
    AgentNode, BackgroundAgent, BackgroundAgentSpec, BackgroundAgentStatus, Skill,
};
use crate::paths;
use crate::services::{
    agent as agent_service, config as config_service, secrets as secrets_service,
    skills as skills_service,
};
use crate::storage::SystemConfig;
use anyhow::Result;
use restflow_contracts::OkResponse;
use std::sync::Arc;

pub enum CoreAccess {
    Local(Arc<AppCore>),
    Remote(IpcClient),
}

impl CoreAccess {
    pub async fn connect() -> Result<Self> {
        let socket_path = paths::socket_path()?;
        ensure_daemon_running().await?;
        let client = IpcClient::connect(&socket_path).await?;
        Ok(CoreAccess::Remote(client))
    }

    pub async fn connect_direct() -> Result<Self> {
        let db_path = paths::ensure_database_path_string()?;
        let core = AppCore::new(&db_path).await?;
        Ok(CoreAccess::Local(Arc::new(core)))
    }

    pub async fn list_agents(&mut self) -> Result<Vec<crate::storage::agent::StoredAgent>> {
        match self {
            CoreAccess::Local(core) => agent_service::list_agents(core).await,
            CoreAccess::Remote(client) => client.request_typed(IpcRequest::ListAgents).await,
        }
    }

    pub async fn get_agent(&mut self, id: &str) -> Result<crate::storage::agent::StoredAgent> {
        match self {
            CoreAccess::Local(core) => agent_service::get_agent(core, id).await,
            CoreAccess::Remote(client) => {
                client
                    .request_typed(IpcRequest::GetAgent { id: id.to_string() })
                    .await
            }
        }
    }

    pub async fn create_agent(
        &mut self,
        name: String,
        agent: AgentNode,
    ) -> Result<crate::storage::agent::StoredAgent> {
        match self {
            CoreAccess::Local(core) => agent_service::create_agent(core, name, agent).await,
            CoreAccess::Remote(client) => {
                let agent = restflow_contracts::request::AgentNode::from(agent);
                client
                    .request_typed(IpcRequest::CreateAgent { name, agent })
                    .await
            }
        }
    }

    pub async fn update_agent(
        &mut self,
        id: &str,
        name: Option<String>,
        agent: Option<AgentNode>,
    ) -> Result<crate::storage::agent::StoredAgent> {
        match self {
            CoreAccess::Local(core) => agent_service::update_agent(core, id, name, agent).await,
            CoreAccess::Remote(client) => {
                let agent = agent.map(restflow_contracts::request::AgentNode::from);
                client
                    .request_typed(IpcRequest::UpdateAgent {
                        id: id.to_string(),
                        name,
                        agent,
                    })
                    .await
            }
        }
    }

    pub async fn delete_agent(&mut self, id: &str) -> Result<()> {
        match self {
            CoreAccess::Local(core) => agent_service::delete_agent(core, id).await,
            CoreAccess::Remote(client) => {
                let _: OkResponse = client
                    .request_typed(IpcRequest::DeleteAgent { id: id.to_string() })
                    .await?;
                Ok(())
            }
        }
    }

    pub async fn list_skills(&mut self) -> Result<Vec<Skill>> {
        match self {
            CoreAccess::Local(core) => skills_service::list_skills(core).await,
            CoreAccess::Remote(client) => client.request_typed(IpcRequest::ListSkills).await,
        }
    }

    pub async fn get_skill(&mut self, id: &str) -> Result<Option<Skill>> {
        match self {
            CoreAccess::Local(core) => skills_service::get_skill(core, id).await,
            CoreAccess::Remote(client) => {
                client
                    .request_optional(IpcRequest::GetSkill { id: id.to_string() })
                    .await
            }
        }
    }

    pub async fn create_skill(&mut self, skill: Skill) -> Result<()> {
        match self {
            CoreAccess::Local(core) => skills_service::create_skill(core, skill).await,
            CoreAccess::Remote(client) => {
                let skill = to_contract(skill)?;
                let _: OkResponse = client
                    .request_typed(IpcRequest::CreateSkill { skill })
                    .await?;
                Ok(())
            }
        }
    }

    pub async fn update_skill(&mut self, id: &str, skill: Skill) -> Result<()> {
        match self {
            CoreAccess::Local(core) => skills_service::update_skill(core, id, &skill).await,
            CoreAccess::Remote(client) => {
                let skill = to_contract(skill)?;
                let _: OkResponse = client
                    .request_typed(IpcRequest::UpdateSkill {
                        id: id.to_string(),
                        skill,
                    })
                    .await?;
                Ok(())
            }
        }
    }

    pub async fn delete_skill(&mut self, id: &str) -> Result<()> {
        match self {
            CoreAccess::Local(core) => skills_service::delete_skill(core, id).await,
            CoreAccess::Remote(client) => {
                let _: OkResponse = client
                    .request_typed(IpcRequest::DeleteSkill { id: id.to_string() })
                    .await?;
                Ok(())
            }
        }
    }

    pub async fn list_background_agents(
        &mut self,
        status: Option<BackgroundAgentStatus>,
    ) -> Result<Vec<BackgroundAgent>> {
        match self {
            CoreAccess::Local(core) => match status {
                Some(status) => core.storage.background_agents.list_tasks_by_status(status),
                None => core.storage.background_agents.list_tasks(),
            },
            CoreAccess::Remote(client) => {
                client
                    .request_typed(IpcRequest::ListBackgroundAgents {
                        status: status.map(|value| value.as_str().to_string()),
                    })
                    .await
            }
        }
    }

    pub async fn get_background_agent(&mut self, id: &str) -> Result<Option<BackgroundAgent>> {
        match self {
            CoreAccess::Local(core) => core.storage.background_agents.get_task(id),
            CoreAccess::Remote(client) => {
                client
                    .request_optional(IpcRequest::GetBackgroundAgent { id: id.to_string() })
                    .await
            }
        }
    }

    pub async fn create_background_agent(
        &mut self,
        spec: BackgroundAgentSpec,
    ) -> Result<BackgroundAgent> {
        match self {
            CoreAccess::Local(core) => core.storage.background_agents.create_background_agent(spec),
            CoreAccess::Remote(client) => {
                let spec = core_spec_to_contract(spec)?;
                client
                    .request_typed(IpcRequest::CreateBackgroundAgent { spec })
                    .await
            }
        }
    }

    pub async fn list_secrets(&mut self) -> Result<Vec<crate::models::Secret>> {
        match self {
            CoreAccess::Local(core) => secrets_service::list_secrets(core).await,
            CoreAccess::Remote(client) => client.request_typed(IpcRequest::ListSecrets).await,
        }
    }

    pub async fn get_secret(&mut self, key: &str) -> Result<Option<String>> {
        match self {
            CoreAccess::Local(core) => secrets_service::get_secret(core, key).await,
            CoreAccess::Remote(client) => {
                let response: restflow_contracts::SecretResponse = client
                    .request_typed(IpcRequest::GetSecret {
                        key: key.to_string(),
                    })
                    .await?;
                Ok(response.value)
            }
        }
    }

    pub async fn set_secret(
        &mut self,
        key: &str,
        value: &str,
        description: Option<String>,
    ) -> Result<()> {
        match self {
            CoreAccess::Local(core) => {
                secrets_service::set_secret(core, key, value, description).await
            }
            CoreAccess::Remote(client) => {
                let _: OkResponse = client
                    .request_typed(IpcRequest::SetSecret {
                        key: key.to_string(),
                        value: value.to_string(),
                        description,
                    })
                    .await?;
                Ok(())
            }
        }
    }

    pub async fn delete_secret(&mut self, key: &str) -> Result<()> {
        match self {
            CoreAccess::Local(core) => secrets_service::delete_secret(core, key).await,
            CoreAccess::Remote(client) => {
                let _: OkResponse = client
                    .request_typed(IpcRequest::DeleteSecret {
                        key: key.to_string(),
                    })
                    .await?;
                Ok(())
            }
        }
    }

    pub async fn get_config(&mut self) -> Result<SystemConfig> {
        match self {
            CoreAccess::Local(core) => config_service::get_config(core).await,
            CoreAccess::Remote(client) => client.request_typed(IpcRequest::GetConfig).await,
        }
    }

    pub async fn get_global_config(&mut self) -> Result<SystemConfig> {
        match self {
            CoreAccess::Local(core) => config_service::get_global_config(core).await,
            CoreAccess::Remote(client) => client.request_typed(IpcRequest::GetGlobalConfig).await,
        }
    }

    pub async fn set_config(&mut self, config: SystemConfig) -> Result<()> {
        match self {
            CoreAccess::Local(core) => config_service::update_config(core, config).await,
            CoreAccess::Remote(client) => {
                let config = to_contract(config)?;
                let _: OkResponse = client
                    .request_typed(IpcRequest::SetConfig { config })
                    .await?;
                Ok(())
            }
        }
    }
}
