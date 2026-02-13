//! Real agent executor implementation for the task runner.
//!
//! This module provides `AgentRuntimeExecutor`, which implements the
//! `AgentExecutor` trait by running the shared agent execution engine.
//! It loads agent configuration from storage, builds the appropriate LLM
//! client, and executes the agent with the configured tools.

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::{
    AIModel, Provider,
    auth::{AuthProfileManager, AuthProvider},
    models::{
        AgentNode, ApiKeyConfig, ChatMessage, ChatRole, ChatSession, MemoryConfig, SteerMessage,
    },
    process::ProcessRegistry,
    prompt_files,
    storage::Storage,
};
use restflow_ai::agent::StreamEmitter;
use restflow_ai::llm::Message;
use restflow_ai::tools::PythonRuntime;
use restflow_ai::{
    AgentConfig as ReActAgentConfig, AgentExecutor as ReActAgentExecutor, AiError, CodexClient,
    CompactionConfig, DefaultLlmClientFactory, LlmClient, LlmClientFactory, LlmProvider,
    ProcessTool, ReplySender, ReplyTool, SwappableLlm, SwitchModelTool,
};
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{debug, info, warn};

use super::failover::{FailoverConfig, FailoverManager, execute_with_failover};
use super::retry::{RetryConfig, RetryState};
use super::runner::{AgentExecutor, ExecutionResult};
use crate::runtime::agent::{
    AgentExecutionEngine, AgentExecutionEngineConfig, SubagentDeps, ToolRegistry,
    build_agent_system_prompt, effective_main_agent_tool_names, registry_from_allowlist,
    resolve_python_runtime_policy, secret_resolver_from_storage,
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
pub struct AgentRuntimeExecutor {
    storage: Arc<Storage>,
    process_registry: Arc<ProcessRegistry>,
    auth_manager: Arc<AuthProfileManager>,
    subagent_tracker: Arc<SubagentTracker>,
    subagent_definitions: Arc<AgentDefinitionRegistry>,
    subagent_config: SubagentConfig,
    reply_sender: Option<Arc<dyn ReplySender>>,
}

/// Result of executing a chat turn for a persisted chat session.
#[derive(Debug, Clone)]
pub struct SessionExecutionResult {
    pub output: String,
    pub iterations: u32,
    pub active_model: String,
}

/// Controls whether the latest user input has already been persisted
/// to the chat session before execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionInputMode {
    /// Latest user input is already stored as the newest session message.
    PersistedInSession,
    /// Latest user input is provided only as runtime input for this turn.
    EphemeralInput,
}

