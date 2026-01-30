//! Real agent executor implementation for the task runner.
//!
//! This module provides `RealAgentExecutor`, which implements the
//! `AgentExecutor` trait by bridging to the `restflow_ai::AgentExecutor`.
//! It loads agent configuration from storage, builds the appropriate LLM
//! client, and executes the agent with the configured tools.

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::sync::Arc;

use restflow_ai::{
    AgentConfig, AgentExecutor as AiAgentExecutor, AnthropicClient, LlmClient, OpenAIClient,
    ToolRegistry,
};
use restflow_core::{models::ApiKeyConfig, storage::Storage, AIModel, Provider};

use super::runner::{AgentExecutor, ExecutionResult};

/// Real agent executor that bridges to restflow_ai::AgentExecutor.
///
/// This executor:
/// - Loads agent configuration from storage
/// - Resolves API keys (direct or from secrets)
/// - Creates the appropriate LLM client for the model
/// - Builds the system prompt from the agent's skill
/// - Executes the agent via the ReAct loop
pub struct RealAgentExecutor {
    storage: Arc<Storage>,
}

impl RealAgentExecutor {
    /// Create a new RealAgentExecutor with access to storage.
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }

    /// Get the API key for a model, resolving from config or secrets.
    ///
    /// Priority:
    /// 1. Agent-level api_key_config (if set)
    /// 2. Well-known secret names (e.g., OPENAI_API_KEY, ANTHROPIC_API_KEY)
    fn resolve_api_key(
        &self,
        provider: Provider,
        agent_api_key_config: Option<&ApiKeyConfig>,
    ) -> Result<String> {
        // First, check agent-level API key config
        if let Some(config) = agent_api_key_config {
            match config {
                ApiKeyConfig::Direct(key) => {
                    if !key.is_empty() {
                        return Ok(key.clone());
                    }
                }
                ApiKeyConfig::Secret(secret_name) => {
                    if let Some(secret_value) = self.storage.secrets.get_secret(secret_name)? {
                        return Ok(secret_value);
                    }
                    return Err(anyhow!("Secret '{}' not found", secret_name));
                }
            }
        }

        // Fall back to well-known secret names for each provider
        let secret_name = match provider {
            Provider::OpenAI => "OPENAI_API_KEY",
            Provider::Anthropic => "ANTHROPIC_API_KEY",
            Provider::DeepSeek => "DEEPSEEK_API_KEY",
        };

        if let Some(secret_value) = self.storage.secrets.get_secret(secret_name)? {
            return Ok(secret_value);
        }

        Err(anyhow!(
            "No API key configured for provider {:?}. Please add secret '{}' in Settings.",
            provider,
            secret_name
        ))
    }

    /// Create an LLM client for the given model.
    fn create_llm_client(
        &self,
        model: AIModel,
        api_key: &str,
    ) -> Result<Arc<dyn LlmClient>> {
        let model_str = model.as_str();

        match model.provider() {
            Provider::OpenAI => {
                let client = OpenAIClient::new(api_key)
                    .with_model(model_str);
                Ok(Arc::new(client))
            }
            Provider::Anthropic => {
                let client = AnthropicClient::new(api_key)
                    .with_model(model_str);
                Ok(Arc::new(client))
            }
            Provider::DeepSeek => {
                // DeepSeek uses OpenAI-compatible API
                let client = OpenAIClient::new(api_key)
                    .with_model(model_str)
                    .with_base_url("https://api.deepseek.com/v1");
                Ok(Arc::new(client))
            }
        }
    }

    /// Build the tool registry for an agent.
    ///
    /// If the agent has specific tools configured, only those tools are registered.
    /// Otherwise, a default set of tools is used.
    fn build_tool_registry(&self, _tool_names: Option<&[String]>) -> Arc<ToolRegistry> {
        // For Phase 1, we create an empty registry
        // Tools will be added in a future phase based on agent configuration
        let registry = ToolRegistry::new();

        // TODO: In future phases, register tools based on tool_names:
        // - HttpTool for HTTP requests
        // - EmailTool for sending emails
        // - TelegramTool for Telegram messages
        // - PythonTool for Python execution
        // - SkillTool for skill-based execution

        Arc::new(registry)
    }
}

