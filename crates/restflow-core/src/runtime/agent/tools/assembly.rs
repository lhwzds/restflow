use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use restflow_tools::{
    BashConfig, EmailTool, FileConfig, HttpTool, PythonTool, RunPythonTool, ToolRegistryBuilder,
};
use restflow_traits::registry::ToolRegistry;
use restflow_traits::security::SecurityGate;

pub(crate) const KNOWN_TOOL_ALIASES: [(&str, &str); 7] = [
    ("http", "http_request"),
    ("email", "send_email"),
    ("telegram", "telegram_send"),
    ("discord", "discord_send"),
    ("slack", "slack_send"),
    ("use_skill", "skill"),
    ("python", "run_python"),
];

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
    allow_write: bool,
    security_gate: Option<Arc<dyn SecurityGate>>,
    agent_id: &str,
    task_id: &str,
) -> ToolRegistryBuilder {
    let config = FileConfig {
        allow_write,
        ..Default::default()
    };

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
