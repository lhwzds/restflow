use super::*;
use crate::tools::{BashConfig, FileConfig};
use types::AgentOperationAssessor;

/// Create the daemon-owned minimal tool registry.
///
/// This function creates a registry with:
/// - Core execution tools (`bash`, file/edit/patch/search helpers)
/// - `load_skill` for read-only skill discovery
/// - `run_skill` for executing installed skrun skills
/// - Optional security gate wiring for execution tools; `None` keeps default permissive behavior
#[allow(clippy::too_many_arguments)]
pub fn create_tool_registry(
    config_storage: ConfigStorage,
    agent_id: Option<String>,
    security_gate: Option<Arc<dyn SecurityGate>>,
) -> anyhow::Result<ToolRegistry> {
    create_tool_registry_with_assessor(config_storage, agent_id, security_gate, None)
}

pub fn create_tool_registry_with_assessor(
    config_storage: ConfigStorage,
    agent_id: Option<String>,
    security_gate: Option<Arc<dyn SecurityGate>>,
    _assessor: Option<Arc<dyn AgentOperationAssessor>>,
) -> anyhow::Result<ToolRegistry> {
    let config_storage = Arc::new(config_storage);
    let agent_defaults = load_agent_defaults(&config_storage);
    let skill_provider = Arc::new(SkrunSkillProvider::default());

    let mut builder = ToolRegistryBuilder::new();
    let security_agent_id = agent_id.as_deref().unwrap_or(DEFAULT_SECURITY_AGENT_ID);
    builder = builder.with_bash(BashConfig {
        timeout_secs: agent_defaults.bash_timeout_secs,
        ..Default::default()
    });
    builder = builder.with_file(FileConfig {
        allow_write: false,
        ..Default::default()
    });
    builder = if let Some(gate) = security_gate.clone() {
        builder.with_load_skill_with_security(
            skill_provider,
            gate,
            security_agent_id,
            DEFAULT_SECURITY_TASK_ID,
        )
    } else {
        builder.with_load_skill(skill_provider)
    };

    let mut run_skill_tool =
        crate::tools::RunSkillTool::new().with_root(crate::services::skills::skill_catalog_root()?);
    if let Some(gate) = security_gate.clone() {
        run_skill_tool =
            run_skill_tool.with_security(gate, security_agent_id, DEFAULT_SECURITY_TASK_ID);
    }
    builder.registry.register(run_skill_tool);

    let registry = builder
        .with_patch_and_base_dir(None)
        .with_edit_and_base_dir(None)
        .with_multiedit_and_base_dir(None)
        .with_glob_and_base_dir(None)
        .with_grep_and_base_dir(None)
        .build();

    Ok(registry)
}
