//! Real agent executor implementation for the task runner.
//!
//! This module provides `RealAgentExecutor`, which implements the
//! `AgentExecutor` trait by running the unified agent stack.
//! It loads agent configuration from storage, builds the appropriate LLM
//! client, and executes the agent with the configured tools.

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use crate::{
    AIModel, Provider,
    auth::AuthProfileManager,
    models::{AgentNode, ApiKeyConfig, SteerMessage},
    process::ProcessRegistry,
    storage::Storage,
};
use restflow_ai::{
    AiError, DefaultLlmClientFactory, LlmClient, LlmClientFactory, LlmProvider, SwappableLlm,
    SwitchModelTool,
};
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{info, warn};

use super::failover::{FailoverConfig, FailoverManager, execute_with_failover};
use super::retry::{RetryConfig, RetryState};
use super::runner::{AgentExecutor, ExecutionResult};
use crate::runtime::agent::{
    SubagentDeps, ToolRegistry, UnifiedAgent, UnifiedAgentConfig, build_agent_system_prompt,
    registry_from_allowlist, secret_resolver_from_storage,
};
use crate::runtime::subagent::{AgentDefinitionRegistry, SubagentConfig, SubagentTracker};

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
    #[allow(dead_code)]
    process_registry: Arc<ProcessRegistry>,
    auth_manager: Arc<AuthProfileManager>,
    subagent_tracker: Arc<SubagentTracker>,
    subagent_definitions: Arc<AgentDefinitionRegistry>,
    subagent_config: SubagentConfig,
}

