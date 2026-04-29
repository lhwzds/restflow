use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use crate::services::adapters::{AgentStoreAdapter, TaskStoreAdapter};
use crate::services::operation_assessment::OperationAssessorAdapter;
use crate::services::session::SessionService;
use crate::storage::Storage;
use crate::storage::{AgentStorage, RunArtifactStorage, SecretStorage, SkillStorage, TaskStorage};
use restflow_tools::{
    BashConfig, BinarySkillBuildTool, BinarySkillNewTool, BinarySkillReadTool, BinarySkillRunTool,
    BinarySkillUpdateTool, EmailTool, FileConfig, HttpTool, ListSubagentsTool, RunPythonTool,
    SpawnSubagentBatchTool, SpawnSubagentTool, ToolRegistryBuilder, WaitSubagentsTool,
    discover_installed_binary_skill_tools,
};
use restflow_traits::AgentOperationAssessor;
use restflow_traits::SubagentManager;
use restflow_traits::registry::ToolRegistry;
use restflow_traits::security::SecurityGate;
use restflow_traits::store::{AgentStore, TaskStore};

pub(crate) const KNOWN_TOOL_ALIASES: [(&str, &str); 5] = [
    ("http", "http_request"),
    ("email", "send_email"),
    ("telegram", "telegram_send"),
    ("discord", "discord_send"),
    ("slack", "slack_send"),
];

pub(crate) struct AgentCrudComponents {
    pub known_tools: Arc<RwLock<HashSet<String>>>,
    pub store: Arc<dyn AgentStore>,
}

