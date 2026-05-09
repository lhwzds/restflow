use super::*;

impl AgentRuntimeExecutor {
    pub(super) fn build_llm_factory(
        api_keys: HashMap<LlmProvider, String>,
        model_specs: Vec<types::ModelSpec>,
    ) -> Arc<dyn LlmClientFactory> {
        #[cfg(any(test, feature = "test-utils"))]
        if let Some(factory) = current_test_llm_factory() {
            return factory;
        }

        Arc::new(DefaultLlmClientFactory::new(api_keys, model_specs))
    }

    pub(super) fn should_skip_api_key_resolution() -> bool {
        #[cfg(any(test, feature = "test-utils"))]
        {
            return current_test_llm_factory().is_some();
        }

        #[allow(unreachable_code)]
        false
    }

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

        for secret_name in provider.api_key_env_candidates() {
            if let Some(secret_value) = self.storage.secrets.get_secret(secret_name)? {
                return Ok(secret_value);
            }
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
            ModelId::Gpt5_4 | ModelId::Gpt5_4Codex => 1_000_000,
            ModelId::Gpt5_4Mini | ModelId::Gpt5_4Nano | ModelId::Gpt5_4MiniCodex => 400_000,
            ModelId::DeepseekChat | ModelId::DeepseekReasoner => 64_000,
            ModelId::Gemini25Pro
            | ModelId::Gemini25Flash
            | ModelId::Gemini3Pro
            | ModelId::Gemini3Flash
            | ModelId::GeminiCli => 1_000_000,
            _ => 128_000,
        }
    }

    pub(super) async fn resolve_model_from_stored_credentials(&self) -> Result<Option<ModelId>> {
        Ok(
            resolve_model_from_credentials(self.auth_manager.as_ref(), |key| {
                secret_exists(&self.storage.secrets, key)
            })
            .await,
        )
    }

    pub(super) async fn resolve_primary_model(&self, agent_node: &AgentNode) -> Result<ModelId> {
        if let Some(model_ref) = agent_node.resolved_model_ref() {
            return Ok(model_ref.model);
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

        for provider in Provider::all().iter().copied() {
            if provider.api_key_env().is_none() {
                continue;
            }
            if let Ok(key) = self
                .resolve_api_key_for_model(provider, agent_api_key_config, primary_provider)
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
            let mut client = CodexClient::new().with_model(model.as_str());
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
}
