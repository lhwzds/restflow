//! Real agent executor implementation for the task runner.
//!
//! This module provides `AgentRuntimeExecutor`, which implements the
//! `AgentExecutor` trait by running the shared agent execution engine.
//! It loads agent configuration from storage, builds the appropriate LLM
//! client, and executes the agent with the configured tools.

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use crate::{
    AIModel, Provider,
    auth::{AuthProfileManager, AuthProvider},
    models::{
        AgentCheckpoint, AgentNode, ApiKeyConfig, ChatMessage, ChatRole, ChatSession,
        DurabilityMode, MemoryConfig, SharedEntry, Skill, SteerMessage, Visibility,
    },
    process::ProcessRegistry,
    prompt_files,
    services::skill_triggers::match_triggers,
    storage::Storage,
};
use restflow_ai::agent::{
    CheckpointDurability, ModelRoutingConfig as AiModelRoutingConfig,
    ModelSwitcher as AiModelSwitcher, StreamEmitter,
};
use restflow_ai::llm::Message;
use restflow_ai::tools::PythonRuntime;
use restflow_ai::{
    AgentConfig as ReActAgentConfig, AgentExecutor as ReActAgentExecutor, AiError, CodexClient,
    CompactionConfig, DefaultLlmClientFactory, LlmClient, LlmClientFactory, LlmProvider,
    ProcessTool, ReplySender, ReplyTool, ResourceLimits as AgentResourceLimits, Scratchpad,
    SwappableLlm, SwitchModelTool,
};
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{debug, info, warn};