pub(crate) struct TaskStoreComponents {
    pub store: Arc<dyn TaskStore>,
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

pub(crate) fn register_http_execution_tool(
    mut builder: ToolRegistryBuilder,
    security_gate: Option<Arc<dyn SecurityGate>>,
    agent_id: &str,
    task_id: &str,
) -> anyhow::Result<ToolRegistryBuilder> {
    if let Some(gate) = security_gate {
        builder
            .registry
            .register(HttpTool::new()?.with_security(gate, agent_id, task_id));
    } else {
        builder = builder.with_http()?;
    }
    Ok(builder)
}

pub(crate) fn register_send_email_execution_tool(
    mut builder: ToolRegistryBuilder,
    security_gate: Option<Arc<dyn SecurityGate>>,
    agent_id: &str,
    task_id: &str,
) -> ToolRegistryBuilder {
    if let Some(gate) = security_gate {
        builder
            .registry
            .register(EmailTool::new().with_security(gate, agent_id, task_id));
    } else {
        builder = builder.with_email();
    }
    builder
}

pub(crate) fn register_python_execution_tools(
    mut builder: ToolRegistryBuilder,
    security_gate: Option<Arc<dyn SecurityGate>>,
    agent_id: &str,
    task_id: &str,
) -> ToolRegistryBuilder {
    if let Some(gate) = security_gate {
        builder
            .registry
            .register(RunPythonTool::new().with_security(gate, agent_id, task_id));
    } else {
        builder = builder.with_python();
    }
    builder
}

pub(crate) fn register_binary_skill_tools(
    mut builder: ToolRegistryBuilder,
    security_gate: Option<Arc<dyn SecurityGate>>,
    agent_id: &str,
    task_id: &str,
) -> ToolRegistryBuilder {
    if let Some(gate) = security_gate.clone() {
        builder
            .registry
            .register(BinarySkillNewTool::new().with_security(gate.clone(), agent_id, task_id));
        builder
            .registry
            .register(BinarySkillBuildTool::new().with_security(gate.clone(), agent_id, task_id));
        builder
            .registry
            .register(BinarySkillReadTool::new().with_security(gate.clone(), agent_id, task_id));
        builder
            .registry
            .register(BinarySkillRunTool::new().with_security(gate.clone(), agent_id, task_id));
        builder
            .registry
            .register(BinarySkillUpdateTool::new().with_security(gate, agent_id, task_id));
    } else {
        builder.registry.register(BinarySkillNewTool::new());
        builder.registry.register(BinarySkillBuildTool::new());
        builder.registry.register(BinarySkillReadTool::new());
        builder.registry.register(BinarySkillRunTool::new());
        builder.registry.register(BinarySkillUpdateTool::new());
    }
    if let Ok(tools) = discover_installed_binary_skill_tools() {
        for tool in tools {
            let tool = if let Some(gate) = security_gate.clone() {
                tool.with_security(gate, agent_id, task_id)
            } else {
                tool
            };
            builder.registry.register(tool);
        }
    }
    builder
}

pub(crate) fn populate_known_tools_from_registry(
    known_tools: &Arc<RwLock<HashSet<String>>>,
    registry: &ToolRegistry,
    aliases: Option<&[(&str, &str)]>,
) {
    if let Ok(mut known) = known_tools.write() {
        *known = registry
            .list()
            .into_iter()
            .map(|name| name.to_string())
            .collect::<HashSet<_>>();

        if let Some(alias_mappings) = aliases {
            for (alias_name, target_name) in alias_mappings {
                if known.contains(*target_name) {
                    known.insert((*alias_name).to_string());
                }
            }
        }
    }
}

pub(crate) fn build_runtime_assessor(storage: &Storage) -> Arc<dyn AgentOperationAssessor> {
    Arc::new(OperationAssessorAdapter::from_storage(storage))
}

pub(crate) fn build_agent_crud_components(
    agent_storage: AgentStorage,
    skill_storage: SkillStorage,
    secret_storage: SecretStorage,
    task_storage: TaskStorage,
) -> AgentCrudComponents {
    let known_tools = Arc::new(RwLock::new(HashSet::new()));
    let store: Arc<dyn AgentStore> = Arc::new(AgentStoreAdapter::new(
        agent_storage,
        skill_storage,
        secret_storage,
        task_storage,
        known_tools.clone(),
    ));
    AgentCrudComponents { known_tools, store }
}

pub(crate) fn build_task_store_components(
    task_storage: TaskStorage,
    agent_storage: AgentStorage,
    run_artifact_storage: RunArtifactStorage,
    session_service: SessionService,
    assessor: Option<Arc<dyn AgentOperationAssessor>>,
) -> TaskStoreComponents {
    let mut store = TaskStoreAdapter::new(
        task_storage,
        agent_storage,
        run_artifact_storage,
        session_service,
    );
    if let Some(assessor) = assessor {
        store = store.with_assessor(assessor);
    }

    TaskStoreComponents {
        store: Arc::new(store),
    }
}

pub(crate) fn register_management_tools(
    mut builder: ToolRegistryBuilder,
    agent_store: Option<Arc<dyn AgentStore>>,
    task_store: Option<Arc<dyn TaskStore>>,
    assessor: Option<Arc<dyn AgentOperationAssessor>>,
) -> ToolRegistryBuilder {
    if let Some(agent_store) = agent_store {
        builder = if let Some(assessor) = assessor.clone() {
            builder.with_agent_crud_and_assessor(agent_store, assessor)
        } else {
            builder.with_agent_crud(agent_store)
        };
    }

    if let Some(task_store) = task_store {
        builder = if let Some(assessor) = assessor.clone() {
            builder.with_task_and_assessor(task_store.clone(), assessor)
        } else {
            builder.with_task(task_store.clone())
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

pub(crate) fn build_task_store_runtime_components(
    storage: &Storage,
    assessor: Option<Arc<dyn AgentOperationAssessor>>,
) -> TaskStoreComponents {
    build_task_store_components(
        storage.tasks.clone(),
        storage.agents.clone(),
        storage.run_artifacts.clone(),
        SessionService::from_storage(storage),
        assessor,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_store_shell_exports_canonical_name() {
        let _canonical_components: Option<TaskStoreComponents> = None;

        let _canonical_builder = build_task_store_components;
        let _canonical_runtime_builder = build_task_store_runtime_components;
    }
}
