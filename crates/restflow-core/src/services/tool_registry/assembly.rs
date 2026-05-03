use super::*;
use restflow_tools::FileConfig;
use restflow_traits::AgentOperationAssessor;

/// Create the daemon-owned minimal tool registry.
///
/// This function creates a registry with:
/// - Core execution tools (`bash`, file/edit/patch/search helpers)
/// - `load_skill` for read-only skill discovery
/// - `run_skill` for executing installed skrun skills
/// - Optional security gate wiring for execution tools; `None` keeps default permissive behavior
#[allow(clippy::too_many_arguments)]
pub fn create_tool_registry(
    memory_storage: MemoryStorage,
    chat_storage: ChatSessionStorage,
    channel_session_binding_storage: ChannelSessionBindingStorage,
    execution_trace_storage: ExecutionTraceStorage,
    secret_storage: SecretStorage,
    config_storage: ConfigStorage,
    agent_storage: AgentStorage,
    task_storage: TaskStorage,
    terminal_storage: TerminalSessionStorage,
    run_artifact_storage: crate::storage::RunArtifactStorage,
    _accessor_id: Option<String>,
    agent_id: Option<String>,
    security_gate: Option<Arc<dyn SecurityGate>>,
) -> anyhow::Result<ToolRegistry> {
    create_tool_registry_with_assessor(
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage,
        secret_storage,
        config_storage,
        agent_storage,
        task_storage,
        terminal_storage,
        run_artifact_storage,
        _accessor_id,
        agent_id,
        security_gate,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn create_tool_registry_with_assessor(
    _memory_storage: MemoryStorage,
    _chat_storage: ChatSessionStorage,
    _channel_session_binding_storage: ChannelSessionBindingStorage,
    _execution_trace_storage: ExecutionTraceStorage,
    _secret_storage: SecretStorage,
    config_storage: ConfigStorage,
    _agent_storage: AgentStorage,
    _task_storage: TaskStorage,
    _terminal_storage: TerminalSessionStorage,
    _run_artifact_storage: crate::storage::RunArtifactStorage,
    _accessor_id: Option<String>,
    agent_id: Option<String>,
    security_gate: Option<Arc<dyn SecurityGate>>,
    _assessor: Option<Arc<dyn AgentOperationAssessor>>,
) -> anyhow::Result<ToolRegistry> {
    let config_storage = Arc::new(config_storage);
    let agent_defaults = load_agent_defaults(&config_storage);
    let skill_provider = Arc::new(SkrunSkillProvider::default());

    let mut builder = ToolRegistryBuilder::new();
    let security_agent_id = agent_id.as_deref().unwrap_or(DEFAULT_SECURITY_AGENT_ID);
    builder = register_bash_execution_tool(
        builder,
        restflow_tools::BashConfig {
            timeout_secs: agent_defaults.bash_timeout_secs,
            ..Default::default()
        },
        security_gate.clone(),
        security_agent_id,
        DEFAULT_SECURITY_TASK_ID,
    );
    builder = register_file_execution_tool(
        builder,
        FileConfig {
            allow_write: false,
            ..Default::default()
        },
        security_gate.clone(),
        security_agent_id,
        DEFAULT_SECURITY_TASK_ID,
    );
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

    let mut run_skill_tool = restflow_tools::RunSkillTool::new();
    if let Some(gate) = security_gate.clone() {
        run_skill_tool =
            run_skill_tool.with_security(gate, security_agent_id, DEFAULT_SECURITY_TASK_ID);
    }
    builder.registry.register(run_skill_tool);

    let registry = builder
        .with_patch_and_base_dir(None)
        .with_edit_and_diagnostics_and_base_dir(None, None)
        .with_multiedit_and_diagnostics_and_base_dir(None, None)
        .with_glob_and_base_dir(None)
        .with_grep_and_base_dir(None)
        .build();

    Ok(registry)
}