use super::failover::{FailoverConfig, FailoverManager, execute_with_failover};
use super::model_catalog::ModelCatalog;
use super::preflight::{PreflightCategory, PreflightIssue, run_preflight};
use super::retry::{RetryConfig, RetryState};
use super::runner::{AgentExecutor, ExecutionResult};
use crate::runtime::agent::{
    BashConfig, SubagentDeps, ToolRegistry, build_agent_system_prompt,
    effective_main_agent_tool_names, registry_from_allowlist, resolve_python_runtime_policy,
    secret_resolver_from_storage,
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

const TOOL_RESULT_CONTEXT_RATIO: f64 = 0.08;
const TOOL_RESULT_MIN_CHARS: usize = 512;
const TOOL_RESULT_MAX_CHARS: usize = 24_000;
const TOOL_RESULT_CHARS_PER_TOKEN_ESTIMATE: usize = 4;

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

struct RuntimeModelSwitcher {
    swappable: Arc<SwappableLlm>,
    factory: Arc<dyn LlmClientFactory>,
    agent_node: AgentNode,
}

#[async_trait]
impl AiModelSwitcher for RuntimeModelSwitcher {
    fn current_model(&self) -> String {
        self.swappable.current_model()
    }

    async fn switch_model(&self, target_model: &str) -> std::result::Result<(), AiError> {
        let model = AIModel::from_api_name(target_model)
            .ok_or_else(|| AiError::Agent(format!("Unsupported routed model: {}", target_model)))?;
        let client = AgentRuntimeExecutor::create_llm_client(
            self.factory.as_ref(),
            model,
            None,
            &self.agent_node,
        )
        .map_err(|error| AiError::Agent(error.to_string()))?;
        self.swappable.swap(client);
        Ok(())
    }
}

impl AgentRuntimeExecutor {
    fn save_task_deliverable(&self, task_id: &str, agent_id: &str, output: &str) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        let key = format!("deliverable:{task_id}");
        let payload = serde_json::json!({
            "agent_id": agent_id,
            "parts": [
                {
                    "type": "text",
                    "content": output,
                }
            ],
            "completed_at": Utc::now().to_rfc3339(),
        });
        let payload = serde_json::to_string(&payload)?;
        let created_at = self
            .storage
            .shared_space
            .get_unchecked(&key)?
            .map(|entry| entry.created_at)
            .unwrap_or(now);
        let entry = SharedEntry {
            key,
            value: payload,
            visibility: Visibility::Shared,
            owner: Some(agent_id.to_string()),
            content_type: Some("application/json".to_string()),
            type_hint: Some("deliverable".to_string()),
            tags: vec!["deliverable".to_string()],
            created_at,
            updated_at: now,
            last_modified_by: Some(agent_id.to_string()),
        };
        self.storage.shared_space.set(&entry)
    }

    fn validate_prerequisites(&self, prerequisites: &[String]) -> Result<()> {
        if prerequisites.is_empty() {
            return Ok(());
        }

        let mut failed = Vec::new();
        for task_id in prerequisites {
            let key = format!("deliverable:{task_id}");
            match self.storage.shared_space.quick_get(&key, None) {
                Ok(Some(raw)) => match serde_json::from_str::<serde_json::Value>(&raw) {
                    Ok(value) => {
                        let parts = value.get("parts").and_then(|part| part.as_array());
                        if parts.is_none_or(|items| items.is_empty()) {
                            failed.push(format!("{task_id} (empty deliverable)"));
                        }
                    }
                    Err(_) => failed.push(format!("{task_id} (invalid JSON)")),
                },
                Ok(None) => failed.push(format!("{task_id} (not found)")),
                Err(error) => failed.push(format!("{task_id} ({error})")),
            }
        }

        if failed.is_empty() {
            Ok(())
        } else {
            Err(anyhow!("Prerequisites not met: {}", failed.join(", ")))
        }
    }

    fn persist_deliverable_if_needed(
        &self,
        background_task_id: Option<&str>,
        agent_id: &str,
        output: &str,
    ) -> Result<()> {
        if let Some(task_id) = background_task_id {
            self.save_task_deliverable(task_id, agent_id, output)?;
        }
        Ok(())
    }

    fn create_scratchpad_for_task(background_task_id: &str) -> Result<Arc<Scratchpad>> {
        let base_dir = crate::paths::ensure_restflow_dir()?.join("scratchpads");
        std::fs::create_dir_all(&base_dir)?;
        let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
        let path = base_dir.join(format!("{background_task_id}-{timestamp}.jsonl"));
        Ok(Arc::new(Scratchpad::new(path)?))
    }

    fn to_ai_model_routing_config(
        config: &crate::models::ModelRoutingConfig,
    ) -> AiModelRoutingConfig {
        AiModelRoutingConfig {
            enabled: config.enabled,
            routine_model: config.routine_model.clone(),
            moderate_model: config.moderate_model.clone(),
            complex_model: config.complex_model.clone(),
            escalate_on_failure: config.escalate_on_failure,
        }
    }

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
            Provider::Zai => AIModel::Glm5,
            Provider::ZaiCodingPlan => AIModel::Glm5CodingPlan,
            Provider::Moonshot => AIModel::KimiK2_5,
            Provider::Doubao => AIModel::DoubaoPro,
            Provider::Yi => AIModel::YiLightning,
            Provider::SiliconFlow => AIModel::SiliconFlowAuto,
            Provider::MiniMax => AIModel::MiniMaxM25,
            Provider::MiniMaxCodingPlan => AIModel::MiniMaxM25CodingPlan,
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

    fn to_agent_resource_limits(limits: &crate::models::ResourceLimits) -> AgentResourceLimits {
        AgentResourceLimits {
            max_tool_calls: limits.max_tool_calls,
            max_wall_clock: Duration::from_secs(limits.max_duration_secs),
            max_depth: AgentResourceLimits::default().max_depth,
            max_cost_usd: limits.max_cost_usd,
        }
    }

    fn effective_max_tool_result_length(
        requested_max_output_bytes: usize,
        context_window: usize,
    ) -> usize {
        let requested = requested_max_output_bytes.max(1);
        let context_token_budget =
            ((context_window as f64) * TOOL_RESULT_CONTEXT_RATIO).round() as usize;
        let context_char_budget =
            context_token_budget.saturating_mul(TOOL_RESULT_CHARS_PER_TOKEN_ESTIMATE);
        let context_cap = context_char_budget.clamp(TOOL_RESULT_MIN_CHARS, TOOL_RESULT_MAX_CHARS);
        requested.min(context_cap)
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
        // Prefer coding-plan providers before regular providers when both exist.
        const SECRET_PROVIDER_ORDER: [Provider; 16] = [
            Provider::MiniMaxCodingPlan,
            Provider::MiniMax,
            Provider::ZaiCodingPlan,
            Provider::Zai,
            Provider::Anthropic,
            Provider::OpenAI,
            Provider::Google,
            Provider::DeepSeek,
            Provider::Groq,
            Provider::OpenRouter,
            Provider::XAI,
            Provider::Qwen,
            Provider::Moonshot,
            Provider::Doubao,
            Provider::Yi,
            Provider::SiliconFlow,
        ];

        for provider in SECRET_PROVIDER_ORDER {
            if self.has_non_empty_secret(provider.api_key_env())? {
                return Ok(Some(Self::default_model_for_provider(provider)));
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

        // Get manually configured fallback models from config
        let config = self.storage.config.get_config().ok().flatten();
        let fallback_models: Option<Vec<AIModel>> = config
            .as_ref()
            .and_then(|c| c.agent.fallback_models.clone())
            .map(|models| {
                models
                    .iter()
                    .filter_map(|s| AIModel::from_api_name(s))
                    .collect()
            });

        let config = FailoverConfig::build_smart(primary, &available_providers, fallback_models);

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
        user_input: Option<&str>,
    ) -> Result<String> {
        let mut prompt_agent = agent_node.clone();

        // SECURITY: Build allowed skill set from agent's assigned skills
        let allowed_skills: HashSet<String> = agent_node
            .skills
            .as_ref()
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default();

        if let Some(input) = user_input.map(str::trim).filter(|value| !value.is_empty()) {
            let triggered_skill_ids = self.resolve_triggered_skill_ids(input)?;

            // SECURITY: Only allow triggered skills that are in agent's skill list
            // This prevents capability scope expansion via crafted input
            let allowed_triggered: Vec<String> = triggered_skill_ids
                .into_iter()
                .filter(|skill_id| allowed_skills.contains(skill_id))
                .collect();

            if !allowed_triggered.is_empty() {
                let mut effective_skills = prompt_agent.skills.clone().unwrap_or_default();
                for skill_id in allowed_triggered {
                    if !effective_skills
                        .iter()
                        .any(|existing| existing == &skill_id)
                    {
                        effective_skills.push(skill_id);
                    }
                }
                prompt_agent.skills = Some(effective_skills);
            }
        }

        let base_prompt = build_agent_system_prompt(self.storage.clone(), &prompt_agent, agent_id)?;
        let policy_prompt = prompt_files::load_background_agent_policy(background_task_id)?;
        if policy_prompt.trim().is_empty() {
            return Ok(base_prompt);
        }
        Ok(format!("{base_prompt}\n\n{policy_prompt}"))
    }

    fn resolve_triggered_skill_ids(&self, user_input: &str) -> Result<Vec<String>> {
        let skills = self.storage.skills.list()?;
        let matches = match_triggers(user_input, &skills);
        Ok(matches
            .into_iter()
            .map(|matched| matched.skill_id)
            .collect())
    }

    fn resolve_preflight_skills(
        &self,
        agent_node: &AgentNode,
        user_input: Option<&str>,
    ) -> Result<Vec<Skill>> {
        // SECURITY: Start with only agent's assigned skills
        let mut skill_ids = agent_node.skills.clone().unwrap_or_default();

        // SECURITY: Build allowed skill set from agent's assigned skills
        let allowed_skills: HashSet<String> = agent_node
            .skills
            .as_ref()
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default();

        if let Some(input) = user_input.map(str::trim).filter(|value| !value.is_empty()) {
            let triggered_skill_ids = self.resolve_triggered_skill_ids(input)?;
            // SECURITY: Only allow triggered skills that are in agent's skill list
            for skill_id in triggered_skill_ids {
                if allowed_skills.contains(&skill_id)
                    && !skill_ids.iter().any(|existing| existing == &skill_id)
                {
                    skill_ids.push(skill_id);
                }
            }
        }

        let mut skills = Vec::new();
        for skill_id in skill_ids {
            match self.storage.skills.get(&skill_id)? {
                Some(skill) => skills.push(skill),
                None => {
                    warn!(skill_id = %skill_id, "Skill referenced by agent not found during preflight")
                }
            }
        }
        Ok(skills)
    }

    async fn run_preflight_check(
        &self,
        agent_node: &AgentNode,
        primary_model: AIModel,
        primary_provider: Provider,
        user_input: Option<&str>,
    ) -> Result<()> {
        let skills = self.resolve_preflight_skills(agent_node, user_input)?;
        let available_tools = effective_main_agent_tool_names(agent_node.tools.as_deref());
        let mut preflight = run_preflight(
            &skills,
            &available_tools,
            agent_node.skill_variables.as_ref(),
            true,
        );

        if !primary_model.is_codex_cli()
            && !primary_model.is_gemini_cli()
            && let Err(error) = self
                .resolve_api_key_for_model(
                    primary_provider,
                    agent_node.api_key_config.as_ref(),
                    primary_provider,
                )
                .await
        {
            preflight.blockers.push(PreflightIssue {
                category: PreflightCategory::MissingSecret,
                message: error.to_string(),
                suggestion: Some("Configure API key via auth profile or secrets".to_string()),
            });
            preflight.passed = false;
        }

        for warning_issue in &preflight.warnings {
            warn!(
                category = warning_issue.category.as_str(),
                message = %warning_issue.message,
                suggestion = ?warning_issue.suggestion,
                "Background agent preflight warning"
            );
        }

        if !preflight.passed {
            let blocker_message = preflight
                .blockers
                .iter()
                .map(|issue| format!("- [{}] {}", issue.category.as_str(), issue.message))
                .collect::<Vec<_>>()
                .join("\n");
            return Err(anyhow!("Preflight check failed:\n{}", blocker_message));
        }

        Ok(())
    }

    /// Build the tool registry for an agent.
    ///
    /// If the agent has specific tools configured, only those tools are registered.
    /// Otherwise, an empty registry is used (secure default).
    #[allow(clippy::too_many_arguments)]
    fn build_tool_registry(
        &self,
        tool_names: Option<&[String]>,
        llm_client: Arc<dyn LlmClient>,
        swappable: Arc<SwappableLlm>,
        factory: Arc<dyn LlmClientFactory>,
        agent_id: Option<&str>,
        python_runtime: PythonRuntime,
        bash_config: Option<BashConfig>,
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
            bash_config.clone(),
        ));
        let subagent_deps = self.build_subagent_deps(llm_client, subagent_tool_registry);
        let mut registry = registry_from_allowlist(
            filtered_tool_names_ref,
            Some(&subagent_deps),
            secret_resolver,
            Some(self.storage.as_ref()),
            agent_id,
            Some(python_runtime),
            bash_config,
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

    fn session_history_messages(
        session: &ChatSession,
        max_messages: usize,
        input_mode: SessionInputMode,
    ) -> Vec<Message> {
        let mut messages = Self::session_messages_for_context(session);
        if messages.is_empty() {
            return Vec::new();
        }

        // Exclude the latest user input because it will be passed to execute()
        // separately for persisted-input flows.
        if input_mode == SessionInputMode::PersistedInSession
            && matches!(messages.last().map(|m| &m.role), Some(ChatRole::User))
        {
            messages.pop();
        }

        let start = messages.len().saturating_sub(max_messages);
        messages[start..]
            .iter()
            .map(Self::chat_message_to_llm_message)
            .collect()
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
        let agent_defaults = self
            .storage
            .config
            .get_config()
            .ok()
            .flatten()
            .map(|c| c.agent)
            .unwrap_or_default();
        let bash_config = BashConfig {
            timeout_secs: agent_defaults.bash_timeout_secs,
            ..BashConfig::default()
        };
        let tools = self.build_tool_registry(
            Some(&effective_tools),
            swappable.clone(),
            swappable.clone(),
            factory.clone(),
            agent_id,
            python_runtime,
            Some(bash_config),
        );
        let system_prompt = build_agent_system_prompt(self.storage.clone(), agent_node, agent_id)?;

        let catalog = ModelCatalog::global().await;
        let model_entry = catalog.resolve(model).await;
        let context_window = model_entry
            .map(|entry| {
                entry
                    .capabilities
                    .input_limit
                    .unwrap_or(entry.capabilities.context_window)
            })
            .unwrap_or_else(|| Self::context_window_for_model(model));
        let max_tool_result_length = Self::effective_max_tool_result_length(4_000, context_window);

        let mut config = ReActAgentConfig::new(user_input.to_string())
            .with_system_prompt(system_prompt.clone())
            .with_tool_timeout(Duration::from_secs(agent_defaults.tool_timeout_secs))
            .with_max_iterations(agent_defaults.max_iterations)
            .with_max_memory_messages(max_history.max(1))
            .with_context_window(context_window)
            .with_max_tool_result_length(max_tool_result_length);
        if let Some(entry) = model_entry
            && !model.is_cli_model()
        {
            config = config.with_max_output_tokens(entry.capabilities.output_limit as u32);
        }
        if model.supports_temperature()
            && let Some(temp) = agent_node.temperature
        {
            config = config.with_temperature(temp as f32);
        }

        let agent = ReActAgentExecutor::new(swappable.clone(), tools);
        let history_messages = Self::session_history_messages(session, max_history, input_mode);
        let result = if history_messages.is_empty() {
            agent.run(config).await?
        } else {
            let mut state = restflow_ai::AgentState::new(
                uuid::Uuid::new_v4().to_string(),
                agent_defaults.max_iterations,
            );
            state.add_message(Message::system(system_prompt));
            for message in history_messages {
                state.add_message(message);
            }
            state.add_message(Message::user(user_input.to_string()));
            agent.run_from_state(config, state).await?
        };
        if !result.success {
            return Err(anyhow!(
                "Agent execution failed: {}",
                result.error.unwrap_or_else(|| "unknown error".to_string())
            ));
        }

        Ok(SessionExecutionResult {
            output: result.answer.unwrap_or_default(),
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
        resource_limits: &crate::models::ResourceLimits,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
        factory: Arc<dyn LlmClientFactory>,
        agent_id: Option<&str>,
        initial_state: Option<restflow_ai::AgentState>,
    ) -> Result<ExecutionResult> {
        // Load agent execution defaults from system config (runtime-configurable).
        let agent_defaults = self
            .storage
            .config
            .get_config()
            .ok()
            .flatten()
            .map(|c| c.agent)
            .unwrap_or_default();

        let swappable = Arc::new(SwappableLlm::new(llm_client));
        let effective_tools = effective_main_agent_tool_names(agent_node.tools.as_deref());
        let python_runtime =
            resolve_python_runtime_policy(agent_node.python_runtime_policy.as_ref());
        let bash_config = BashConfig {
            timeout_secs: agent_defaults.bash_timeout_secs,
            ..BashConfig::default()
        };
        let tools = self.build_tool_registry(
            Some(&effective_tools),
            swappable.clone(),
            swappable.clone(),
            factory.clone(),
            agent_id,
            python_runtime,
            Some(bash_config),
        );
        let system_prompt =
            self.build_background_system_prompt(agent_node, agent_id, background_task_id, input)?;
        let goal = input.unwrap_or("Execute the agent task");
        let catalog = ModelCatalog::global().await;
        let model_entry = catalog.resolve(model).await;
        let context_window = model_entry
            .map(|entry| {
                entry
                    .capabilities
                    .input_limit
                    .unwrap_or(entry.capabilities.context_window)
            })
            .unwrap_or_else(|| Self::context_window_for_model(model));
        let max_tool_result_length = Self::effective_max_tool_result_length(
            resource_limits.max_output_bytes,
            context_window,
        );
        if max_tool_result_length < resource_limits.max_output_bytes {
            debug!(
                model = ?model,
                requested_max_output_bytes = resource_limits.max_output_bytes,
                context_window,
                clamped_max_tool_result_length = max_tool_result_length,
                "Clamped max tool result length based on context window"
            );
        }

        let mut config = ReActAgentConfig::new(goal.to_string())
            .with_system_prompt(system_prompt)
            .with_tool_timeout(Duration::from_secs(agent_defaults.tool_timeout_secs))
            .with_max_iterations(agent_defaults.max_iterations)
            .with_max_memory_messages(memory_config.max_messages)
            .with_context_window(context_window)
            .with_resource_limits(Self::to_agent_resource_limits(resource_limits))
            .with_max_tool_result_length(max_tool_result_length)
            .with_yolo_mode(background_task_id.is_some());
        if let Some(entry) = model_entry
            && !model.is_cli_model()
        {
            config = config.with_max_output_tokens(entry.capabilities.output_limit as u32);
        }
        if let Some(compaction) = Self::build_compaction_config(memory_config) {
            config = config.with_compaction_config(compaction);
        }
        if let Some(task_id) = background_task_id
            && let Ok(scratchpad) = Self::create_scratchpad_for_task(task_id)
        {
            config = config.with_scratchpad(scratchpad);
        }
        if model.supports_temperature()
            && let Some(temp) = agent_node.temperature
        {
            config = config.with_temperature(temp as f32);
        }
        if let Some(model_routing) = agent_node.model_routing.as_ref() {
            config = config.with_model_routing(Self::to_ai_model_routing_config(model_routing));
            if model_routing.enabled {
                let switcher: Arc<dyn AiModelSwitcher> = Arc::new(RuntimeModelSwitcher {
                    swappable: swappable.clone(),
                    factory: factory.clone(),
                    agent_node: agent_node.clone(),
                });
                config = config.with_model_switcher(switcher);
            }
        }
        if let Some(task_id) = background_task_id
            && let Ok(Some(task)) = self.storage.background_agents.get_task(task_id)
        {
            let checkpoint_durability = match task.durability_mode {
                DurabilityMode::Sync => CheckpointDurability::PerTurn,
                DurabilityMode::Async => CheckpointDurability::Periodic { interval: 5 },
                DurabilityMode::Exit => CheckpointDurability::OnComplete,
            };
            config = config.with_checkpoint_durability(checkpoint_durability);

            let checkpoints = self.storage.background_agents.clone();
            let task_id_owned = task.id.clone();
            config = config.with_checkpoint_callback(move |state| {
                let checkpoints = checkpoints.clone();
                let task_id = task_id_owned.clone();
                let state = state.clone();
                async move {
                    let state_json = serde_json::to_vec(&state)
                        .map_err(|e| AiError::Agent(format!("Failed to encode state: {e}")))?;
                    let mut checkpoint = AgentCheckpoint::new(
                        state.execution_id.clone(),
                        Some(task_id),
                        state.version,
                        state.iteration,
                        state_json,
                        "periodic_checkpoint".to_string(),
                    );
                    // Atomic checkpoint + savepoint: first save with savepoint (no savepoint_id in data),
                    // then re-save with savepoint_id embedded to close the race window.
                    let savepoint_id = checkpoints
                        .save_checkpoint_with_savepoint(&checkpoint)
                        .map_err(|e| {
                            AiError::Agent(format!("Failed to save checkpoint with savepoint: {e}"))
                        })?;
                    checkpoint.savepoint_id = Some(savepoint_id);
                    checkpoints
                        .save_checkpoint_with_savepoint_id(&checkpoint)
                        .map_err(|e| {
                            AiError::Agent(format!(
                                "Failed to persist checkpoint with savepoint id: {e}"
                            ))
                        })?;
                    Ok(())
                }
            });
        }

        let mut agent = ReActAgentExecutor::new(swappable, tools);
        if let Some(rx) = steer_rx {
            agent = agent.with_steer_channel(rx);
        }

        let force_non_stream = model.is_codex_cli();

        let result = if let Some(state) = initial_state {
            if force_non_stream {
                agent.run_from_state(config, state).await?
            } else if let Some(mut emitter) = emitter {
                agent
                    .execute_from_state(config, state, emitter.as_mut())
                    .await?
            } else {
                agent.run_from_state(config, state).await?
            }
        } else if force_non_stream {
            agent.run(config).await?
        } else if let Some(mut emitter) = emitter {
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
        resource_limits: &crate::models::ResourceLimits,
        primary_provider: Provider,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
        agent_id: Option<&str>,
        initial_state: Option<restflow_ai::AgentState>,
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
            resource_limits,
            steer_rx,
            emitter,
            factory,
            agent_id,
            initial_state,
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
        resource_limits: &crate::models::ResourceLimits,
        primary_provider: Provider,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
        agent_id: Option<&str>,
        initial_state: Option<restflow_ai::AgentState>,
    ) -> Result<ExecutionResult> {
        if model.is_codex_cli() {
            return self
                .execute_with_model(
                    agent_node,
                    model,
                    background_task_id,
                    input,
                    memory_config,
                    resource_limits,
                    primary_provider,
                    steer_rx,
                    emitter,
                    agent_id,
                    initial_state,
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
                    resource_limits,
                    primary_provider,
                    steer_rx,
                    emitter,
                    agent_id,
                    initial_state,
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
                    resource_limits,
                    primary_provider,
                    steer_rx,
                    emitter,
                    agent_id,
                    initial_state,
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
                    resource_limits,
                    steer_rx.take(),
                    emitter.take(),
                    factory,
                    agent_id,
                    initial_state.clone(),
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

    async fn execute_from_state(
        &self,
        agent_id: &str,
        background_task_id: Option<&str>,
        state: restflow_ai::AgentState,
        memory_config: &MemoryConfig,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
    ) -> Result<ExecutionResult> {
        self.execute_internal_from_state(
            agent_id,
            background_task_id,
            state,
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
        // Fail closed on storage errors - do not silently swallow DB failures.
        let background_task = match background_task_id {
            Some(task_id) => match self.storage.background_agents.get_task(task_id) {
                Ok(task_opt) => task_opt,
                Err(e) => {
                    warn!(task_id, error = %e, "Failed to load background task");
                    return Err(e);
                }
            },
            None => None,
        };
        if let Some(task) = background_task.as_ref() {
            self.validate_prerequisites(&task.prerequisites)?;
        }
        let resolved_resource_limits = background_task
            .as_ref()
            .map(|task| task.resource_limits.clone())
            .unwrap_or_default();

        let agent_node = stored_agent.agent.clone();
        let primary_model = self.resolve_primary_model(&agent_node).await?;
        let primary_provider = primary_model.provider();
        self.run_preflight_check(&agent_node, primary_model, primary_provider, input)
            .await?;

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
                let limits = resolved_resource_limits.clone();
                async move {
                    self.execute_with_profiles(
                        &node,
                        model,
                        background_task_id,
                        input_ref,
                        memory_config,
                        &limits,
                        primary_provider,
                        steer_rx,
                        emitter,
                        Some(agent_id),
                        None,
                    )
                    .await
                }
            })
            .await;

            match result {
                Ok((exec_result, _model)) => {
                    self.persist_deliverable_if_needed(
                        background_task_id,
                        agent_id,
                        &exec_result.output,
                    )?;
                    return Ok(exec_result);
                }
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

    async fn execute_internal_from_state(
        &self,
        agent_id: &str,
        background_task_id: Option<&str>,
        state: restflow_ai::AgentState,
        memory_config: &MemoryConfig,
        steer_rx: Option<mpsc::Receiver<SteerMessage>>,
        emitter: Option<Box<dyn StreamEmitter>>,
    ) -> Result<ExecutionResult> {
        let stored_agent = self
            .storage
            .agents
            .get_agent(agent_id.to_string())?
            .ok_or_else(|| anyhow!("Agent '{}' not found", agent_id))?;
        // Fail closed on storage errors - do not silently swallow DB failures.
        let resolved_resource_limits = match background_task_id {
            Some(task_id) => match self.storage.background_agents.get_task(task_id) {
                Ok(Some(task)) => task.resource_limits,
                Ok(None) => {
                    warn!(task_id, "Background task not found, using default limits");
                    Default::default()
                }
                Err(e) => return Err(e),
            },
            None => Default::default(),
        };

        let agent_node = stored_agent.agent.clone();
        let primary_model = self.resolve_primary_model(&agent_node).await?;
        let primary_provider = primary_model.provider();

        self.execute_with_profiles(
            &agent_node,
            primary_model,
            background_task_id,
            None,
            memory_config,
            &resolved_resource_limits,
            primary_provider,
            steer_rx,
            emitter,
            Some(agent_id),
            Some(state),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AuthProvider, Credential, CredentialSource};
    use crate::models::{AgentNode, MemoryConfig, SharedEntry, Skill, Visibility};
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

    fn create_trigger_skill(id: &str, trigger: &str, content: &str) -> Skill {
        let mut skill = Skill::new(
            id.to_string(),
            "Trigger Skill".to_string(),
            Some("triggered skill".to_string()),
            None,
            content.to_string(),
        );
        skill.triggers = vec![trigger.to_string()];
        skill
    }

    fn insert_shared_entry(storage: &Storage, key: &str, value: &str) {
        let now = Utc::now().timestamp_millis();
        let entry = SharedEntry {
            key: key.to_string(),
            value: value.to_string(),
            visibility: Visibility::Public,
            owner: None,
            content_type: Some("application/json".to_string()),
            type_hint: Some("deliverable".to_string()),
            tags: vec!["deliverable".to_string()],
            created_at: now,
            updated_at: now,
            last_modified_by: Some("test".to_string()),
        };
        storage.shared_space.set(&entry).unwrap();
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

    #[test]
    fn test_to_agent_resource_limits_maps_cost_budget() {
        let limits = crate::models::ResourceLimits {
            max_tool_calls: 12,
            max_duration_secs: 34,
            max_output_bytes: 56,
            max_cost_usd: Some(7.5),
        };
        let mapped = AgentRuntimeExecutor::to_agent_resource_limits(&limits);
        assert_eq!(mapped.max_tool_calls, 12);
        assert_eq!(mapped.max_wall_clock, Duration::from_secs(34));
        assert_eq!(mapped.max_cost_usd, Some(7.5));
    }

    #[test]
    fn test_effective_max_tool_result_length_respects_small_requested_limit() {
        let value = AgentRuntimeExecutor::effective_max_tool_result_length(300, 128_000);
        assert_eq!(value, 300);
    }

    #[test]
    fn test_effective_max_tool_result_length_clamps_large_requested_limit() {
        let value = AgentRuntimeExecutor::effective_max_tool_result_length(1_000_000, 128_000);
        assert_eq!(value, TOOL_RESULT_MAX_CHARS);
    }

    #[test]
    fn test_effective_max_tool_result_length_for_small_context_window() {
        let value = AgentRuntimeExecutor::effective_max_tool_result_length(1_000_000, 2013);
        assert_eq!(value, 644);
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

    #[test]
    fn test_build_background_system_prompt_includes_triggered_skill() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = create_test_executor(storage.clone());
        let skill = create_trigger_skill("triggered-skill", "code review", "Triggered Content");
        storage.skills.create(&skill).unwrap();

        // SECURITY: Agent must have the skill in its skill list for triggers to work
        let node = AgentNode {
            prompt: Some("Base Prompt".to_string()),
            skills: Some(vec!["triggered-skill".to_string()]),
            ..AgentNode::new()
        };
        let prompt = executor
            .build_background_system_prompt(&node, None, None, Some("please do code review"))
            .unwrap();

        assert!(prompt.contains("Base Prompt"));
        assert!(prompt.contains("Triggered Content"));
    }

    #[test]
    fn test_build_background_system_prompt_skips_non_matching_skill() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = create_test_executor(storage.clone());
        let skill = create_trigger_skill("triggered-skill", "deploy release", "Triggered Content");
        storage.skills.create(&skill).unwrap();

        let node = AgentNode {
            prompt: Some("Base Prompt".to_string()),
            ..AgentNode::new()
        };
        let prompt = executor
            .build_background_system_prompt(&node, None, None, Some("review this patch"))
            .unwrap();

        assert!(prompt.contains("Base Prompt"));
        assert!(!prompt.contains("Triggered Content"));
    }

    /// SECURITY TEST: Triggered skills NOT in agent's skill list must be ignored
    /// to prevent capability scope expansion via crafted input
    #[test]
    fn test_build_background_system_prompt_ignores_unauthorized_triggered_skill() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = create_test_executor(storage.clone());

        // Create a privileged skill with trigger
        let privileged_skill =
            create_trigger_skill("privileged-skill", "admin", "Privileged Content");
        storage.skills.create(&privileged_skill).unwrap();

        // Agent does NOT have the privileged skill in its skill list
        let node = AgentNode {
            prompt: Some("Base Prompt".to_string()),
            skills: Some(vec!["regular-skill".to_string()]),
            ..AgentNode::new()
        };

        // User input triggers the privileged skill
        let prompt = executor
            .build_background_system_prompt(&node, None, None, Some("please do admin"))
            .unwrap();

        assert!(prompt.contains("Base Prompt"));
        // SECURITY: Privileged skill content must NOT be included
        assert!(!prompt.contains("Privileged Content"));
    }

    /// Test that triggered skills that ARE in agent's skill list are included
    #[test]
    fn test_build_background_system_prompt_includes_authorized_triggered_skill() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = create_test_executor(storage.clone());

        // Create a skill with trigger
        let skill = create_trigger_skill("authorized-skill", "code review", "Authorized Content");
        storage.skills.create(&skill).unwrap();

        // Agent HAS the skill in its skill list
        let node = AgentNode {
            prompt: Some("Base Prompt".to_string()),
            skills: Some(vec!["authorized-skill".to_string()]),
            ..AgentNode::new()
        };

        let prompt = executor
            .build_background_system_prompt(&node, None, None, Some("please do code review"))
            .unwrap();

        assert!(prompt.contains("Base Prompt"));
        assert!(prompt.contains("Authorized Content"));
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

    #[tokio::test]
    async fn test_resolve_api_key_requires_matching_zai_secret() {
        let (storage, _temp_dir) = create_test_storage();
        storage
            .secrets
            .set_secret("ZAI_CODING_PLAN_API_KEY", "zai-coding-plan-key", None)
            .unwrap();
        let executor = create_test_executor(storage);

        let result = executor
            .resolve_api_key_for_model(Provider::Zai, None, Provider::Zai)
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_resolve_api_key_requires_matching_zai_coding_plan_secret() {
        let (storage, _temp_dir) = create_test_storage();
        storage
            .secrets
            .set_secret("ZAI_API_KEY", "zai-key", None)
            .unwrap();
        let executor = create_test_executor(storage);

        let result = executor
            .resolve_api_key_for_model(Provider::ZaiCodingPlan, None, Provider::ZaiCodingPlan)
            .await;

        assert!(result.is_err());
    }

    #[test]
    fn test_validate_prerequisites_passes_with_valid_deliverables() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = create_test_executor(storage.clone());
        insert_shared_entry(
            &storage,
            "deliverable:task-a",
            r#"{"parts":[{"type":"text","content":"ok"}]}"#,
        );
        insert_shared_entry(
            &storage,
            "deliverable:task-b",
            r#"{"parts":[{"type":"text","content":"done"}]}"#,
        );

        let prerequisites = vec!["task-a".to_string(), "task-b".to_string()];
        let result = executor.validate_prerequisites(&prerequisites);
        assert!(result.is_ok(), "validation should pass: {:?}", result.err());
    }

    #[test]
    fn test_validate_prerequisites_fails_when_missing() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = create_test_executor(storage);
        let prerequisites = vec!["missing-task".to_string()];

        let err = executor
            .validate_prerequisites(&prerequisites)
            .expect_err("validation should fail");
        assert!(err.to_string().contains("missing-task (not found)"));
    }

    #[test]
    fn test_validate_prerequisites_fails_on_empty_parts() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = create_test_executor(storage.clone());
        insert_shared_entry(&storage, "deliverable:task-empty", r#"{"parts":[]}"#);
        let prerequisites = vec!["task-empty".to_string()];

        let err = executor
            .validate_prerequisites(&prerequisites)
            .expect_err("validation should fail");
        assert!(err.to_string().contains("task-empty (empty deliverable)"));
    }

    #[test]
    fn test_validate_prerequisites_fails_on_invalid_json() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = create_test_executor(storage.clone());
        insert_shared_entry(&storage, "deliverable:task-invalid", "not-json");
        let prerequisites = vec!["task-invalid".to_string()];

        let err = executor
            .validate_prerequisites(&prerequisites)
            .expect_err("validation should fail");
        assert!(err.to_string().contains("task-invalid (invalid JSON)"));
    }

    #[test]
    fn test_save_task_deliverable_persists_structured_payload() {
        let (storage, _temp_dir) = create_test_storage();
        let executor = create_test_executor(storage.clone());

        executor
            .save_task_deliverable("task-save", "agent-1", "final answer")
            .expect("save deliverable should succeed");

        let entry = storage
            .shared_space
            .get_unchecked("deliverable:task-save")
            .expect("shared space read should succeed")
            .expect("deliverable entry should exist");
        assert_eq!(entry.type_hint.as_deref(), Some("deliverable"));
        assert_eq!(entry.owner.as_deref(), Some("agent-1"));

        let payload: serde_json::Value =
            serde_json::from_str(&entry.value).expect("payload should be valid json");
        assert_eq!(payload["agent_id"].as_str(), Some("agent-1"));
        let parts = payload["parts"]
            .as_array()
            .expect("parts should be an array");
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0]["content"].as_str(), Some("final answer"));
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
