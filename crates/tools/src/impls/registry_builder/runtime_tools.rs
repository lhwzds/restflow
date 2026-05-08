use std::sync::Arc;

use crate::impls::list_subagents::ListSubagentsTool;
use crate::impls::load_skill::LoadSkillTool;
use crate::impls::spawn::SpawnTool;
use crate::impls::spawn_subagent::SpawnSubagentTool;
use crate::impls::wait_subagents::WaitSubagentsTool;
use crate::security::SecurityGate;
use types::skill::SkillProvider;
use types::{SubagentManager, SubagentSpawner};

use super::ToolRegistryBuilder;

impl ToolRegistryBuilder {
    pub fn with_spawn(mut self, spawner: Arc<dyn SubagentSpawner>) -> Self {
        self.registry.register(SpawnTool::new(spawner));
        self
    }

    pub fn with_spawn_subagent(mut self, manager: Arc<dyn SubagentManager>) -> Self {
        self.registry.register(SpawnSubagentTool::new(manager));
        self
    }

    pub fn with_wait_subagents(mut self, manager: Arc<dyn SubagentManager>) -> Self {
        self.registry.register(WaitSubagentsTool::new(manager));
        self
    }

    pub fn with_list_subagents(mut self, manager: Arc<dyn SubagentManager>) -> Self {
        self.registry.register(ListSubagentsTool::new(manager));
        self
    }

    pub fn with_load_skill(mut self, provider: Arc<dyn SkillProvider>) -> Self {
        self.registry.register(LoadSkillTool::new(provider));
        self
    }

    pub fn with_load_skill_with_security(
        mut self,
        provider: Arc<dyn SkillProvider>,
        security_gate: Arc<dyn SecurityGate>,
        agent_id: impl Into<String>,
        task_id: impl Into<String>,
    ) -> Self {
        self.registry
            .register(LoadSkillTool::new(provider).with_security(security_gate, agent_id, task_id));
        self
    }
}
