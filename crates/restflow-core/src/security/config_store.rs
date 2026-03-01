use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::RwLock;

use crate::models::security::AgentSecurityConfig;

#[derive(Debug)]
pub struct SecurityConfigStore {
    default_config: RwLock<AgentSecurityConfig>,
    agents: DashMap<String, AgentSecurityConfig>,
}

impl SecurityConfigStore {
    pub fn new(default_config: AgentSecurityConfig) -> Self {
        Self {
            default_config: RwLock::new(default_config),
            agents: DashMap::new(),
        }
    }

    pub async fn set_default_config(&self, config: AgentSecurityConfig) {
        let mut current = self.default_config.write().await;
        *current = config;
    }

    pub async fn get_default_config(&self) -> AgentSecurityConfig {
        self.default_config.read().await.clone()
    }

    pub fn set_agent_config(&self, agent_id: &str, config: AgentSecurityConfig) {
        self.agents.insert(agent_id.to_string(), config);
    }

    pub fn get_agent_config(&self, agent_id: &str) -> Option<AgentSecurityConfig> {
        self.agents
            .get(agent_id)
            .map(|c| c.clone())
            .or_else(|| self.agents.get("*").map(|c| c.clone()))
    }

    pub fn remove_agent_config(&self, agent_id: &str) {
        self.agents.remove(agent_id);
    }

    pub fn shared(self) -> Arc<Self> {
        Arc::new(self)
    }
}
