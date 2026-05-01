mod agent;
mod api;
mod channel;
mod registry;
mod runtime;
mod system;

use serde_json::Value;

use restflow_traits::config_types::ConfigDocument;

use crate::Result;

use super::fields;

pub(crate) fn apply_update(key: &str, value: &Value, config: &mut ConfigDocument) -> Result<()> {
    if system::supports_key(key) {
        return system::apply(key, value, config);
    }
    if let Some(field) = key.strip_prefix("agent.") {
        return agent::apply(field, value, config);
    }
    if let Some(field) = key.strip_prefix("api.") {
        return api::apply(field, value, config);
    }
    if let Some(field) = key.strip_prefix("runtime.") {
        return runtime::apply(field, value, config);
    }
    if let Some(field) = key.strip_prefix("channel.") {
        return channel::apply(field, value, config);
    }
    if let Some(field) = key.strip_prefix("registry.") {
        return registry::apply(field, value, config);
    }
    Err(fields::unknown_top_level_field(key))
}
