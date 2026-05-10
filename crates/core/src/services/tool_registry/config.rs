use super::*;

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