#[async_trait]
impl AgentExecutor for RealAgentExecutor {
    /// Execute an agent with the given input.
    ///
    /// This method:
    /// 1. Loads the agent configuration from storage
    /// 2. Resolves the API key for the model
    /// 3. Creates the appropriate LLM client
    /// 4. Builds the system prompt (from agent config or skill)
    /// 5. Creates the tool registry
    /// 6. Executes the agent via restflow_ai::AgentExecutor
    /// 7. Returns the execution result with output and messages
    async fn execute(&self, agent_id: &str, input: Option<&str>) -> Result<ExecutionResult> {
        // 1. Load agent from storage
        let stored_agent = self
            .storage
            .agents
            .get_agent(agent_id.to_string())?
            .ok_or_else(|| anyhow!("Agent '{}' not found", agent_id))?;

        let agent_node = &stored_agent.agent;

        // 2. Resolve API key
        let api_key = self.resolve_api_key(
            agent_node.model.provider(),
            agent_node.api_key_config.as_ref(),
        )?;

        // 3. Create LLM client
        let llm = self.create_llm_client(agent_node.model, &api_key)?;

        // 4. Build tool registry
        let tools = self.build_tool_registry(agent_node.tools.as_deref());

        // 5. Build agent config
        let goal = input.unwrap_or("Execute the agent task");
        let mut config = AgentConfig::new(goal);

        // Set system prompt from agent configuration
        if let Some(prompt) = &agent_node.prompt {
            config = config.with_system_prompt(prompt);
        }

        // Set temperature if supported and configured
        if agent_node.model.supports_temperature()
            && let Some(temp) = agent_node.temperature
        {
            config = config.with_temperature(temp as f32);
        }

        // 6. Create and run the executor
        let executor = AiAgentExecutor::new(llm, tools);
        let result = executor.run(config).await?;

        // 7. Return result with messages for memory persistence
        if result.success {
            let output = result.answer.unwrap_or_else(|| "Task completed".to_string());
            let messages = result.state.messages.clone();
            Ok(ExecutionResult::success(output, messages))
        } else {
            Err(anyhow!(
                "Agent execution failed: {}",
                result.error.unwrap_or_else(|| "Unknown error".to_string())
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_core::models::AgentNode;
    use tempfile::tempdir;

    fn create_test_storage() -> (Arc<Storage>, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
        (Arc::new(storage), temp_dir)
    }

    #[test]
    fn test_executor_creation() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = RealAgentExecutor::new(storage);
        // Executor should be created successfully
        assert!(Arc::strong_count(&executor.storage) >= 1);
    }

    #[tokio::test]
    async fn test_executor_agent_not_found() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = RealAgentExecutor::new(storage);

        let result = executor.execute("nonexistent-agent", None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_executor_no_api_key() {
        let (storage, _temp_dir) = create_test_storage();

        // Create an agent without API key
        let agent_node = AgentNode::new(AIModel::ClaudeSonnet4_5);
        storage
            .agents
            .create_agent("Test Agent".to_string(), agent_node)
            .unwrap();

        let agents = storage.agents.list_agents().unwrap();
        let agent_id = &agents[0].id;

        let executor = RealAgentExecutor::new(storage);
        let result = executor.execute(agent_id, Some("test input")).await;

        // Should fail due to missing API key (no ANTHROPIC_API_KEY secret configured)
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("API key") || err_msg.contains("ANTHROPIC_API_KEY"),
            "Error should mention API key: {}",
            err_msg
        );
    }

    #[test]
    fn test_build_tool_registry() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = RealAgentExecutor::new(storage);

        // Build with no tools
        let registry = executor.build_tool_registry(None);
        assert!(registry.list().is_empty());

        // Build with tool names (currently returns empty, tools added in future)
        let tool_names = vec!["http".to_string(), "email".to_string()];
        let registry = executor.build_tool_registry(Some(&tool_names));
        // For Phase 1, registry is empty until tools are implemented
        assert!(registry.list().is_empty());
    }
}