impl AgentRuntimeExecutor {
    /// Create a new AgentRuntimeExecutor with access to storage.
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
            reply_sender: None,
        }
    }

    /// Set a reply sender so the agent can send intermediate messages.
    pub fn with_reply_sender(mut self, sender: Arc<dyn ReplySender>) -> Self {
        self.reply_sender = Some(sender);
        self
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

    fn default_model_for_provider(provider: Provider) -> AIModel {
        match provider {
            Provider::OpenAI => AIModel::Gpt5,
            Provider::Anthropic => AIModel::ClaudeOpus4_6,
            Provider::DeepSeek => AIModel::DeepseekChat,
            Provider::Google => AIModel::Gemini25Pro,
            Provider::Groq => AIModel::GroqLlama4Maverick,
            Provider::OpenRouter => AIModel::OpenRouterAuto,
            Provider::XAI => AIModel::Grok4,
            Provider::Qwen => AIModel::Qwen3Max,
            Provider::Zhipu => AIModel::Glm4_7,
            Provider::Moonshot => AIModel::KimiK2_5,
            Provider::Doubao => AIModel::DoubaoPro,
            Provider::Yi => AIModel::YiLightning,
            Provider::SiliconFlow => AIModel::SiliconFlowAuto,
        }
    }

    fn context_window_for_model(model: AIModel) -> usize {
        match model {
            AIModel::ClaudeOpus4_6
            | AIModel::ClaudeSonnet4_5
            | AIModel::ClaudeHaiku4_5
            | AIModel::ClaudeCodeOpus
            | AIModel::ClaudeCodeSonnet
            | AIModel::ClaudeCodeHaiku => 200_000,
            AIModel::Gpt5
            | AIModel::Gpt5Mini
            | AIModel::Gpt5Nano
            | AIModel::Gpt5Pro
            | AIModel::Gpt5_1
            | AIModel::Gpt5_2
            | AIModel::Gpt5Codex
            | AIModel::Gpt5_1Codex
            | AIModel::Gpt5_2Codex
            | AIModel::CodexCli => 128_000,
            AIModel::DeepseekChat | AIModel::DeepseekReasoner => 64_000,
            AIModel::Gemini25Pro
            | AIModel::Gemini25Flash
            | AIModel::Gemini3Pro
            | AIModel::Gemini3Flash
            | AIModel::GeminiCli => 1_000_000,
            _ => 128_000,
        }
    }

    fn build_compaction_config(memory: &MemoryConfig) -> Option<CompactionConfig> {
        if !memory.enable_compaction {
            return None;
        }

        Some(CompactionConfig {
            threshold_ratio: memory.compaction_threshold_ratio,
            max_summary_tokens: memory.max_summary_tokens,
            ..CompactionConfig::default()
        })
    }

    fn has_non_empty_secret(&self, name: &str) -> Result<bool> {
        Ok(self
            .storage
            .secrets
            .get_secret(name)?
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty()))
    }

    async fn resolve_model_from_stored_credentials(&self) -> Result<Option<AIModel>> {
        // Prefer Codex CLI model only when a dedicated OpenAI Codex profile exists.
        if self
            .auth_manager
            .get_available_profile(AuthProvider::OpenAICodex)
            .await
            .is_some()
        {
            return Ok(Some(AIModel::CodexCli));
        }

        // Then try provider-specific auth profiles.
        let profile_order = [
            (AuthProvider::ClaudeCode, AIModel::ClaudeCodeOpus),
            (AuthProvider::Anthropic, AIModel::ClaudeOpus4_6),
            (AuthProvider::OpenAI, AIModel::Gpt5),
            (AuthProvider::Google, AIModel::Gemini25Pro),
        ];
        for (provider, model) in profile_order {
            if self
                .auth_manager
                .get_available_profile(provider)
                .await
                .is_some()
            {
                return Ok(Some(model));
            }
        }

        // Finally, fall back to explicit provider secrets in storage.
        for provider in Provider::all() {
            if self.has_non_empty_secret(provider.api_key_env())? {
                return Ok(Some(Self::default_model_for_provider(*provider)));
            }
        }

        Ok(None)
    }

    async fn resolve_primary_model(&self, agent_node: &AgentNode) -> Result<AIModel> {
        if let Some(model) = agent_node.model {
            return Ok(model);
        }

        if let Some(model) = self.resolve_model_from_stored_credentials().await? {
            info!(
                selected_model = %model.as_str(),
                "Resolved model from stored credentials for agent without explicit model"
            );
            return Ok(model);
        }

        Err(anyhow!(
            "Model not specified. Please set a model for this agent or configure a compatible API secret/auth profile."
        ))
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

    fn create_llm_client(
        factory: &dyn LlmClientFactory,
        model: AIModel,
        api_key: Option<&str>,
        agent_node: &AgentNode,
    ) -> Result<Arc<dyn LlmClient>> {
        if model.is_codex_cli() {
            let mut client = CodexClient::new().with_model(model.as_serialized_str());
            if let Some(effort) = agent_node
                .codex_cli_reasoning_effort
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                client = client.with_reasoning_effort(effort);
            }
            if let Some(mode) = agent_node.codex_cli_execution_mode.as_ref() {
                client = client.with_execution_mode(mode.as_str());
            }
            return Ok(Arc::new(client));
        }

        Ok(factory.create_client(model.as_serialized_str(), api_key)?)
    }

    fn build_subagent_deps(
        &self,
        llm_client: Arc<dyn LlmClient>,
        tool_registry: Arc<ToolRegistry>,
    ) -> SubagentDeps {
        SubagentDeps {
            tracker: self.subagent_tracker.clone(),
            definitions: self.subagent_definitions.clone(),
            llm_client,
            tool_registry,
            config: self.subagent_config.clone(),
        }
    }

    /// Build a credential-aware failover config for the given primary model.
    async fn build_failover_config(
        &self,
        primary: AIModel,
        agent_api_key_config: Option<&ApiKeyConfig>,
    ) -> FailoverConfig {
        let primary_provider = primary.provider();
        let api_keys = self
            .build_api_keys(agent_api_key_config, primary_provider)
            .await;

        let available_providers: HashSet<Provider> = api_keys
            .keys()
            .filter_map(|llm_provider| {
                Provider::all()
                    .iter()
                    .find(|p| p.as_llm_provider() == *llm_provider)
                    .copied()
            })
            .collect();

        let config = FailoverConfig::build_smart(primary, &available_providers);

        info!(
            primary = %primary.as_str(),
            fallbacks = ?config.fallbacks.iter().map(|m| m.as_str()).collect::<Vec<_>>(),
            "Built failover chain with {} available fallbacks",
            config.fallbacks.len()
        );

        config
    }

    fn build_background_system_prompt(
        &self,
        agent_node: &AgentNode,
        agent_id: Option<&str>,
        background_task_id: Option<&str>,
    ) -> Result<String> {
        let base_prompt = build_agent_system_prompt(self.storage.clone(), agent_node, agent_id)?;
        let policy_prompt = prompt_files::load_background_agent_policy(background_task_id)?;
        if policy_prompt.trim().is_empty() {
            return Ok(base_prompt);
        }
        Ok(format!("{base_prompt}\n\n{policy_prompt}"))
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
        python_runtime: PythonRuntime,
    ) -> Arc<ToolRegistry> {
        let filtered_tool_names = self.filter_requested_tool_names(tool_names);
        let filtered_tool_names_ref = filtered_tool_names.as_deref();
        let secret_resolver = Some(secret_resolver_from_storage(&self.storage));
        let subagent_tool_registry = Arc::new(registry_from_allowlist(
            filtered_tool_names_ref,
            None,
            secret_resolver.clone(),
            Some(self.storage.as_ref()),
            agent_id,
            Some(python_runtime.clone()),
        ));
        let subagent_deps = self.build_subagent_deps(llm_client, subagent_tool_registry);
        let mut registry = registry_from_allowlist(
            filtered_tool_names_ref,
            Some(&subagent_deps),
            secret_resolver,
            Some(self.storage.as_ref()),
            agent_id,
            Some(python_runtime),
        );

        let requested = |name: &str| {
            filtered_tool_names_ref
                .map(|names| names.iter().any(|n| n == name))
                .unwrap_or(false)
        };

        if requested("switch_model") {
            registry.register(SwitchModelTool::new(swappable, factory));
        }

        if requested("process") {
            registry.register(ProcessTool::new(self.process_registry.clone()));
        }

        if requested("reply")
            && let Some(sender) = &self.reply_sender
        {
            registry.register(ReplyTool::new(sender.clone()));
        }

        Arc::new(registry)
    }

    fn filter_requested_tool_names(&self, tool_names: Option<&[String]>) -> Option<Vec<String>> {
        let names = tool_names?;
        let has_reply_sender = self.reply_sender.is_some();

        Some(
            names
                .iter()
                .filter_map(|name| {
                    if name == "reply" && !has_reply_sender {
                        debug!(
                            tool_name = "reply",
                            "Reply sender missing in this execution context; skipping tool"
                        );
                        return None;
                    }
                    Some(name.clone())
                })
                .collect(),
        )
    }

    /// Resolve the stored agent referenced by a chat session.
    ///
    /// If the session references a missing agent, this method falls back to
    /// the "default" agent (or the first available one) and updates the session.
    fn resolve_stored_agent_for_session(
        &self,
        session: &mut ChatSession,
    ) -> Result<crate::storage::agent::StoredAgent> {
        if let Some(agent) = self.storage.agents.get_agent(session.agent_id.clone())? {
            return Ok(agent);
        }

        let agents = self.storage.agents.list_agents()?;
        let fallback = agents
            .iter()
            .find(|agent| agent.name.eq_ignore_ascii_case("default"))
            .cloned()
            .or_else(|| agents.first().cloned())
            .ok_or_else(|| anyhow!("No AI agent configured"))?;

        let fallback_model = fallback
            .agent
            .model
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        session.agent_id = fallback.id.clone();
        session.model = fallback_model.clone();
        session.metadata.last_model = Some(fallback_model);

        Ok(fallback)
    }

    fn chat_message_to_llm_message(message: &ChatMessage) -> Message {
        match message.role {
            ChatRole::User => Message::user(message.content.clone()),
            ChatRole::Assistant => Message::assistant(message.content.clone()),
            ChatRole::System => Message::system(message.content.clone()),
        }
    }

    fn session_messages_for_context(session: &ChatSession) -> Vec<ChatMessage> {
        if session.messages.is_empty() {
            return Vec::new();
        }

        if let Some(summary_id) = session.summary_message_id.as_ref()
            && let Some(idx) = session.messages.iter().position(|m| &m.id == summary_id)
        {
            let mut messages = session.messages[idx..].to_vec();
            if let Some(summary) = messages.first_mut() {
                summary.role = ChatRole::User;
            }
            return messages;
        }

        session.messages.clone()
    }

    fn add_session_history(
        agent: &mut AgentExecutionEngine,
        session: &ChatSession,
        max_messages: usize,
        input_mode: SessionInputMode,
    ) {
        let mut messages = Self::session_messages_for_context(session);
        if messages.is_empty() {
            return;
        }

        // Exclude the latest user input because it will be passed to execute()
        // separately for persisted-input flows.
        if input_mode == SessionInputMode::PersistedInSession
            && matches!(messages.last().map(|m| &m.role), Some(ChatRole::User))
        {
            messages.pop();
        }

        let start = messages.len().saturating_sub(max_messages);
        for message in &messages[start..] {
            agent.add_history_message(Self::chat_message_to_llm_message(message));
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_session_with_client(
        &self,
        agent_node: &AgentNode,
        model: AIModel,
        llm_client: Arc<dyn LlmClient>,
        session: &ChatSession,
        user_input: &str,
        max_history: usize,
        input_mode: SessionInputMode,
        factory: Arc<dyn LlmClientFactory>,
        agent_id: Option<&str>,
    ) -> Result<SessionExecutionResult> {
        let swappable = Arc::new(SwappableLlm::new(llm_client));
        let effective_tools = effective_main_agent_tool_names(agent_node.tools.as_deref());
        let python_runtime =
            resolve_python_runtime_policy(agent_node.python_runtime_policy.as_ref());
        let tools = self.build_tool_registry(
            Some(&effective_tools),
            swappable.clone(),
            swappable.clone(),
            factory,
            agent_id,
            python_runtime,
        );
        let system_prompt = build_agent_system_prompt(self.storage.clone(), agent_node, agent_id)?;

        let mut config = AgentExecutionEngineConfig::default();
        if model.supports_temperature()
            && let Some(temp) = agent_node.temperature
        {
            config.temperature = temp as f32;
        }

        let mut agent = AgentExecutionEngine::new(swappable.clone(), tools, system_prompt, config);
        Self::add_session_history(&mut agent, session, max_history, input_mode);

        let result = agent.execute(user_input).await?;
        if !result.success {
            return Err(anyhow!("Agent execution failed: {}", result.output));
        }

        Ok(SessionExecutionResult {
            output: result.output,
            iterations: result.iterations as u32,
            active_model: swappable.current_model(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_session_with_model(
        &self,
        agent_node: &AgentNode,
        model: AIModel,
        session: &ChatSession,
        user_input: &str,
        primary_provider: Provider,
        max_history: usize,
        input_mode: SessionInputMode,
        agent_id: Option<&str>,
    ) -> Result<SessionExecutionResult> {
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

        let llm_client =
            Self::create_llm_client(factory.as_ref(), model, api_key.as_deref(), agent_node)?;
        self.execute_session_with_client(
            agent_node,
            model,
            llm_client,
            session,
            user_input,
            max_history,
            input_mode,
            factory,
            agent_id,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_session_with_profiles(
        &self,
        agent_node: &AgentNode,
        model: AIModel,
        session: &ChatSession,
        user_input: &str,
        primary_provider: Provider,
        max_history: usize,
        input_mode: SessionInputMode,
        agent_id: Option<&str>,
    ) -> Result<SessionExecutionResult> {
        if model.is_codex_cli() || agent_node.api_key_config.is_some() {
            return self
                .execute_session_with_model(
                    agent_node,
                    model,
                    session,
                    user_input,
                    primary_provider,
                    max_history,
                    input_mode,
                    agent_id,
                )
                .await;
        }

        let profiles = self
            .auth_manager
            .get_compatible_profiles_for_model_provider(model.provider())
            .await;
        if profiles.is_empty() {
            return self
                .execute_session_with_model(
                    agent_node,
                    model,
                    session,
                    user_input,
                    primary_provider,
                    max_history,
                    input_mode,
                    agent_id,
                )
                .await;
        }

        let mut last_error: Option<anyhow::Error> = None;
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
            let llm_client = Self::create_llm_client(
                factory.as_ref(),
                model,
                Some(api_key.as_str()),
                agent_node,
            )?;

            match self
                .execute_session_with_client(
                    agent_node,
                    model,
                    llm_client,
                    session,
                    user_input,
                    max_history,
                    input_mode,
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

    /// Execute a chat turn for an existing chat session.
    ///
    /// This method keeps chat execution in daemon-side runtime logic so UI
    /// clients (Tauri/HTTP/MCP) can share the same execution behavior.
    pub async fn execute_session_turn(
        &self,
        session: &mut ChatSession,
        user_input: &str,
        max_history: usize,
        input_mode: SessionInputMode,
    ) -> Result<SessionExecutionResult> {
        let stored_agent = self.resolve_stored_agent_for_session(session)?;
        let agent_node = stored_agent.agent.clone();
        let primary_model = self.resolve_primary_model(&agent_node).await?;
        let primary_provider = primary_model.provider();
        let failover_config = self
            .build_failover_config(primary_model, agent_node.api_key_config.as_ref())
            .await;
        let failover_manager = FailoverManager::new(failover_config);
        let retry_config = RetryConfig::default();
        let mut retry_state = RetryState::new();
        let session_snapshot = session.clone();
        let agent_id = session.agent_id.clone();

        loop {
            let node = agent_node.clone();
            let session_for_execution = session_snapshot.clone();
            let result = execute_with_failover(&failover_manager, |model| {
                let node = node.clone();
                let session_for_execution = session_for_execution.clone();
                let agent_id = agent_id.clone();
                async move {
                    self.execute_session_with_profiles(
                        &node,
                        model,
                        &session_for_execution,
                        user_input,
                        primary_provider,
                        max_history,
                        input_mode,
                        Some(agent_id.as_str()),
                    )
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

    #[allow(clippy::too_many_arguments)]
    async fn execute_agent_with_client(
        &self,
        agent_node: &AgentNode,
        model: AIModel,
        llm_client: Arc<dyn LlmClient>,
        background_task_id: Option<&str>,
        input: Option<&str>,
        memory_config: &MemoryConfig,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
        factory: Arc<dyn LlmClientFactory>,
        agent_id: Option<&str>,
    ) -> Result<ExecutionResult> {
        let swappable = Arc::new(SwappableLlm::new(llm_client));
        let effective_tools = effective_main_agent_tool_names(agent_node.tools.as_deref());
        let python_runtime =
            resolve_python_runtime_policy(agent_node.python_runtime_policy.as_ref());
        let tools = self.build_tool_registry(
            Some(&effective_tools),
            swappable.clone(),
            swappable.clone(),
            factory,
            agent_id,
            python_runtime,
        );
        let system_prompt =
            self.build_background_system_prompt(agent_node, agent_id, background_task_id)?;
        let goal = input.unwrap_or("Execute the agent task");
        let mut config = ReActAgentConfig::new(goal.to_string())
            .with_system_prompt(system_prompt)
            .with_max_memory_messages(memory_config.max_messages)
            .with_context_window(Self::context_window_for_model(model));
        if let Some(compaction) = Self::build_compaction_config(memory_config) {
            config = config.with_compaction_config(compaction);
        }
        if model.supports_temperature()
            && let Some(temp) = agent_node.temperature
        {
            config = config.with_temperature(temp as f32);
        }

        let mut agent = ReActAgentExecutor::new(swappable, tools);
        if let Some(rx) = steer_rx {
            agent = agent.with_steer_channel(rx);
        }

        let result = if let Some(mut emitter) = emitter {
            #[allow(deprecated)]
            {
                agent.execute_streaming(config, emitter.as_mut()).await?
            }
        } else {
            agent.run(config).await?
        };
        if result.success {
            let compaction = result.compaction_results.iter().fold(
                super::runner::CompactionMetrics::default(),
                |mut acc, item| {
                    acc.event_count += 1;
                    acc.tokens_before += item.tokens_before;
                    acc.tokens_after += item.tokens_after;
                    acc.messages_compacted += item.compacted_count;
                    acc
                },
            );
            let messages = result.state.messages;
            let output = result.answer.unwrap_or_default();
            if compaction.event_count > 0 {
                Ok(ExecutionResult::success_with_compaction(
                    output, messages, compaction,
                ))
            } else {
                Ok(ExecutionResult::success(output, messages))
            }
        } else {
            Err(anyhow!(
                "Agent execution failed: {}",
                result.error.unwrap_or_else(|| "unknown error".to_string())
            ))
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_with_model(
        &self,
        agent_node: &AgentNode,
        model: AIModel,
        background_task_id: Option<&str>,
        input: Option<&str>,
        memory_config: &MemoryConfig,
        primary_provider: Provider,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
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

        let llm_client =
            Self::create_llm_client(factory.as_ref(), model, api_key.as_deref(), agent_node)?;
        self.execute_agent_with_client(
            agent_node,
            model,
            llm_client,
            background_task_id,
            input,
            memory_config,
            steer_rx,
            emitter,
            factory,
            agent_id,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_with_profiles(
        &self,
        agent_node: &AgentNode,
        model: AIModel,
        background_task_id: Option<&str>,
        input: Option<&str>,
        memory_config: &MemoryConfig,
        primary_provider: Provider,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
        agent_id: Option<&str>,
    ) -> Result<ExecutionResult> {
        if model.is_codex_cli() {
            return self
                .execute_with_model(
                    agent_node,
                    model,
                    background_task_id,
                    input,
                    memory_config,
                    primary_provider,
                    steer_rx,
                    emitter,
                    agent_id,
                )
                .await;
        }

        if agent_node.api_key_config.is_some() {
            return self
                .execute_with_model(
                    agent_node,
                    model,
                    background_task_id,
                    input,
                    memory_config,
                    primary_provider,
                    steer_rx,
                    emitter,
                    agent_id,
                )
                .await;
        }

        let profiles = self
            .auth_manager
            .get_compatible_profiles_for_model_provider(model.provider())
            .await;

        if profiles.is_empty() {
            return self
                .execute_with_model(
                    agent_node,
                    model,
                    background_task_id,
                    input,
                    memory_config,
                    primary_provider,
                    steer_rx,
                    emitter,
                    agent_id,
                )
                .await;
        }

        let mut last_error: Option<anyhow::Error> = None;
        let mut steer_rx = steer_rx;
        let mut emitter = emitter;

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
            let llm_client = Self::create_llm_client(
                factory.as_ref(),
                model,
                Some(api_key.as_str()),
                agent_node,
            )?;

            match self
                .execute_agent_with_client(
                    agent_node,
                    model,
                    llm_client,
                    background_task_id,
                    input,
                    memory_config,
                    steer_rx.take(),
                    emitter.take(),
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
impl AgentExecutor for AgentRuntimeExecutor {
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
        background_task_id: Option<&str>,
        input: Option<&str>,
        memory_config: &MemoryConfig,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
    ) -> Result<ExecutionResult> {
        self.execute_internal(
            agent_id,
            background_task_id,
            input,
            memory_config,
            steer_rx,
            None,
        )
        .await
    }

    async fn execute_with_emitter(
        &self,
        agent_id: &str,
        background_task_id: Option<&str>,
        input: Option<&str>,
        memory_config: &MemoryConfig,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
    ) -> Result<ExecutionResult> {
        self.execute_internal(
            agent_id,
            background_task_id,
            input,
            memory_config,
            steer_rx,
            emitter,
        )
        .await
    }
}

impl AgentRuntimeExecutor {
    async fn execute_internal(
        &self,
        agent_id: &str,
        background_task_id: Option<&str>,
        input: Option<&str>,
        memory_config: &MemoryConfig,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
    ) -> Result<ExecutionResult> {
        let stored_agent = self
            .storage
            .agents
            .get_agent(agent_id.to_string())?
            .ok_or_else(|| anyhow!("Agent '{}' not found", agent_id))?;

        let agent_node = stored_agent.agent.clone();
        let primary_model = self.resolve_primary_model(&agent_node).await?;
        let primary_provider = primary_model.provider();

        // steer_rx/emitter are one-shot resources and cannot be replayed safely across
        // failover or retry attempts. Execute a single primary-model attempt when either
        // channel is present to avoid dropping steering/streaming state mid-run.
        if steer_rx.is_some() || emitter.is_some() {
            return self
                .execute_with_profiles(
                    &agent_node,
                    primary_model,
                    background_task_id,
                    input,
                    memory_config,
                    primary_provider,
                    steer_rx,
                    emitter,
                    Some(agent_id),
                )
                .await;
        }

        let failover_config = self
            .build_failover_config(primary_model, agent_node.api_key_config.as_ref())
            .await;
        let failover_manager = FailoverManager::new(failover_config);
        let retry_config = RetryConfig::default();
        let mut retry_state = RetryState::new();
        let input_owned = input.map(|value| value.to_string());
        let mut steer_rx = steer_rx;
        let mut emitter = emitter;

        loop {
            let input_ref = input_owned.as_deref();
            let agent_node_clone = agent_node.clone();
            // Note: steer_rx is consumed on first execution attempt only.
            // Retries after this point won't have steering support.
            let result = execute_with_failover(&failover_manager, |model| {
                let node = agent_node_clone.clone();
                let steer_rx = steer_rx.take();
                let emitter = emitter.take();
                async move {
                    self.execute_with_profiles(
                        &node,
                        model,
                        background_task_id,
                        input_ref,
                        memory_config,
                        primary_provider,
                        steer_rx,
                        emitter,
                        Some(agent_id),
                    )
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
    use crate::auth::{AuthProvider, Credential, CredentialSource};
    use crate::models::{AgentNode, MemoryConfig};
    use crate::runtime::subagent::{AgentDefinitionRegistry, SubagentConfig, SubagentTracker};
    use restflow_ai::ReplySender;
    use std::future::Future;
    use std::pin::Pin;
    use tempfile::tempdir;
    use tokio::sync::mpsc;

    fn create_test_storage() -> (Arc<Storage>, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
        (Arc::new(storage), temp_dir)
    }

    fn create_test_executor(storage: Arc<Storage>) -> AgentRuntimeExecutor {
        let auth_manager = Arc::new(AuthProfileManager::new(Arc::new(storage.secrets.clone())));
        let (completion_tx, completion_rx) = mpsc::channel(10);
        let subagent_tracker = Arc::new(SubagentTracker::new(completion_tx, completion_rx));
        let subagent_definitions = Arc::new(AgentDefinitionRegistry::with_builtins());
        let subagent_config = SubagentConfig::default();
        AgentRuntimeExecutor::new(
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

    #[test]
    fn test_context_window_for_model() {
        assert_eq!(
            AgentRuntimeExecutor::context_window_for_model(AIModel::ClaudeSonnet4_5),
            200_000
        );
        assert_eq!(
            AgentRuntimeExecutor::context_window_for_model(AIModel::Gpt5),
            128_000
        );
        assert_eq!(
            AgentRuntimeExecutor::context_window_for_model(AIModel::DeepseekChat),
            64_000
        );
        assert_eq!(
            AgentRuntimeExecutor::context_window_for_model(AIModel::Gemini25Pro),
            1_000_000
        );
    }

    #[test]
    fn test_build_compaction_config_from_memory_config() {
        let enabled = MemoryConfig::default();
        let config = AgentRuntimeExecutor::build_compaction_config(&enabled)
            .expect("compaction should be enabled by default");
        assert_eq!(config.threshold_ratio, 0.80);
        assert_eq!(config.max_summary_tokens, 2_000);
        assert!(config.auto_compact);

        let disabled = MemoryConfig {
            enable_compaction: false,
            ..MemoryConfig::default()
        };
        assert!(AgentRuntimeExecutor::build_compaction_config(&disabled).is_none());
    }

    struct NoopReplySender;

    impl ReplySender for NoopReplySender {
        fn send(
            &self,
            _message: String,
        ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
            Box::pin(async { Ok(()) })
        }
    }

    #[test]
    fn test_filter_requested_tool_names_removes_reply_without_sender() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = create_test_executor(storage);
        let requested = vec!["bash".to_string(), "reply".to_string(), "file".to_string()];

        let filtered = executor
            .filter_requested_tool_names(Some(&requested))
            .expect("filtered tool list");

        assert!(filtered.iter().any(|name| name == "bash"));
        assert!(filtered.iter().any(|name| name == "file"));
        assert!(!filtered.iter().any(|name| name == "reply"));
    }

    #[test]
    fn test_filter_requested_tool_names_keeps_reply_with_sender() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = create_test_executor(storage).with_reply_sender(Arc::new(NoopReplySender));
        let requested = vec!["reply".to_string(), "bash".to_string()];

        let filtered = executor
            .filter_requested_tool_names(Some(&requested))
            .expect("filtered tool list");

        assert!(filtered.iter().any(|name| name == "reply"));
        assert!(filtered.iter().any(|name| name == "bash"));
    }

    #[tokio::test]
    async fn test_resolve_primary_model_prefers_explicit_model() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = create_test_executor(storage);
        let node = AgentNode::with_model(AIModel::ClaudeSonnet4_5);

        let resolved = executor.resolve_primary_model(&node).await.unwrap();
        assert_eq!(resolved, AIModel::ClaudeSonnet4_5);
    }

    #[tokio::test]
    async fn test_resolve_primary_model_uses_openai_secret_when_model_missing() {
        let (storage, _temp_dir) = create_test_storage();
        storage
            .secrets
            .set_secret("OPENAI_API_KEY", "test-openai-key", None)
            .unwrap();
        let executor = create_test_executor(storage);
        let node = AgentNode::new();

        let resolved = executor.resolve_primary_model(&node).await.unwrap();
        assert_eq!(resolved, AIModel::Gpt5);
    }

    #[tokio::test]
    async fn test_resolve_primary_model_uses_anthropic_opus_when_model_missing() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = create_test_executor(storage);
        executor
            .auth_manager
            .add_profile_from_credential(
                "anthropic-test",
                Credential::ApiKey {
                    key: "test-anthropic-key".to_string(),
                    email: None,
                },
                CredentialSource::Manual,
                AuthProvider::Anthropic,
            )
            .await
            .unwrap();
        let node = AgentNode::new();

        let resolved = executor.resolve_primary_model(&node).await.unwrap();
        assert_eq!(resolved, AIModel::ClaudeOpus4_6);
    }

    #[test]
    fn test_default_model_for_provider_uses_anthropic_opus() {
        assert_eq!(
            AgentRuntimeExecutor::default_model_for_provider(Provider::Anthropic),
            AIModel::ClaudeOpus4_6
        );
    }

    #[tokio::test]
    async fn test_executor_agent_not_found() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = create_test_executor(storage);

        let result = executor
            .execute(
                "nonexistent-agent",
                None,
                None,
                &MemoryConfig::default(),
                None,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_executor_no_api_key() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = create_test_executor(storage);
        let result = executor
            .resolve_api_key_for_model(
                Provider::Anthropic,
                Some(&ApiKeyConfig::Secret("MISSING_TEST_SECRET".to_string())),
                Provider::Anthropic,
            )
            .await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("MISSING_TEST_SECRET"),
            "Error should mention missing secret: {}",
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
