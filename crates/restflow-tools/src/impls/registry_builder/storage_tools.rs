use std::sync::Arc;

use crate::impls::TaskTool;
use crate::impls::agent_crud::AgentCrudTool;
use crate::impls::auth_profile::AuthProfileTool;
use crate::impls::background_agent::BackgroundAgentTool;
use crate::impls::config::ConfigTool;
use crate::impls::diagnostics::DiagnosticsTool;
use crate::impls::kv_store::KvStoreTool;
use crate::impls::manage_ops::ManageOpsTool;
use crate::impls::marketplace::MarketplaceTool;
use crate::impls::memory_mgmt::MemoryManagementTool;
use crate::impls::memory_store::{
    DeleteMemoryTool, ListMemoryTool, ReadMemoryTool, SaveMemoryTool,
};
use crate::impls::save_deliverable::SaveDeliverableTool;
use crate::impls::secrets::SecretsTool;
use crate::impls::security_query::SecurityQueryTool;
use crate::impls::session::SessionTool;
use crate::impls::skill::SkillTool;
use crate::impls::terminal::TerminalTool;
use crate::impls::trigger::TriggerTool;
use crate::impls::unified_memory_search::UnifiedMemorySearchTool;
use crate::impls::work_item::WorkItemTool;
use crate::security::SecurityGate;
use restflow_traits::AgentOperationAssessor;
use restflow_traits::skill::SkillProvider;
use restflow_traits::store::{
    AgentStore, AuthProfileStore, ConfigStore, DeliverableStore, DiagnosticsProvider, KvStore,
    MarketplaceStore, MemoryManager, MemoryStore, OpsProvider, SecretStore, SecurityQueryProvider,
    SessionStore, TaskStore, TerminalStore, TriggerStore, UnifiedMemorySearch, WorkItemProvider,
};

use super::ToolRegistryBuilder;
use super::configs::SecretsConfig;

fn build_task_tool(
    store: Arc<dyn TaskStore>,
    kv_store: Option<Arc<dyn KvStore>>,
    assessor: Option<Arc<dyn AgentOperationAssessor>>,
) -> TaskTool {
    let mut tool = TaskTool::from_task_store(store);
    if let Some(kv_store) = kv_store {
        tool = tool.with_kv_store(kv_store);
    }
    if let Some(assessor) = assessor {
        tool = tool.with_assessor(assessor);
    }
    tool.with_write(true)
}

fn build_legacy_task_alias_tool(
    store: Arc<dyn TaskStore>,
    kv_store: Option<Arc<dyn KvStore>>,
    assessor: Option<Arc<dyn AgentOperationAssessor>>,
) -> BackgroundAgentTool {
    BackgroundAgentTool::from_task_tool(build_task_tool(store, kv_store, assessor))
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

    pub fn with_memory_management(mut self, manager: Arc<dyn MemoryManager>) -> Self {
        self.registry
            .register(MemoryManagementTool::new(manager).with_write(true));
        self
    }

    pub fn with_memory_store(mut self, store: Arc<dyn MemoryStore>) -> Self {
        self.registry.register(SaveMemoryTool::new(store.clone()));
        self.registry.register(ReadMemoryTool::new(store.clone()));
        self.registry.register(ListMemoryTool::new(store.clone()));
        self.registry.register(DeleteMemoryTool::new(store));
        self
    }

    pub fn with_deliverable(mut self, store: Arc<dyn DeliverableStore>) -> Self {
        self.registry.register(SaveDeliverableTool::new(store));
        self
    }

    pub fn with_unified_search(mut self, search: Arc<dyn UnifiedMemorySearch>) -> Self {
        self.registry.register(UnifiedMemorySearchTool::new(search));
        self
    }

    pub fn with_ops(mut self, provider: Arc<dyn OpsProvider>) -> Self {
        self.registry.register(ManageOpsTool::new(provider));
        self
    }

    pub fn with_kv_store(mut self, store: Arc<dyn KvStore>) -> Self {
        self.registry.register(KvStoreTool::new(store, None));
        self
    }

    pub fn with_work_items(mut self, provider: Arc<dyn WorkItemProvider>) -> Self {
        self.registry
            .register(WorkItemTool::new(provider).with_write(true));
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
        self.registry.register(build_task_tool(store, None, None));
        self
    }

    pub fn with_legacy_task_alias(mut self, store: Arc<dyn TaskStore>) -> Self {
        self.registry
            .register(build_legacy_task_alias_tool(store, None, None));
        self
    }

    pub fn with_task_and_kv(
        mut self,
        store: Arc<dyn TaskStore>,
        kv_store: Arc<dyn KvStore>,
    ) -> Self {
        self.registry
            .register(build_task_tool(store, Some(kv_store), None));
        self
    }

    pub fn with_legacy_task_alias_and_kv(
        mut self,
        store: Arc<dyn TaskStore>,
        kv_store: Arc<dyn KvStore>,
    ) -> Self {
        self.registry
            .register(build_legacy_task_alias_tool(store, Some(kv_store), None));
        self
    }

    pub fn with_task_and_kv_and_assessor(
        mut self,
        store: Arc<dyn TaskStore>,
        kv_store: Arc<dyn KvStore>,
        assessor: Arc<dyn AgentOperationAssessor>,
    ) -> Self {
        self.registry
            .register(build_task_tool(store, Some(kv_store), Some(assessor)));
        self
    }

    pub fn with_legacy_task_alias_and_kv_and_assessor(
        mut self,
        store: Arc<dyn TaskStore>,
        kv_store: Arc<dyn KvStore>,
        assessor: Arc<dyn AgentOperationAssessor>,
    ) -> Self {
        self.registry.register(build_legacy_task_alias_tool(
            store,
            Some(kv_store),
            Some(assessor),
        ));
        self
    }

    pub fn with_marketplace(mut self, store: Arc<dyn MarketplaceStore>) -> Self {
        self.registry.register(MarketplaceTool::new(store));
        self
    }

    pub fn with_trigger(mut self, store: Arc<dyn TriggerStore>) -> Self {
        self.registry.register(TriggerTool::new(store));
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
