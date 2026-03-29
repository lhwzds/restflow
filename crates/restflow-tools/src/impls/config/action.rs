use serde::Deserialize;
use serde_json::Value;

use restflow_traits::config_types::ConfigDocument;

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub(crate) enum ConfigAction {
    Get,
    Show,
    List,
    Reset,
    Set {
        #[serde(default)]
        config: Option<Box<ConfigDocument>>,
        #[serde(default)]
        key: Option<String>,
        #[serde(default)]
        value: Option<Value>,
    },
}
