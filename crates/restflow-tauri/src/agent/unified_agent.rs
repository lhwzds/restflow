//! Unified agent executor that uses the shared tool registry and agent config.

use anyhow::{Result, anyhow};
use restflow_ai::{AgentConfig, AgentExecutor, AgentResult, LlmClient, ToolOutput};
use restflow_ai::agent::{AgentContext, SkillSummary, load_workspace_context};
use restflow_ai::tools::{Tool as AiTool, ToolRegistry as AiToolRegistry};
use restflow_ai::DEFAULT_MAX_MESSAGES;
use restflow_core::models::AgentNode;
use restflow_core::storage::Storage;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

use crate::agent::{Tool, ToolDefinition, ToolRegistry};

/// Configuration for the unified agent.
#[derive(Debug, Clone)]
pub struct UnifiedAgentConfig {
    pub max_iterations: usize,
    pub tool_timeout: Duration,
    pub max_tool_result_length: usize,
    pub max_memory_messages: usize,
}

impl Default for UnifiedAgentConfig {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            tool_timeout: Duration::from_secs(30),
            max_tool_result_length: 4000,
            max_memory_messages: DEFAULT_MAX_MESSAGES,
        }
    }
}

/// Unified agent result including conversation messages.
#[derive(Debug, Clone)]
pub struct UnifiedAgentResult {
    pub output: String,
    pub messages: Vec<restflow_ai::Message>,
    pub total_tokens: u32,
}

/// Unified agent executor that runs a ReAct loop with shared tools.
pub struct UnifiedAgent {
    llm_client: Arc<dyn LlmClient>,
    tool_registry: Arc<ToolRegistry>,
    storage: Arc<Storage>,
    agent: AgentNode,
    config: UnifiedAgentConfig,
}

impl UnifiedAgent {
    pub fn new(
        llm_client: Arc<dyn LlmClient>,
        tool_registry: Arc<ToolRegistry>,
        storage: Arc<Storage>,
        agent: AgentNode,
        config: UnifiedAgentConfig,
    ) -> Self {
        Self {
            llm_client,
            tool_registry,
            storage,
            agent,
            config,
        }
    }

    pub async fn execute(&mut self, input: &str) -> Result<UnifiedAgentResult> {
        let tools = self.build_ai_tool_registry()?;
        let mut config = self.build_agent_config(input).await?;
        config = config
            .with_max_iterations(self.config.max_iterations)
            .with_tool_timeout(self.config.tool_timeout)
            .with_max_tool_result_length(self.config.max_tool_result_length)
            .with_max_memory_messages(self.config.max_memory_messages);

        let executor = AgentExecutor::new(self.llm_client.clone(), tools);
        let result = executor.run(config).await?;
        self.map_result(result)
    }

    async fn build_agent_config(&self, input: &str) -> Result<AgentConfig> {
        let mut config = AgentConfig::new(input);

        let base_prompt = self
            .agent
            .prompt
            .clone()
            .unwrap_or_else(|| "You are a helpful AI assistant.".to_string());
        config = config.with_system_prompt(base_prompt);

        if let Some(model) = self.agent.model
            && model.supports_temperature()
            && let Some(temp) = self.agent.temperature
        {
            config = config.with_temperature(temp as f32);
        }

        let context = self.build_agent_context().await?;
        if !context.is_empty() {
            config = config.with_agent_context(context);
        }

        Ok(config)
    }

    async fn build_agent_context(&self) -> Result<AgentContext> {
        let mut context = AgentContext::new();

        let db = self.storage.get_db();
        let skill_storage = restflow_core::storage::skill::SkillStorage::new(db.clone())?;
        let skills = skill_storage
            .list()?
            .into_iter()
            .map(|skill| SkillSummary {
                id: skill.id,
                name: skill.name,
                description: skill.description,
            })
            .collect::<Vec<_>>();

        if !skills.is_empty() {
            context = context.with_skills(skills);
        }

        if let Ok(workdir) = std::env::current_dir() {
            if let Some(content) = load_workspace_context(&workdir) {
                context = context.with_workspace_context(content);
            }
            context = context.with_workdir(workdir.to_string_lossy().to_string());
        }

        Ok(context)
    }

    fn build_ai_tool_registry(&self) -> Result<Arc<AiToolRegistry>> {
        let mut registry = AiToolRegistry::new();
        for name in self.tool_registry.list() {
            if let Some(tool) = self.tool_registry.get(&name) {
                registry.register(TauriToolAdapter::new(tool));
            }
        }
        Ok(Arc::new(registry))
    }

    fn map_result(&self, result: AgentResult) -> Result<UnifiedAgentResult> {
        if result.success {
            let output = result
                .answer
                .unwrap_or_else(|| "Task completed".to_string());
            Ok(UnifiedAgentResult {
                output,
                messages: result.state.messages.clone(),
                total_tokens: result.total_tokens,
            })
        } else {
            Err(anyhow!(
                "Agent execution failed: {}",
                result.error.unwrap_or_else(|| "Unknown error".to_string())
            ))
        }
    }
}

struct TauriToolAdapter {
    tool: Arc<dyn Tool>,
    definition: ToolDefinition,
}

impl TauriToolAdapter {
    fn new(tool: Arc<dyn Tool>) -> Self {
        let definition = tool.definition();
        Self { tool, definition }
    }
}

#[async_trait::async_trait]
impl AiTool for TauriToolAdapter {
    fn name(&self) -> &str {
        &self.definition.name
    }

    fn description(&self) -> &str {
        &self.definition.description
    }

    fn parameters_schema(&self) -> Value {
        self.definition.parameters.clone()
    }

    async fn execute(&self, input: Value) -> restflow_ai::Result<ToolOutput> {
        let result = self.tool.execute(input).await;
        match result {
            Ok(result) => Ok(ToolOutput {
                success: result.success,
                result: json!({ "output": result.output }),
                error: result.error,
            }),
            Err(err) => Ok(ToolOutput::error(err.to_string())),
        }
    }
}
