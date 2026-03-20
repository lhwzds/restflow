use super::*;

impl AgentRuntimeExecutor {
    pub(super) async fn resolve_api_key(
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
        let Some(secret_name) = provider.api_key_env() else {
            return Err(anyhow!(
                "No API key fallback is defined for provider {:?}. Please configure a compatible auth profile.",
                provider
            ));
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
    pub(super) async fn resolve_api_key_for_model(
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

    pub(super) fn default_model_for_provider(provider: Provider) -> AIModel {
        match provider {
            Provider::OpenAI => AIModel::Gpt5,
            Provider::Anthropic => AIModel::ClaudeOpus4_6,
            Provider::ClaudeCode => AIModel::ClaudeCodeOpus,
            Provider::Codex => AIModel::CodexCli,
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
            Provider::MiniMax => AIModel::MiniMaxM27,
            Provider::MiniMaxCodingPlan => AIModel::MiniMaxM25CodingPlan,
        }
    }

    pub(super) fn context_window_for_model(model: AIModel) -> usize {
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

    pub(super) fn has_non_empty_secret(&self, name: &str) -> Result<bool> {
        Ok(self.storage.secrets.get_non_empty(name)?.is_some())
    }

    pub(super) async fn resolve_model_from_stored_credentials(&self) -> Result<Option<AIModel>> {
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
            let Some(secret_name) = provider.api_key_env() else {
                continue;
            };
            if self.has_non_empty_secret(secret_name)? {
                return Ok(Some(Self::default_model_for_provider(provider)));
            }
        }

        Ok(None)
    }

    pub(super) async fn resolve_primary_model(&self, agent_node: &AgentNode) -> Result<AIModel> {
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

    pub(super) async fn build_api_keys(
        &self,
        agent_api_key_config: Option<&ApiKeyConfig>,
        primary_provider: Provider,
    ) -> HashMap<LlmProvider, String> {
        let mut keys = HashMap::new();

        for provider in Provider::all() {
            if provider.api_key_env().is_none() {
                continue;
            }
            if let Ok(key) = self
                .resolve_api_key_for_model(*provider, agent_api_key_config, primary_provider)
                .await
            {
                keys.insert(provider.as_llm_provider(), key);
            }
        }

        keys
    }

    pub(super) fn create_llm_client(
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

    pub(super) async fn build_failover_config(
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
        let config = self.storage.config.get_effective_config().ok();
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
}
