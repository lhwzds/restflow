use super::*;
use restflow_tools::FileConfig;
use restflow_traits::AgentOperationAssessor;

/// Create a tool registry with all available tools including storage-backed tools.
///
/// This function creates a registry with:
/// - Default tools from restflow-ai (http_request, send_email)
/// - SkillTool that can access skills from storage
/// - Memory search tool for unified memory and session search
/// - Agent memory CRUD tools (save_to_memory, read_memory, etc.) — always registered, agent_id is a tool input
/// - Optional security gate wiring for execution tools; `None` keeps default permissive behavior
#[allow(clippy::too_many_arguments)]
pub fn create_tool_registry(
    skill_storage: SkillStorage,
    memory_storage: MemoryStorage,
    chat_storage: ChatSessionStorage,
    channel_session_binding_storage: ChannelSessionBindingStorage,
    execution_trace_storage: ExecutionTraceStorage,
    kv_store_storage: KvStoreStorage,
    work_item_storage: WorkItemStorage,
    secret_storage: SecretStorage,
    config_storage: ConfigStorage,
    agent_storage: AgentStorage,
    background_agent_storage: BackgroundAgentStorage,
    trigger_storage: TriggerStorage,
    terminal_storage: TerminalSessionStorage,
    deliverable_storage: crate::storage::DeliverableStorage,
    accessor_id: Option<String>,
    agent_id: Option<String>,
    security_gate: Option<Arc<dyn SecurityGate>>,
) -> anyhow::Result<ToolRegistry> {
    create_tool_registry_with_assessor(
        skill_storage,
        memory_storage,
        chat_storage,
        channel_session_binding_storage,
        execution_trace_storage,
        kv_store_storage,
        work_item_storage,
        secret_storage,
        config_storage,
        agent_storage,
        background_agent_storage,
        trigger_storage,
        terminal_storage,
        deliverable_storage,
        accessor_id,
        agent_id,
        security_gate,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn create_tool_registry_with_assessor(
    skill_storage: SkillStorage,
    memory_storage: MemoryStorage,
    chat_storage: ChatSessionStorage,
    channel_session_binding_storage: ChannelSessionBindingStorage,
    execution_trace_storage: ExecutionTraceStorage,
    kv_store_storage: KvStoreStorage,
    work_item_storage: WorkItemStorage,
    secret_storage: SecretStorage,
    config_storage: ConfigStorage,
    agent_storage: AgentStorage,
    background_agent_storage: BackgroundAgentStorage,
    trigger_storage: TriggerStorage,
    terminal_storage: TerminalSessionStorage,
    deliverable_storage: crate::storage::DeliverableStorage,
    accessor_id: Option<String>,
    agent_id: Option<String>,
    security_gate: Option<Arc<dyn SecurityGate>>,
    assessor: Option<Arc<dyn AgentOperationAssessor>>,
) -> anyhow::Result<ToolRegistry> {
    let config_storage = Arc::new(config_storage);
    let agent_defaults = load_agent_defaults(&config_storage);
    let api_defaults = load_api_defaults(&config_storage);
    let registry_defaults = load_registry_defaults(&config_storage);

    let secret_resolver: SecretResolver = {
        let secrets = Arc::new(secret_storage.clone());
        Arc::new(move |key| secrets.get_secret(key).ok().flatten())
    };

    // Create adapters
    let skill_provider = Arc::new(SkillStorageProvider::new(skill_storage.clone()));
    let session_store = Arc::new(SessionStorageAdapter::new(
        crate::storage::SessionStorage::new(
            chat_storage.clone(),
            channel_session_binding_storage,
            execution_trace_storage.clone(),
        ),
        agent_storage.clone(),
        background_agent_storage.clone(),
    ));
    let memory_manager = Arc::new(MemoryManagerAdapter::new(memory_storage.clone()));
    let mem_store = Arc::new(DbMemoryStoreAdapter::new(memory_storage.clone()));
    let deliverable_store = Arc::new(DeliverableStoreAdapter::new(deliverable_storage.clone()));
    let search_engine = UnifiedSearchEngine::new(memory_storage, chat_storage.clone());
    let unified_search = Arc::new(UnifiedMemorySearchAdapter::new(search_engine));
    let ops_provider = Arc::new(OpsProviderAdapter::new(
        background_agent_storage.clone(),
        chat_storage,
    ));
    let kv_store = Arc::new(KvStoreAdapter::new(kv_store_storage, accessor_id));
    let work_item_provider = Arc::new(DbWorkItemAdapter::new(work_item_storage.clone()));
    let auth_store = Arc::new(AuthProfileStorageAdapter::new(secret_storage.clone()));
    let known_tools = Arc::new(RwLock::new(HashSet::new()));
    let agent_store = Arc::new(AgentStoreAdapter::new(
        agent_storage.clone(),
        skill_storage.clone(),
        secret_storage.clone(),
        background_agent_storage.clone(),
        known_tools.clone(),
    ));
    let background_agent_store = Arc::new(BackgroundAgentStoreAdapter::new(
        background_agent_storage,
        agent_storage.clone(),
        deliverable_storage,
    ));
    let marketplace_store = Arc::new(MarketplaceStoreAdapter::new_with_defaults(
        skill_storage,
        registry_defaults,
    ));
    let trigger_store = Arc::new(TriggerStoreAdapter::new(trigger_storage));
    let terminal_store = Arc::new(TerminalStoreAdapter::new(terminal_storage));
    let security_provider: Arc<_> = Arc::new(SecurityQueryProviderAdapter::with_config_storage(
        config_storage.clone(),
    ));

    let process_manager: Arc<dyn ProcessManager> =
        Arc::new(ProcessRegistry::new().with_ttl_seconds(agent_defaults.process_session_ttl_secs));
    let reply_sender: Arc<dyn ReplySender> = Arc::new(UnavailableReplySender);
    let llm_client_factory = build_llm_factory(Some(&secret_storage));
    let switch_model_tool = build_switch_model_tool(llm_client_factory.clone());

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
    builder = register_http_execution_tool(
        builder,
        security_gate.clone(),
        security_agent_id,
        DEFAULT_SECURITY_TASK_ID,
    )?;
    builder = register_send_email_execution_tool(
        builder,
        security_gate.clone(),
        security_agent_id,
        DEFAULT_SECURITY_TASK_ID,
    );
    builder = register_python_execution_tools(
        builder,
        security_gate.clone(),
        security_agent_id,
        DEFAULT_SECURITY_TASK_ID,
    );

    builder = if let Some(gate) = security_gate.clone() {
        builder.with_skill_tool_with_security(
            skill_provider,
            gate,
            security_agent_id,
            DEFAULT_SECURITY_TASK_ID,
        )
    } else {
        builder.with_skill_tool(skill_provider)
    };

    let mut builder = if let Some(assessor) = assessor.clone() {
        builder.with_agent_crud_and_assessor(agent_store, assessor.clone())
    } else {
        builder.with_agent_crud(agent_store)
    };
    builder = if let Some(assessor) = assessor.clone() {
        builder.with_background_agent_and_kv_and_assessor(
            background_agent_store,
            kv_store.clone(),
            assessor,
        )
    } else {
        builder.with_background_agent_and_kv(background_agent_store, kv_store.clone())
    };

    let mut registry = builder
        .with_telegram()?
        .with_discord()?
        .with_slack()?
        .with_browser_timeout(agent_defaults.browser_timeout_secs)?
        .with_patch_and_base_dir(None)
        .with_edit_and_diagnostics_and_base_dir(None, None)
        .with_multiedit_and_diagnostics_and_base_dir(None, None)
        .with_glob_and_base_dir(None)
        .with_grep_and_base_dir(None)
        .with_web_fetch()
        .with_jina_reader()?
        .with_web_search_with_resolver_and_defaults(
            secret_resolver.clone(),
            api_defaults.web_search_num_results,
        )?
        .with_transcribe_config(
            secret_resolver.clone(),
            restflow_tools::TranscribeConfig::default(),
        )?
        .with_vision(secret_resolver)?
        .with_session(session_store)
        .with_memory_management(memory_manager)
        .with_memory_store(mem_store)
        .with_deliverable(deliverable_store)
        .with_unified_search(unified_search)
        .with_ops(ops_provider)
        .with_kv_store(kv_store.clone())
        .with_work_items(work_item_provider)
        .with_task_list(Arc::new(DbWorkItemAdapter::new(work_item_storage.clone())))
        .with_auth_profile(auth_store)
        .with_secrets(Arc::new(secret_storage.clone()))
        .with_config(config_storage.clone())
        .with_marketplace(marketplace_store)
        .with_trigger(trigger_store)
        .with_terminal(terminal_store)
        .with_security_query(security_provider)
        .build();

    let process_tool = if let Some(gate) = security_gate {
        ProcessTool::new(process_manager).with_security(
            gate,
            security_agent_id,
            DEFAULT_SECURITY_TASK_ID,
        )
    } else {
        ProcessTool::new(process_manager)
    };
    registry.register(process_tool);
    registry.register(ReplyTool::new(reply_sender));
    registry.register(switch_model_tool);
    let subagent_manager = create_subagent_manager(
        agent_storage,
        &registry,
        llm_client_factory.clone(),
        config_storage,
        execution_trace_storage,
    );
    let mut spawn_tool =
        SpawnSubagentTool::new(subagent_manager.clone()).with_kv_store(kv_store.clone());
    if let Some(assessor) = assessor.clone() {
        spawn_tool = spawn_tool.with_assessor(assessor);
    }
    registry.register(spawn_tool);
    registry.register(WaitSubagentsTool::new(subagent_manager.clone()));
    registry.register(ListSubagentsTool::new(subagent_manager));

    // Populate known_tools for AgentStoreAdapter validation
    populate_known_tools_from_registry(&known_tools, &registry, Some(&KNOWN_TOOL_ALIASES));

    Ok(registry)
}
