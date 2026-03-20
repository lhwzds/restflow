use super::*;

pub(super) fn build_api_keys(
    secret_storage: Option<&SecretStorage>,
) -> HashMap<LlmProvider, String> {
    let mut keys = HashMap::new();
    for provider in Provider::all() {
        let Some(env_name) = provider.api_key_env() else {
            continue;
        };
        if let Some(storage) = secret_storage
            && let Ok(Some(value)) = storage.get_secret(env_name)
            && !value.trim().is_empty()
        {
            keys.insert(provider.as_llm_provider(), value);
            continue;
        }

        if let Ok(value) = std::env::var(env_name)
            && !value.trim().is_empty()
        {
            keys.insert(provider.as_llm_provider(), value);
        }
    }
    keys
}

pub(super) fn build_llm_factory(
    secret_storage: Option<&SecretStorage>,
) -> Arc<dyn LlmClientFactory> {
    let api_keys = build_api_keys(secret_storage);
    Arc::new(DefaultLlmClientFactory::new(
        api_keys,
        ModelId::build_model_specs(),
    ))
}

pub(super) fn build_switch_model_tool(factory: Arc<dyn LlmClientFactory>) -> SwitchModelTool {
    let initial_client: Arc<dyn LlmClient> = Arc::new(CodexClient::new());
    let swappable = Arc::new(SwappableLlm::new(initial_client));
    let switcher = Arc::new(LlmSwitcherImpl::new(swappable, factory));
    SwitchModelTool::new(switcher)
}

pub(super) fn build_subagent_config(defaults: &AgentDefaults) -> SubagentConfig {
    SubagentConfig {
        max_parallel_agents: defaults.max_parallel_subagents,
        subagent_timeout_secs: defaults.subagent_timeout_secs,
        max_iterations: defaults.max_iterations,
        max_depth: defaults.max_depth,
    }
}

fn load_system_config(config_storage: &ConfigStorage) -> SystemConfig {
    match config_storage.get_effective_config() {
        Ok(config) => config,
        Err(error) => {
            warn!(
                error = %error,
                "Failed to load system config defaults; falling back to built-in defaults"
            );
            SystemConfig::default()
        }
    }
}

pub(super) fn load_agent_defaults(config_storage: &ConfigStorage) -> AgentDefaults {
    load_system_config(config_storage).agent
}

pub(super) fn load_api_defaults(config_storage: &ConfigStorage) -> ApiDefaults {
    load_system_config(config_storage).api_defaults
}

pub(super) fn load_registry_defaults(
    config_storage: &ConfigStorage,
) -> restflow_storage::RegistryDefaults {
    load_system_config(config_storage).registry_defaults
}

pub(super) fn load_subagent_config(config_storage: &ConfigStorage) -> SubagentConfig {
    let defaults = load_agent_defaults(config_storage);
    build_subagent_config(&defaults)
}
