use super::*;
#[cfg(test)]
use crate::auth::provider_access::build_runtime_api_keys;

#[cfg(test)]
pub(super) fn build_api_keys(
    secret_storage: Option<&SecretStorage>,
) -> HashMap<LlmProvider, String> {
    build_runtime_api_keys(secret_storage)
}

#[cfg(test)]
pub(super) fn build_llm_factory(
    secret_storage: Option<&SecretStorage>,
) -> Arc<dyn LlmClientFactory> {
    let api_keys = build_api_keys(secret_storage);
    Arc::new(DefaultLlmClientFactory::new(
        api_keys,
        ModelId::build_model_specs(),
    ))
}

#[cfg(test)]
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

#[cfg(test)]
pub(super) fn load_subagent_config(config_storage: &ConfigStorage) -> SubagentConfig {
    let defaults = load_agent_defaults(config_storage);
    build_subagent_config(&defaults)
}
