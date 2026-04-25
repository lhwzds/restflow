use std::sync::Arc;

use crate::impls::list_subagents::ListSubagentsTool;
use crate::impls::spawn::SpawnTool;
use crate::impls::spawn_subagent::SpawnSubagentTool;
use crate::impls::task_list::TaskListTool;
use crate::impls::use_skill::UseSkillTool;
use crate::impls::wait_subagents::WaitSubagentsTool;
use crate::security::SecurityGate;
use restflow_traits::skill::SkillProvider;
use restflow_traits::store::{TeamTemplateStore, WorkItemProvider};
use restflow_traits::{SubagentManager, SubagentSpawner};

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

    pub fn with_spawn_subagent_with_store(
        mut self,
        manager: Arc<dyn SubagentManager>,
        team_template_store: Arc<dyn TeamTemplateStore>,
    ) -> Self {
        self.registry.register(
            SpawnSubagentTool::new(manager).with_team_template_store(team_template_store),
        );
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

    pub fn with_use_skill(mut self, provider: Arc<dyn SkillProvider>) -> Self {
        self.registry.register(UseSkillTool::new(provider));
        self
    }

    pub fn with_use_skill_with_security(
        mut self,
        provider: Arc<dyn SkillProvider>,
        security_gate: Arc<dyn SecurityGate>,
        agent_id: impl Into<String>,
        task_id: impl Into<String>,
    ) -> Self {
        self.registry
            .register(UseSkillTool::new(provider).with_security(security_gate, agent_id, task_id));
        self
    }

    pub fn with_task_list(mut self, provider: Arc<dyn WorkItemProvider>) -> Self {
        self.registry.register(TaskListTool::new(provider));
        self
    }
}
