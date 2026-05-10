use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use super::SUBAGENT_TOOL_NAMES;
use crate::AgentStorage;
use crate::services::adapters::AgentStoreAdapter;
use crate::services::operation_assessment::OperationAssessorAdapter;
use crate::storage::SecretStorage;
use crate::storage::Storage;
use crate::tools::{
    BashConfig, FileConfig, ListSubagentsTool, SpawnSubagentBatchTool, SpawnSubagentTool,
    ToolRegistryBuilder, WaitSubagentsTool,
};
use types::AgentOperationAssessor;
use types::SubagentManager;
use types::store::AgentStore;
use types::tool::SecurityGate;
use types::toolset::ToolRegistry;

pub(crate) struct AgentCrudComponents {
    pub known_tools: Arc<RwLock<HashSet<String>>>,
    pub store: Arc<dyn AgentStore>,
}

pub(crate) fn register_bash_execution_tool(
    mut builder: ToolRegistryBuilder,
    config: BashConfig,
    security_gate: Option<Arc<dyn SecurityGate>>,
    agent_id: &str,
    task_id: &str,
) -> ToolRegistryBuilder {
    if let Some(gate) = security_gate {
        builder.registry.register(
            config
                .into_bash_tool()
                .with_security(gate, agent_id, task_id),
        );
    } else {
        builder = builder.with_bash(config);
    }
    builder
}

pub(crate) fn register_file_execution_tool(
    mut builder: ToolRegistryBuilder,
    config: FileConfig,
    security_gate: Option<Arc<dyn SecurityGate>>,
    agent_id: &str,
    task_id: &str,
) -> ToolRegistryBuilder {
    if let Some(gate) = security_gate.clone() {
        let tool = config
            .into_file_tool_with_tracker(builder.tracker())
            .with_security(gate, agent_id, task_id);
        builder.registry.register(tool);
    } else {
        builder = builder.with_file(config);
    }

    builder
}

pub(crate) fn populate_known_tools_from_registry(
    known_tools: &Arc<RwLock<HashSet<String>>>,
    registry: &ToolRegistry,
) {
    if let Ok(mut known) = known_tools.write() {
        *known = registry
            .list()
            .into_iter()
            .map(|name| name.to_string())
            .collect::<HashSet<_>>();
        for name in [
            "bash",
            "file",
            "edit",
            "multiedit",
            "patch",
            "glob",
            "grep",
            "load_skill",
            "run_skill",
            "manage_agents",
        ] {
            known.insert(name.to_string());
        }
        for name in SUBAGENT_TOOL_NAMES {
            known.insert((*name).to_string());
        }
    }
}

pub(crate) fn build_runtime_assessor(storage: &Storage) -> Arc<dyn AgentOperationAssessor> {
    Arc::new(OperationAssessorAdapter::from_storage(storage))
}

pub(crate) fn build_agent_crud_components(
    agent_storage: AgentStorage,
    secret_storage: SecretStorage,
) -> AgentCrudComponents {
    let known_tools = Arc::new(RwLock::new(HashSet::new()));
    let store: Arc<dyn AgentStore> = Arc::new(AgentStoreAdapter::new(
        agent_storage,
        secret_storage,
        known_tools.clone(),
    ));
    AgentCrudComponents { known_tools, store }
}

pub(crate) fn register_management_tools(
    mut builder: ToolRegistryBuilder,
    agent_store: Option<Arc<dyn AgentStore>>,
    assessor: Option<Arc<dyn AgentOperationAssessor>>,
) -> ToolRegistryBuilder {
    if let Some(agent_store) = agent_store {
        builder = if let Some(assessor) = assessor.clone() {
            builder.with_agent_crud_and_assessor(agent_store, assessor)
        } else {
            builder.with_agent_crud(agent_store)
        };
    }

    builder
}

pub(crate) fn register_subagent_management_tools(
    registry: &mut ToolRegistry,
    manager: Arc<dyn SubagentManager>,
    assessor: Option<Arc<dyn AgentOperationAssessor>>,
) {
    let mut spawn_tool = SpawnSubagentTool::new(manager.clone());
    let mut batch_tool = SpawnSubagentBatchTool::new(manager.clone());
    if let Some(assessor) = assessor {
        spawn_tool = spawn_tool.with_assessor(assessor.clone());
        batch_tool = batch_tool.with_assessor(assessor);
    }

    registry.register(spawn_tool);
    registry.register(batch_tool);
    registry.register(WaitSubagentsTool::new(manager.clone()));
    registry.register(ListSubagentsTool::new(manager));
}
