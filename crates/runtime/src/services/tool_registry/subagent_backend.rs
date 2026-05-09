use super::*;

pub(super) struct ServiceSubagentRuntimeBundle {
    pub tracker: Arc<SubagentTracker>,
    pub definitions: Arc<dyn SubagentDefLookup>,
    pub llm_client: Arc<dyn LlmClient>,
    pub tool_registry: Arc<ToolRegistry>,
    pub config: SubagentConfig,
    pub llm_client_factory: Arc<dyn LlmClientFactory>,
}

pub(super) fn build_service_subagent_tool_registry(source: &ToolRegistry) -> ToolRegistry {
    let mut cloned = ToolRegistry::new();
    for name in main_agent_default_tool_names() {
        if let Some(tool) = source.get(&name) {
            cloned.register_arc(tool);
        }
    }
    cloned
}

struct ToolRegistrySubagentBackend {
    definitions: Arc<dyn SubagentDefLookup>,
    llm_client: Arc<dyn LlmClient>,
    tool_registry: Arc<ToolRegistry>,
    config: SubagentConfig,
    llm_client_factory: Arc<dyn LlmClientFactory>,
}

#[async_trait::async_trait]
impl ExecutionBackend for ToolRegistrySubagentBackend {
    fn load_chat_session(&self, session_id: &str) -> anyhow::Result<crate::models::ChatSession> {
        anyhow::bail!(
            "Chat session loading is not supported in service subagent backend: {session_id}"
        )
    }

    async fn execute_interactive_session_turn(
        &self,
        _session: &mut crate::models::ChatSession,
        _user_input: &str,
        _max_history: usize,
        _input_mode: crate::runtime::SessionInputMode,
        _emitter: Option<Box<dyn StreamEmitter>>,
        _options: crate::runtime::task_runtime::SessionTurnRuntimeOptions,
    ) -> anyhow::Result<crate::runtime::SessionExecutionResult> {
        anyhow::bail!("Interactive execution is not supported in service subagent backend")
    }

    async fn execute_task(
        &self,
        _agent_id: &str,
        _task_id: Option<&str>,
        _input: Option<&str>,
        _steer_rx: Option<mpsc::Receiver<crate::models::SteerMessage>>,
        _emitter: Option<Box<dyn StreamEmitter>>,
    ) -> anyhow::Result<crate::runtime::ExecutionResult> {
        anyhow::bail!("Task execution is not supported in service subagent backend")
    }

    async fn execute_task_from_state(
        &self,
        _agent_id: &str,
        _task_id: Option<&str>,
        _state: AgentState,
        _steer_rx: Option<mpsc::Receiver<crate::models::SteerMessage>>,
        _emitter: Option<Box<dyn StreamEmitter>>,
    ) -> anyhow::Result<crate::runtime::ExecutionResult> {
        anyhow::bail!("Background resume is not supported in service subagent backend")
    }

    async fn execute_subagent_plan(&self, plan: ExecutionPlan) -> anyhow::Result<ExecutionOutcome> {
        execute_subagent_plan(
            self.definitions.clone(),
            self.llm_client.clone(),
            self.tool_registry.clone(),
            self.config.clone(),
            plan,
            SubagentExecutionBridge {
                llm_client_factory: Some(self.llm_client_factory.clone()),
                orchestrator: None,
            },
        )
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))
    }
}

pub(super) fn build_service_subagent_runtime_bundle(
    agent_storage: AgentStorage,
    base_registry: &ToolRegistry,
    llm_client_factory: Arc<dyn LlmClientFactory>,
    config_storage: Arc<ConfigStorage>,
) -> ServiceSubagentRuntimeBundle {
    let (completion_tx, completion_rx) = mpsc::channel(128);
    let tracker = Arc::new(SubagentTracker::new(completion_tx, completion_rx));
    let definitions = Arc::new(StorageBackedSubagentLookup::new(agent_storage));
    let llm_client: Arc<dyn LlmClient> = Arc::new(CodexClient::new());
    let subagent_config = load_subagent_config(&config_storage);
    let tool_registry = Arc::new(build_service_subagent_tool_registry(base_registry));
    ServiceSubagentRuntimeBundle {
        tracker,
        definitions,
        llm_client,
        tool_registry,
        config: subagent_config,
        llm_client_factory,
    }
}

pub(super) fn build_service_subagent_manager(
    bundle: &ServiceSubagentRuntimeBundle,
) -> SubagentManagerImpl {
    let orchestrator = Arc::new(AgentOrchestratorImpl::new(Arc::new(
        ToolRegistrySubagentBackend {
            definitions: bundle.definitions.clone(),
            llm_client: bundle.llm_client.clone(),
            tool_registry: bundle.tool_registry.clone(),
            config: bundle.config.clone(),
            llm_client_factory: bundle.llm_client_factory.clone(),
        },
    )));
    SubagentManagerImpl::new(
        bundle.tracker.clone(),
        bundle.definitions.clone(),
        bundle.llm_client.clone(),
        bundle.tool_registry.clone(),
        bundle.config.clone(),
    )
    .with_llm_client_factory(bundle.llm_client_factory.clone())
    .with_orchestrator(orchestrator)
}

#[cfg(test)]
pub(super) fn create_subagent_manager(
    agent_storage: AgentStorage,
    base_registry: &ToolRegistry,
    llm_client_factory: Arc<dyn LlmClientFactory>,
    config_storage: Arc<ConfigStorage>,
) -> Arc<dyn types::SubagentManager> {
    let bundle = build_service_subagent_runtime_bundle(
        agent_storage,
        base_registry,
        llm_client_factory,
        config_storage,
    );
    Arc::new(build_service_subagent_manager(&bundle))
}
