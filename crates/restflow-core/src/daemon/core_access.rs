use super::ipc_client::IpcClient;
use super::ipc_protocol::{IpcRequest, IpcResponse};
use super::launcher::ensure_daemon_running;
use crate::AppCore;
use crate::models::{AgentNode, AgentTaskStatus, BackgroundAgentSpec, Skill};
use crate::paths;
use crate::services::{
    agent as agent_service, config as config_service, secrets as secrets_service,
    skills as skills_service,
};
use crate::storage::SystemConfig;
use anyhow::Result;
use serde::de::DeserializeOwned;
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
            CoreAccess::Remote(client) => {
                let response = client.request(IpcRequest::ListAgents).await?;
                decode_response(response)
            }
        }
    }

    pub async fn get_agent(&mut self, id: &str) -> Result<crate::storage::agent::StoredAgent> {
        match self {
            CoreAccess::Local(core) => agent_service::get_agent(core, id).await,
            CoreAccess::Remote(client) => {
                let response = client
                    .request(IpcRequest::GetAgent { id: id.to_string() })
                    .await?;
                decode_response(response)
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
                let response = client
                    .request(IpcRequest::CreateAgent { name, agent })
                    .await?;
                decode_response(response)
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
                let response = client
                    .request(IpcRequest::UpdateAgent {
                        id: id.to_string(),
                        name,
                        agent,
                    })
                    .await?;
                decode_response(response)
            }
        }
    }

    pub async fn delete_agent(&mut self, id: &str) -> Result<()> {
        match self {
            CoreAccess::Local(core) => agent_service::delete_agent(core, id).await,
            CoreAccess::Remote(client) => {
                let response = client
                    .request(IpcRequest::DeleteAgent { id: id.to_string() })
                    .await?;
                decode_response::<serde_json::Value>(response).map(|_| ())
            }
        }
    }

    pub async fn list_skills(&mut self) -> Result<Vec<Skill>> {
        match self {
            CoreAccess::Local(core) => skills_service::list_skills(core).await,
            CoreAccess::Remote(client) => {
                let response = client.request(IpcRequest::ListSkills).await?;
                decode_response(response)
            }
        }
    }

    pub async fn get_skill(&mut self, id: &str) -> Result<Option<Skill>> {
        match self {
            CoreAccess::Local(core) => skills_service::get_skill(core, id).await,
            CoreAccess::Remote(client) => {
                let response = client
                    .request(IpcRequest::GetSkill { id: id.to_string() })
                    .await?;
                match response {
                    IpcResponse::Success(value) => Ok(Some(serde_json::from_value(value)?)),
                    IpcResponse::Error { code: 404, .. } => Ok(None),
                    IpcResponse::Error { message, .. } => anyhow::bail!(message),
                    IpcResponse::Pong => anyhow::bail!("Unexpected response"),
                }
            }
        }
    }

    pub async fn create_skill(&mut self, skill: Skill) -> Result<()> {
        match self {
            CoreAccess::Local(core) => skills_service::create_skill(core, skill).await,
            CoreAccess::Remote(client) => {
                let response = client.request(IpcRequest::CreateSkill { skill }).await?;
                decode_response::<serde_json::Value>(response).map(|_| ())
            }
        }
    }

    pub async fn update_skill(&mut self, id: &str, skill: Skill) -> Result<()> {
        match self {
            CoreAccess::Local(core) => skills_service::update_skill(core, id, &skill).await,
            CoreAccess::Remote(client) => {
                let response = client
                    .request(IpcRequest::UpdateSkill {
                        id: id.to_string(),
                        skill,
                    })
                    .await?;
                decode_response::<serde_json::Value>(response).map(|_| ())
            }
        }
    }

    pub async fn delete_skill(&mut self, id: &str) -> Result<()> {
        match self {
            CoreAccess::Local(core) => skills_service::delete_skill(core, id).await,
            CoreAccess::Remote(client) => {
                let response = client
                    .request(IpcRequest::DeleteSkill { id: id.to_string() })
                    .await?;
                decode_response::<serde_json::Value>(response).map(|_| ())
            }
        }
    }

    pub async fn list_background_agents(
        &mut self,
        status: Option<AgentTaskStatus>,
    ) -> Result<Vec<crate::models::AgentTask>> {
        match self {
            CoreAccess::Local(core) => match status {
                Some(status) => core.storage.agent_tasks.list_tasks_by_status(status),
                None => core.storage.agent_tasks.list_tasks(),
            },
            CoreAccess::Remote(client) => {
                let response = client
                    .request(IpcRequest::ListBackgroundAgents {
                        status: status.map(|value| value.as_str().to_string()),
                    })
                    .await?;
                decode_response(response)
            }
        }
    }

    pub async fn get_background_agent(
        &mut self,
        id: &str,
    ) -> Result<Option<crate::models::AgentTask>> {
        match self {
            CoreAccess::Local(core) => core.storage.agent_tasks.get_task(id),
            CoreAccess::Remote(client) => {
                let response = client
                    .request(IpcRequest::GetBackgroundAgent { id: id.to_string() })
                    .await?;
                match response {
                    IpcResponse::Success(value) => Ok(Some(serde_json::from_value(value)?)),
                    IpcResponse::Error { code: 404, .. } => Ok(None),
                    IpcResponse::Error { message, .. } => anyhow::bail!(message),
                    IpcResponse::Pong => anyhow::bail!("Unexpected response"),
                }
            }
        }
    }

    pub async fn create_background_agent(
        &mut self,
        spec: BackgroundAgentSpec,
    ) -> Result<crate::models::AgentTask> {
        match self {
            CoreAccess::Local(core) => core.storage.agent_tasks.create_background_agent(spec),
            CoreAccess::Remote(client) => {
                let response = client
                    .request(IpcRequest::CreateBackgroundAgent { spec })
                    .await?;
                decode_response(response)
            }
        }
    }

    pub async fn list_secrets(&mut self) -> Result<Vec<crate::models::Secret>> {
        match self {
            CoreAccess::Local(core) => secrets_service::list_secrets(core).await,
            CoreAccess::Remote(client) => {
                let response = client.request(IpcRequest::ListSecrets).await?;
                decode_response(response)
            }
        }
    }

    pub async fn get_secret(&mut self, key: &str) -> Result<Option<String>> {
        match self {
            CoreAccess::Local(core) => secrets_service::get_secret(core, key).await,
            CoreAccess::Remote(client) => {
                let response = client
                    .request(IpcRequest::GetSecret {
                        key: key.to_string(),
                    })
                    .await?;
                match response {
                    IpcResponse::Success(value) => Ok(value
                        .get("value")
                        .and_then(|value| value.as_str())
                        .map(|value| value.to_string())),
                    IpcResponse::Error { code: 404, .. } => Ok(None),
                    IpcResponse::Error { message, .. } => anyhow::bail!(message),
                    IpcResponse::Pong => anyhow::bail!("Unexpected response"),
                }
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
                let response = client
                    .request(IpcRequest::SetSecret {
                        key: key.to_string(),
                        value: value.to_string(),
                        description,
                    })
                    .await?;
                decode_response::<serde_json::Value>(response).map(|_| ())
            }
        }
    }

    pub async fn delete_secret(&mut self, key: &str) -> Result<()> {
        match self {
            CoreAccess::Local(core) => secrets_service::delete_secret(core, key).await,
            CoreAccess::Remote(client) => {
                let response = client
                    .request(IpcRequest::DeleteSecret {
                        key: key.to_string(),
                    })
                    .await?;
                decode_response::<serde_json::Value>(response).map(|_| ())
            }
        }
    }

    pub async fn get_config(&mut self) -> Result<SystemConfig> {
        match self {
            CoreAccess::Local(core) => config_service::get_config(core).await,
            CoreAccess::Remote(client) => {
                let response = client.request(IpcRequest::GetConfig).await?;
                decode_response(response)
            }
        }
    }

    pub async fn set_config(&mut self, config: SystemConfig) -> Result<()> {
        match self {
            CoreAccess::Local(core) => config_service::update_config(core, config).await,
            CoreAccess::Remote(client) => {
                let response = client.request(IpcRequest::SetConfig { config }).await?;
                decode_response::<serde_json::Value>(response).map(|_| ())
            }
        }
    }
}

fn decode_response<T: DeserializeOwned>(response: IpcResponse) -> Result<T> {
    match response {
        IpcResponse::Success(value) => Ok(serde_json::from_value(value)?),
        IpcResponse::Error { message, .. } => anyhow::bail!(message),
        IpcResponse::Pong => anyhow::bail!("Unexpected response"),
    }
}
