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

    pub(super) fn default_model_for_provider(provider: Provider) -> ModelId {
        match provider {
            Provider::OpenAI => ModelId::Gpt5,
            Provider::Anthropic => ModelId::ClaudeOpus4_6,
            Provider::ClaudeCode => ModelId::ClaudeCodeOpus,
            Provider::Codex => ModelId::CodexCli,
            Provider::DeepSeek => ModelId::DeepseekChat,
            Provider::Google => ModelId::Gemini25Pro,
            Provider::Groq => ModelId::GroqLlama4Maverick,
            Provider::OpenRouter => ModelId::OpenRouterAuto,
            Provider::XAI => ModelId::Grok4,
            Provider::Qwen => ModelId::Qwen3Max,
            Provider::Zai => ModelId::Glm5,
            Provider::ZaiCodingPlan => ModelId::Glm5CodingPlan,
            Provider::Moonshot => ModelId::KimiK2_5,
            Provider::Doubao => ModelId::DoubaoPro,
            Provider::Yi => ModelId::YiLightning,
            Provider::SiliconFlow => ModelId::SiliconFlowAuto,
            Provider::MiniMax => ModelId::MiniMaxM27,
            Provider::MiniMaxCodingPlan => ModelId::MiniMaxM25CodingPlan,
        }
    }

    pub(super) fn context_window_for_model(model: ModelId) -> usize {
        match model {
            ModelId::ClaudeOpus4_6
            | ModelId::ClaudeSonnet4_5
            | ModelId::ClaudeHaiku4_5
            | ModelId::ClaudeCodeOpus
            | ModelId::ClaudeCodeSonnet
            | ModelId::ClaudeCodeHaiku => 200_000,
            ModelId::Gpt5
            | ModelId::Gpt5Mini
            | ModelId::Gpt5Nano
            | ModelId::Gpt5Pro
            | ModelId::Gpt5_1
            | ModelId::Gpt5_2
            | ModelId::Gpt5Codex
            | ModelId::Gpt5_1Codex
            | ModelId::Gpt5_2Codex
            | ModelId::CodexCli => 128_000,
            ModelId::DeepseekChat | ModelId::DeepseekReasoner => 64_000,
            ModelId::Gemini25Pro
            | ModelId::Gemini25Flash
            | ModelId::Gemini3Pro
            | ModelId::Gemini3Flash
            | ModelId::GeminiCli => 1_000_000,
            _ => 128_000,
        }
    }

    pub(super) fn has_non_empty_secret(&self, name: &str) -> Result<bool> {
        Ok(self.storage.secrets.get_non_empty(name)?.is_some())
    }

    pub(super) async fn resolve_model_from_stored_credentials(&self) -> Result<Option<ModelId>> {
        // Prefer Codex CLI model only when a dedicated OpenAI Codex profile exists.
        if self
            .auth_manager
            .get_available_profile(AuthProvider::OpenAICodex)
            .await
            .is_some()
        {
            return Ok(Some(ModelId::CodexCli));
        }

        // Then try provider-specific auth profiles.
        let profile_order = [
            (AuthProvider::ClaudeCode, ModelId::ClaudeCodeOpus),
            (AuthProvider::Anthropic, ModelId::ClaudeOpus4_6),
            (AuthProvider::OpenAI, ModelId::Gpt5),
            (AuthProvider::Google, ModelId::Gemini25Pro),
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

    pub(super) async fn resolve_primary_model(&self, agent_node: &AgentNode) -> Result<ModelId> {
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
        model: ModelId,
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
        primary: ModelId,
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
        let fallback_models: Option<Vec<ModelId>> = config
            .as_ref()
            .and_then(|c| c.agent.fallback_models.clone())
            .map(|models| {
                models
                    .iter()
                    .filter_map(|s| ModelId::from_api_name(s))
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
