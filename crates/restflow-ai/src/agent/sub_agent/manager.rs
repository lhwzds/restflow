use std::sync::Arc;

use crate::llm::{LlmClient, LlmClientFactory};
use crate::tools::ToolRegistry;
use restflow_traits::boundary::subagent::spawn_request_from_contract;
use restflow_traits::AgentOrchestrator;
use restflow_traits::ToolError;
use restflow_traits::subagent::{
    ContractSubagentSpawnRequest, SpawnHandle, SubagentCompletion, SubagentConfig,
    SubagentDefLookup, SubagentDefSummary, SubagentManager, SubagentState,
};

use super::spawn::{SubagentExecutionBridge, spawn_subagent};
use super::tracker::SubagentTracker;

/// Dependencies needed for sub-agent tools.
#[derive(Clone)]
pub struct SubagentDeps {
    pub tracker: Arc<SubagentTracker>,
    pub definitions: Arc<dyn SubagentDefLookup>,
    pub llm_client: Arc<dyn LlmClient>,
    pub tool_registry: Arc<ToolRegistry>,
    pub config: SubagentConfig,
    /// Optional factory for creating LLM clients when a per-spawn model is requested.
    pub llm_client_factory: Option<Arc<dyn LlmClientFactory>>,
    /// Optional shared orchestrator bridge for unified execution.
    pub orchestrator: Option<Arc<dyn AgentOrchestrator>>,
}

/// Concrete implementation of [`SubagentManager`].
#[derive(Clone)]
pub struct SubagentManagerImpl {
    pub tracker: Arc<SubagentTracker>,
    pub definitions: Arc<dyn SubagentDefLookup>,
    pub llm_client: Arc<dyn LlmClient>,
    pub tool_registry: Arc<ToolRegistry>,
    pub config: SubagentConfig,
    /// Optional factory for creating LLM clients when a per-spawn model is requested.
    pub llm_client_factory: Option<Arc<dyn LlmClientFactory>>,
    /// Optional shared orchestrator bridge for unified execution.
    pub orchestrator: Option<Arc<dyn AgentOrchestrator>>,
}

impl SubagentManagerImpl {
    pub fn new(
        tracker: Arc<SubagentTracker>,
        definitions: Arc<dyn SubagentDefLookup>,
        llm_client: Arc<dyn LlmClient>,
        tool_registry: Arc<ToolRegistry>,
        config: SubagentConfig,
    ) -> Self {
        Self {
            tracker,
            definitions,
            llm_client,
            tool_registry,
            config,
            llm_client_factory: None,
            orchestrator: None,
        }
    }

    /// Attach a shared orchestrator bridge for future spawns.
    pub fn with_orchestrator(mut self, orchestrator: Arc<dyn AgentOrchestrator>) -> Self {
        self.orchestrator = Some(orchestrator);
        self
    }

    /// Create from existing [`SubagentDeps`].
    pub fn from_deps(deps: &SubagentDeps) -> Self {
        Self {
            tracker: deps.tracker.clone(),
            definitions: deps.definitions.clone(),
            llm_client: deps.llm_client.clone(),
            tool_registry: deps.tool_registry.clone(),
            config: deps.config.clone(),
            llm_client_factory: deps.llm_client_factory.clone(),
            orchestrator: deps.orchestrator.clone(),
        }
    }
}

#[async_trait::async_trait]
impl SubagentManager for SubagentManagerImpl {
    fn spawn(
        &self,
        request: ContractSubagentSpawnRequest,
    ) -> std::result::Result<SpawnHandle, ToolError> {
        let available_agents = self.definitions.list_callable();
        let request = spawn_request_from_contract(&available_agents, request)?;
        spawn_subagent(
            self.tracker.clone(),
            self.definitions.clone(),
            self.llm_client.clone(),
            self.tool_registry.clone(),
            self.config.clone(),
            request,
            SubagentExecutionBridge {
                llm_client_factory: self.llm_client_factory.clone(),
                orchestrator: self.orchestrator.clone(),
                telemetry_sink: self.tracker.telemetry_sink(),
            },
        )
        .map_err(|error| ToolError::Tool(error.to_string()))
    }

    fn list_callable(&self) -> Vec<SubagentDefSummary> {
        self.definitions.list_callable()
    }

    fn list_running(&self) -> Vec<SubagentState> {
        self.tracker.running()
    }

    fn running_count(&self) -> usize {
        self.tracker.running_count()
    }

    async fn wait(&self, task_id: &str) -> Option<SubagentCompletion> {
        self.tracker.wait(task_id).await
    }

    fn config(&self) -> &SubagentConfig {
        &self.config
    }
}