impl RealAgentExecutor {
    /// Create a new RealAgentExecutor with access to storage.
    pub fn new(
        storage: Arc<Storage>,
        process_registry: Arc<ProcessRegistry>,
        auth_manager: Arc<AuthProfileManager>,
        subagent_tracker: Arc<SubagentTracker>,
        subagent_definitions: Arc<AgentDefinitionRegistry>,
        subagent_config: SubagentConfig,
    ) -> Self {
        Self {
            storage,
            process_registry,
            auth_manager,
            subagent_tracker,
            subagent_definitions,
            subagent_config,
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
            return profile.get_api_key(self.auth_manager.resolver());
        }

        // Fall back to well-known secret names for each provider
        let secret_name = provider.api_key_env();

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

    async fn build_api_keys(
        &self,
        agent_api_key_config: Option<&ApiKeyConfig>,
        primary_provider: Provider,
    ) -> HashMap<LlmProvider, String> {
        let mut keys = HashMap::new();

        for provider in Provider::all() {
            if let Ok(key) = self
                .resolve_api_key_for_model(*provider, agent_api_key_config, primary_provider)
                .await
            {
                keys.insert(provider.as_llm_provider(), key);
            }
        }

        keys
    }

    fn build_subagent_deps(&self, llm_client: Arc<dyn LlmClient>) -> SubagentDeps {
        SubagentDeps {
            tracker: self.subagent_tracker.clone(),
            definitions: self.subagent_definitions.clone(),
            llm_client,
            tool_registry: Arc::new(ToolRegistry::new()),
            config: self.subagent_config.clone(),
        }
    }

    /// Build the tool registry for an agent.
    ///
    /// If the agent has specific tools configured, only those tools are registered.
    /// Otherwise, an empty registry is used (secure default).
    fn build_tool_registry(
        &self,
        tool_names: Option<&[String]>,
        llm_client: Arc<dyn LlmClient>,
        swappable: Arc<SwappableLlm>,
        factory: Arc<dyn LlmClientFactory>,
        agent_id: Option<&str>,
    ) -> Arc<ToolRegistry> {
        let subagent_deps = self.build_subagent_deps(llm_client);
        let secret_resolver = Some(secret_resolver_from_storage(&self.storage));
        let mut registry = registry_from_allowlist(
            tool_names,
            Some(&subagent_deps),
            secret_resolver,
            Some(self.storage.as_ref()),
            agent_id,
        );

        let enable_switch = tool_names
            .map(|names| names.iter().any(|name| name == "switch_model"))
            .unwrap_or(false);

        if enable_switch {
            registry.register(SwitchModelTool::new(swappable, factory));
        }

        Arc::new(registry)
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_agent_with_client(
        &self,
        agent_node: &AgentNode,
        model: AIModel,
        llm_client: Arc<dyn LlmClient>,
        input: Option<&str>,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        factory: Arc<dyn LlmClientFactory>,
        agent_id: Option<&str>,
    ) -> Result<ExecutionResult> {
        let swappable = Arc::new(SwappableLlm::new(llm_client));
        let tools = self.build_tool_registry(
            agent_node.tools.as_deref(),
            swappable.clone(),
            swappable.clone(),
            factory,
            agent_id,
        );
        let system_prompt = build_agent_system_prompt(self.storage.clone(), agent_node)?;

        let mut config = UnifiedAgentConfig::default();
        if model.supports_temperature()
            && let Some(temp) = agent_node.temperature
        {
            config.temperature = temp as f32;
        }

        let mut agent = UnifiedAgent::new(swappable, tools, system_prompt, config);
        if let Some(rx) = steer_rx {
            agent = agent.with_steer_channel(rx);
        }

        let goal = input.unwrap_or("Execute the agent task");
        let result = agent.execute(goal).await?;

        if result.success {
            Ok(ExecutionResult::success(result.output, result.messages))
        } else {
            Err(anyhow!("Agent execution failed: {}", result.output))
        }
    }

    async fn execute_with_model(
        &self,
        agent_node: &AgentNode,
        model: AIModel,
        input: Option<&str>,
        primary_provider: Provider,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        agent_id: Option<&str>,
    ) -> Result<ExecutionResult> {
        let model_specs = AIModel::build_model_specs();
        let api_keys = self
            .build_api_keys(agent_node.api_key_config.as_ref(), primary_provider)
            .await;
        let factory = Arc::new(DefaultLlmClientFactory::new(api_keys, model_specs));

        let api_key = if model.is_codex_cli() {
            None
        } else if model.is_gemini_cli() {
            self.resolve_api_key_for_model(
                model.provider(),
                agent_node.api_key_config.as_ref(),
                primary_provider,
            )
            .await
            .ok()
        } else {
            Some(
                self.resolve_api_key_for_model(
                    model.provider(),
                    agent_node.api_key_config.as_ref(),
                    primary_provider,
                )
                .await?,
            )
        };

        let llm_client = factory.create_client(model.as_serialized_str(), api_key.as_deref())?;
        self.execute_agent_with_client(agent_node, model, llm_client, input, steer_rx, factory, agent_id)
            .await
    }

    async fn execute_with_profiles(
        &self,
        agent_node: &AgentNode,
        model: AIModel,
        input: Option<&str>,
        primary_provider: Provider,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        agent_id: Option<&str>,
    ) -> Result<ExecutionResult> {
        if agent_node.api_key_config.is_some() {
            return self
                .execute_with_model(agent_node, model, input, primary_provider, steer_rx, agent_id)
                .await;
        }

        let profiles = self
            .auth_manager
            .get_compatible_profiles_for_model_provider(model.provider())
            .await;

        if profiles.is_empty() {
            return self
                .execute_with_model(agent_node, model, input, primary_provider, steer_rx, agent_id)
                .await;
        }

        let mut last_error: Option<anyhow::Error> = None;
        let mut steer_rx = steer_rx;

        for profile in profiles {
            let api_key = match profile.get_api_key(self.auth_manager.resolver()) {
                Ok(key) => key,
                Err(error) => {
                    warn!(
                        profile_id = %profile.id,
                        profile_name = %profile.name,
                        model = ?model,
                        error = %error,
                        "Skipping profile because credential resolution failed"
                    );
                    continue;
                }
            };

            let model_specs = AIModel::build_model_specs();
            let api_keys = self
                .build_api_keys(agent_node.api_key_config.as_ref(), primary_provider)
                .await;
            let factory = Arc::new(DefaultLlmClientFactory::new(api_keys, model_specs));
            let llm_client =
                factory.create_client(model.as_serialized_str(), Some(api_key.as_str()))?;

            match self
                .execute_agent_with_client(
                    agent_node,
                    model,
                    llm_client,
                    input,
                    steer_rx.take(),
                    factory,
                    agent_id,
                )
                .await
            {
                Ok(result) => {
                    if let Err(error) = self.auth_manager.mark_success(&profile.id).await {
                        warn!(
                            profile_id = %profile.id,
                            profile_name = %profile.name,
                            model = ?model,
                            error = %error,
                            "Failed to mark profile success"
                        );
                    }
                    return Ok(result);
                }
                Err(error) => {
                    if is_credential_error(&error) {
                        if let Err(mark_error) = self.auth_manager.mark_failure(&profile.id).await {
                            warn!(
                                profile_id = %profile.id,
                                profile_name = %profile.name,
                                model = ?model,
                                error = %mark_error,
                                "Failed to mark profile failure"
                            );
                        }

                        warn!(
                            profile_id = %profile.id,
                            profile_name = %profile.name,
                            model = ?model,
                            error = %error,
                            "Profile failed with credential-related error, trying next profile"
                        );
                        last_error = Some(error);
                        continue;
                    }

                    return Err(error);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            anyhow!("All profiles exhausted for provider {:?}", model.provider())
        }))
    }
}

fn is_credential_error(error: &anyhow::Error) -> bool {
    if let Some(ai_error) = error.downcast_ref::<AiError>() {
        return match ai_error {
            AiError::LlmHttp { status, .. } => matches!(status, 401 | 403 | 429),
            AiError::Llm(message) => {
                let lower = message.to_lowercase();
                lower.contains("rate limit")
                    || lower.contains("429")
                    || lower.contains("unauthorized")
                    || lower.contains("forbidden")
                    || lower.contains("quota")
                    || lower.contains("billing")
                    || lower.contains("api key")
            }
            _ => false,
        };
    }

    let lower = error.to_string().to_lowercase();
    lower.contains("rate limit")
        || lower.contains("429")
        || lower.contains("unauthorized")
        || lower.contains("forbidden")
        || lower.contains("quota")
        || lower.contains("billing")
        || lower.contains("api key")
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
    async fn execute(
        &self,
        agent_id: &str,
        input: Option<&str>,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
    ) -> Result<ExecutionResult> {
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
        let mut steer_rx = steer_rx;

        loop {
            let input_ref = input_owned.as_deref();
            let agent_node_clone = agent_node.clone();
            // Note: steer_rx is consumed on first execution attempt only.
            // Retries after this point won't have steering support.
            let result = execute_with_failover(&failover_manager, |model| {
                let node = agent_node_clone.clone();
                let steer_rx = steer_rx.take();
                async move {
                    self.execute_with_profiles(&node, model, input_ref, primary_provider, steer_rx, Some(agent_id))
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
    use crate::models::AgentNode;
    use crate::runtime::subagent::{AgentDefinitionRegistry, SubagentConfig, SubagentTracker};
    use tempfile::tempdir;
    use tokio::sync::mpsc;

    fn create_test_storage() -> (Arc<Storage>, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
        (Arc::new(storage), temp_dir)
    }

    fn create_test_executor(storage: Arc<Storage>) -> RealAgentExecutor {
        let auth_manager = Arc::new(AuthProfileManager::new(Arc::new(storage.secrets.clone())));
        let (completion_tx, completion_rx) = mpsc::channel(10);
        let subagent_tracker = Arc::new(SubagentTracker::new(completion_tx, completion_rx));
        let subagent_definitions = Arc::new(AgentDefinitionRegistry::with_builtins());
        let subagent_config = SubagentConfig::default();
        RealAgentExecutor::new(
            storage,
            Arc::new(ProcessRegistry::new()),
            auth_manager,
            subagent_tracker,
            subagent_definitions,
            subagent_config,
        )
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

        let result = executor.execute("nonexistent-agent", None, None).await;
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
        let result = executor.execute(agent_id, Some("test input"), None).await;

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
    fn test_is_credential_error_for_http_statuses() {
        let rate_limit = anyhow::Error::new(AiError::LlmHttp {
            provider: "anthropic".to_string(),
            status: 429,
            message: "rate limited".to_string(),
            retry_after_secs: Some(1),
        });
        assert!(is_credential_error(&rate_limit));

        let unauthorized = anyhow::Error::new(AiError::LlmHttp {
            provider: "openai".to_string(),
            status: 401,
            message: "unauthorized".to_string(),
            retry_after_secs: None,
        });
        assert!(is_credential_error(&unauthorized));

        let server_error = anyhow::Error::new(AiError::LlmHttp {
            provider: "openai".to_string(),
            status: 500,
            message: "server error".to_string(),
            retry_after_secs: None,
        });
        assert!(!is_credential_error(&server_error));
    }

    #[test]
    fn test_is_credential_error_for_llm_message_fallback() {
        let err = anyhow::Error::new(AiError::Llm("Rate limit exceeded".to_string()));
        assert!(is_credential_error(&err));

        let err = anyhow::Error::new(AiError::Llm("context window exceeded".to_string()));
        assert!(!is_credential_error(&err));
    }

    // Note: test_build_tool_registry removed because build_tool_registry now requires
    // an LlmClient for SubagentDeps. The core logic (registry_from_allowlist) is
    // tested in restflow-tauri/src/agent/tools/mod.rs
}
