use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use crate::services::adapters::{AgentStoreAdapter, BackgroundAgentStoreAdapter, KvStoreAdapter};
use crate::services::operation_assessment::OperationAssessorAdapter;
use crate::services::session::SessionService;
use crate::storage::{
    AgentStorage, BackgroundAgentStorage, DeliverableStorage, KvStoreStorage, SecretStorage,
    SkillStorage,
};
use crate::storage::Storage;
use restflow_tools::{
    BashConfig, EmailTool, FileConfig, HttpTool, ListSubagentsTool, PythonTool, RunPythonTool,
    SpawnSubagentTool, ToolRegistryBuilder, WaitSubagentsTool,
};
use restflow_traits::AgentOperationAssessor;
use restflow_traits::registry::ToolRegistry;
use restflow_traits::security::SecurityGate;
use restflow_traits::SubagentManager;
use restflow_traits::store::{AgentStore, BackgroundAgentStore, KvStore};

pub(crate) const KNOWN_TOOL_ALIASES: [(&str, &str); 7] = [
    ("http", "http_request"),
    ("email", "send_email"),
    ("telegram", "telegram_send"),
    ("discord", "discord_send"),
    ("slack", "slack_send"),
    ("use_skill", "skill"),
    ("python", "run_python"),
];

pub(crate) struct AgentCrudComponents {
    pub known_tools: Arc<RwLock<HashSet<String>>>,
    pub store: Arc<dyn AgentStore>,
}

pub(crate) struct BackgroundAgentComponents {
    pub store: Arc<dyn BackgroundAgentStore>,
    pub kv_store: Arc<dyn KvStore>,
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
    if let Some(gate) = security_gate {
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
            .register(RunPythonTool::new().with_security(gate.clone(), agent_id, task_id));
        builder
            .registry
            .register(PythonTool::new().with_security(gate, agent_id, task_id));
    } else {
        builder = builder.with_python();
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

pub(crate) fn build_kv_store(
    kv_store_storage: KvStoreStorage,
    accessor_id: Option<String>,
) -> Arc<dyn KvStore> {
    Arc::new(KvStoreAdapter::new(kv_store_storage, accessor_id))
}

pub(crate) fn build_agent_crud_components(
    agent_storage: AgentStorage,
    skill_storage: SkillStorage,
    secret_storage: SecretStorage,
    background_agent_storage: BackgroundAgentStorage,
) -> AgentCrudComponents {
    let known_tools = Arc::new(RwLock::new(HashSet::new()));
    let store: Arc<dyn AgentStore> = Arc::new(AgentStoreAdapter::new(
        agent_storage,
        skill_storage,
        secret_storage,
        background_agent_storage,
        known_tools.clone(),
    ));
    AgentCrudComponents { known_tools, store }
}

pub(crate) fn build_background_agent_components(
    background_agent_storage: BackgroundAgentStorage,
    agent_storage: AgentStorage,
    deliverable_storage: DeliverableStorage,
    session_service: SessionService,
    kv_store: Arc<dyn KvStore>,
    assessor: Option<Arc<dyn AgentOperationAssessor>>,
) -> BackgroundAgentComponents {
    let mut store = BackgroundAgentStoreAdapter::new(
        background_agent_storage,
        agent_storage,
        deliverable_storage,
        session_service,
    );
    if let Some(assessor) = assessor {
        store = store.with_assessor(assessor);
    }

    BackgroundAgentComponents {
        store: Arc::new(store),
        kv_store,
    }
}

pub(crate) fn register_management_tools(
    mut builder: ToolRegistryBuilder,
    agent_store: Option<Arc<dyn AgentStore>>,
    background_agent_store: Option<Arc<dyn BackgroundAgentStore>>,
    kv_store: Option<Arc<dyn KvStore>>,
    assessor: Option<Arc<dyn AgentOperationAssessor>>,
) -> ToolRegistryBuilder {
    if let Some(agent_store) = agent_store {
        builder = if let Some(assessor) = assessor.clone() {
            builder.with_agent_crud_and_assessor(agent_store, assessor)
        } else {
            builder.with_agent_crud(agent_store)
        };
    }

    if let (Some(background_agent_store), Some(kv_store)) = (background_agent_store, kv_store) {
        builder = if let Some(assessor) = assessor {
            builder.with_background_agent_and_kv_and_assessor(
                background_agent_store,
                kv_store,
                assessor,
            )
        } else {
            builder.with_background_agent_and_kv(background_agent_store, kv_store)
        };
    }

    builder
}

pub(crate) fn register_subagent_management_tools(
    registry: &mut ToolRegistry,
    manager: Arc<dyn SubagentManager>,
    kv_store: Option<Arc<dyn KvStore>>,
    assessor: Option<Arc<dyn AgentOperationAssessor>>,
) {
    let mut spawn_tool = SpawnSubagentTool::new(manager.clone());
    if let Some(kv_store) = kv_store {
        spawn_tool = spawn_tool.with_kv_store(kv_store);
    }
    if let Some(assessor) = assessor {
        spawn_tool = spawn_tool.with_assessor(assessor);
    }

    registry.register(spawn_tool);
    registry.register(WaitSubagentsTool::new(manager.clone()));
    registry.register(ListSubagentsTool::new(manager));
}

pub(crate) fn build_background_agent_runtime_components(
    storage: &Storage,
    kv_store: Arc<dyn KvStore>,
    assessor: Option<Arc<dyn AgentOperationAssessor>>,
) -> BackgroundAgentComponents {
    build_background_agent_components(
        storage.background_agents.clone(),
        storage.agents.clone(),
        storage.deliverables.clone(),
        SessionService::from_storage(storage),
        kv_store,
        assessor,
    )
}
