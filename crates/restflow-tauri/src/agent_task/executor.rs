//! Real agent executor implementation for the task runner.
//!
//! This module provides `RealAgentExecutor`, which implements the
//! `AgentExecutor` trait by bridging to the `restflow_ai::AgentExecutor`.
//! It loads agent configuration from storage, builds the appropriate LLM
//! client, and executes the agent with the configured tools.

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::sync::Arc;

use restflow_ai::{
    AgentConfig, AgentExecutor as AiAgentExecutor, AnthropicClient, LlmClient, OpenAIClient,
    ToolRegistry,
};
use restflow_core::{
    AIModel,
    Provider,
    auth::AuthProfileManager,
    models::{AgentNode, ApiKeyConfig},
    process::ProcessRegistry,
    storage::Storage,
};
use tokio::time::sleep;
use tracing::info;

use super::failover::{FailoverConfig, FailoverManager, execute_with_failover};
use super::retry::{RetryConfig, RetryState};
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
    process_registry: Arc<ProcessRegistry>,
    auth_manager: Arc<AuthProfileManager>,
}

impl RealAgentExecutor {
    /// Create a new RealAgentExecutor with access to storage.
    pub fn new(
        storage: Arc<Storage>,
        process_registry: Arc<ProcessRegistry>,
        auth_manager: Arc<AuthProfileManager>,
    ) -> Self {
        Self {
            storage,
            process_registry,
            auth_manager,
        }
    }

    /// Get the API key for a model, resolving from config or secrets.
    ///
    /// Priority:
    /// 1. Agent-level api_key_config (if set)
    /// 2. Well-known secret names (e.g., OPENAI_API_KEY, ANTHROPIC_API_KEY)
    async fn resolve_api_key(
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

        if let Some(profile) = self.auth_manager.get_credential_for_model(provider).await {
            info!(
                profile_name = %profile.name,
                auth_provider = %profile.provider,
                model_provider = ?provider,
                "Using auth profile for model provider"
            );
            return Ok(profile.get_api_key().to_string());
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

    /// Resolve API key, avoiding mismatched agent-level keys for fallback providers.
    async fn resolve_api_key_for_model(
        &self,
        provider: Provider,
        agent_api_key_config: Option<&ApiKeyConfig>,
        primary_provider: Provider,
    ) -> Result<String> {
        let config = if provider == primary_provider {
            agent_api_key_config
        } else {
            None
        };
        self.resolve_api_key(provider, config).await
    }

    /// Create an LLM client for the given model.
    fn create_llm_client(&self, model: AIModel, api_key: &str) -> Result<Arc<dyn LlmClient>> {
        let model_str = model.as_str();

        match model.provider() {
            Provider::OpenAI => {
                let client = OpenAIClient::new(api_key).with_model(model_str);
                Ok(Arc::new(client))
            }
            Provider::Anthropic => {
                let client = AnthropicClient::new(api_key).with_model(model_str);
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
        let registry = ToolRegistry::new().with_process_tool(self.process_registry.clone());
        Arc::new(registry)
    }

    async fn execute_with_model(
        &self,
        agent_node: &AgentNode,
        model: AIModel,
        input: Option<&str>,
        primary_provider: Provider,
    ) -> Result<ExecutionResult> {
        let api_key = self
            .resolve_api_key_for_model(
                model.provider(),
                agent_node.api_key_config.as_ref(),
                primary_provider,
            )
            .await?;

        let llm = self.create_llm_client(model, &api_key)?;
        let tools = self.build_tool_registry(agent_node.tools.as_deref());

        let goal = input.unwrap_or("Execute the agent task");
        let mut config = AgentConfig::new(goal);

        if let Some(prompt) = &agent_node.prompt {
            config = config.with_system_prompt(prompt);
        }

        if model.supports_temperature()
            && let Some(temp) = agent_node.temperature
        {
            config = config.with_temperature(temp as f32);
        }

        let executor = AiAgentExecutor::new(llm, tools);
        let result = executor.run(config).await?;

        if result.success {
            let output = result
                .answer
                .unwrap_or_else(|| "Task completed".to_string());
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
        let stored_agent = self
            .storage
            .agents
            .get_agent(agent_id.to_string())?
            .ok_or_else(|| anyhow!("Agent '{}' not found", agent_id))?;

        let agent_node = stored_agent.agent.clone();
        let primary_model = agent_node.require_model().map_err(|e| anyhow!(e))?;
        let primary_provider = primary_model.provider();

        let failover_manager = FailoverManager::new(FailoverConfig::with_primary(primary_model));
        let retry_config = RetryConfig::default();
        let mut retry_state = RetryState::new();
        let input_owned = input.map(|value| value.to_string());

        loop {
            let input_ref = input_owned.as_deref();
            let agent_node_clone = agent_node.clone();
            let result = execute_with_failover(&failover_manager, |model| {
                let node = agent_node_clone.clone();
                async move {
                    self.execute_with_model(&node, model, input_ref, primary_provider)
                        .await
                }
            })
            .await;

            match result {
                Ok((exec_result, _model)) => return Ok(exec_result),
                Err(err) => {
                    let error_msg = err.to_string();
                    if retry_state.should_retry(&retry_config, &error_msg) {
                        retry_state.record_failure(&error_msg, &retry_config);
                        let delay = retry_state.calculate_delay(&retry_config);
                        sleep(delay).await;
                        continue;
                    }
                    return Err(err);
                }
            }
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

    fn create_test_executor(storage: Arc<Storage>) -> RealAgentExecutor {
        let auth_manager = Arc::new(AuthProfileManager::new());
        RealAgentExecutor::new(storage, Arc::new(ProcessRegistry::new()), auth_manager)
    }

    #[test]
    fn test_executor_creation() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = create_test_executor(storage);
        // Executor should be created successfully
        assert!(Arc::strong_count(&executor.storage) >= 1);
    }

    #[tokio::test]
    async fn test_executor_agent_not_found() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = create_test_executor(storage);

        let result = executor.execute("nonexistent-agent", None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_executor_no_api_key() {
        let (storage, _temp_dir) = create_test_storage();

        // Create an agent without API key
        let agent_node = AgentNode::with_model(AIModel::ClaudeSonnet4_5);
        storage
            .agents
            .create_agent("Test Agent".to_string(), agent_node)
            .unwrap();

        let agents = storage.agents.list_agents().unwrap();
        let agent_id = &agents[0].id;

        let executor = create_test_executor(storage);
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
        let executor = create_test_executor(storage);

        // Build with no tools
        let registry = executor.build_tool_registry(None);
        assert!(!registry.list().is_empty());

        // Build with tool names (currently ignored in this phase)
        let tool_names = vec!["http".to_string(), "email".to_string()];
        let registry = executor.build_tool_registry(Some(&tool_names));
        assert!(!registry.list().is_empty());
        assert!(registry.has("process"));
    }
}
