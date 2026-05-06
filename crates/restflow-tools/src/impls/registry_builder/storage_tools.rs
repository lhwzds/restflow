use std::sync::Arc;

use crate::impls::TaskTool;
use crate::impls::agent_crud::AgentCrudTool;
use crate::impls::auth_profile::AuthProfileTool;
use crate::impls::config::ConfigTool;
use crate::impls::diagnostics::DiagnosticsTool;
use crate::impls::manage_ops::ManageOpsTool;
use crate::impls::marketplace::MarketplaceTool;
use crate::impls::secrets::SecretsTool;
use crate::impls::security_query::SecurityQueryTool;
use crate::impls::session::SessionTool;
use crate::impls::skill::SkillTool;
use crate::impls::terminal::TerminalTool;
use crate::security::SecurityGate;
use restflow_traits::AgentOperationAssessor;
use restflow_traits::skill::SkillProvider;
use restflow_traits::store::{
    AgentStore, AuthProfileStore, ConfigStore, DiagnosticsProvider, MarketplaceStore, OpsProvider,
    SecretStore, SecurityQueryProvider, SessionStore, TaskStore, TerminalStore,
};

use super::ToolRegistryBuilder;
use super::configs::SecretsConfig;

fn build_task_tool(
    store: Arc<dyn TaskStore>,
    assessor: Option<Arc<dyn AgentOperationAssessor>>,
) -> TaskTool {
    let mut tool = TaskTool::from_task_store(store);
    if let Some(assessor) = assessor {
        tool = tool.with_assessor(assessor);
    }
    tool.with_write(true)
}

impl ToolRegistryBuilder {
    pub fn with_diagnostics(mut self, provider: Arc<dyn DiagnosticsProvider>) -> Self {
        self.registry.register(DiagnosticsTool::new(provider));
        self
    }

    pub fn with_diagnostics_with_timeout(
        mut self,
        provider: Arc<dyn DiagnosticsProvider>,
        default_timeout_ms: u64,
    ) -> Self {
        self.registry
            .register(DiagnosticsTool::with_timeout(provider, default_timeout_ms));
        self
    }

    pub fn with_skill_tool(mut self, provider: Arc<dyn SkillProvider>) -> Self {
        self.registry.register(SkillTool::new(provider));
        self
    }

    pub fn with_skill_tool_with_security(
        mut self,
        provider: Arc<dyn SkillProvider>,
        security_gate: Arc<dyn SecurityGate>,
        agent_id: impl Into<String>,
        task_id: impl Into<String>,
    ) -> Self {
        self.registry
            .register(SkillTool::new(provider).with_security(security_gate, agent_id, task_id));
        self
    }

    pub fn with_session(mut self, store: Arc<dyn SessionStore>) -> Self {
        self.registry
            .register(SessionTool::new(store).with_write(true));
        self
    }

    pub fn with_ops(mut self, provider: Arc<dyn OpsProvider>) -> Self {
        self.registry.register(ManageOpsTool::new(provider));
        self
    }

    pub fn with_auth_profile(mut self, store: Arc<dyn AuthProfileStore>) -> Self {
        self.registry
            .register(AuthProfileTool::new(store).with_write(true));
        self
    }

    pub fn with_secrets(mut self, store: Arc<dyn SecretStore>) -> Self {
        self = self.with_secrets_config(store, SecretsConfig::default());
        self
    }

    pub fn with_secrets_config(
        mut self,
        store: Arc<dyn SecretStore>,
        config: SecretsConfig,
    ) -> Self {
        self.registry.register(
            SecretsTool::new(store)
                .with_write(config.allow_write)
                .with_get_policy(config.get_policy),
        );
        self
    }

    pub fn with_config(mut self, store: Arc<dyn ConfigStore>) -> Self {
        self.registry.register(ConfigTool::new(store));
        self
    }

    pub fn with_agent_crud(mut self, store: Arc<dyn AgentStore>) -> Self {
        self.registry
            .register(AgentCrudTool::new(store).with_write(true));
        self
    }

    pub fn with_agent_crud_and_assessor(
        mut self,
        store: Arc<dyn AgentStore>,
        assessor: Arc<dyn AgentOperationAssessor>,
    ) -> Self {
        self.registry.register(
            AgentCrudTool::new(store)
                .with_assessor(assessor)
                .with_write(true),
        );
        self
    }

    pub fn with_task(mut self, store: Arc<dyn TaskStore>) -> Self {
        self.registry.register(build_task_tool(store, None));
        self
    }

    pub fn with_task_and_assessor(
        mut self,
        store: Arc<dyn TaskStore>,
        assessor: Arc<dyn AgentOperationAssessor>,
    ) -> Self {
        self.registry
            .register(build_task_tool(store, Some(assessor)));
        self
    }

    pub fn with_marketplace(mut self, store: Arc<dyn MarketplaceStore>) -> Self {
        self.registry.register(MarketplaceTool::new(store));
        self
    }

    pub fn with_terminal(mut self, store: Arc<dyn TerminalStore>) -> Self {
        self.registry.register(TerminalTool::new(store));
        self
    }

    pub fn with_security_query(mut self, provider: Arc<dyn SecurityQueryProvider>) -> Self {
        self.registry.register(SecurityQueryTool::new(provider));
        self
    }
}
