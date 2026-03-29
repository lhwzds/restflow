use serde_json::Value;

use restflow_traits::config_types::ConfigDocument;

use crate::Result;

use super::super::fields;
use super::super::parse::{parse_u64, parse_usize};

pub(crate) fn apply(field: &str, value: &Value, config: &mut ConfigDocument) -> Result<()> {
    match field {
        "background_runner_poll_interval_ms" => {
            config.runtime.background_runner_poll_interval_ms =
                parse_u64(value, "runtime.background_runner_poll_interval_ms")?;
        }
        "background_runner_max_concurrent_tasks" => {
            config.runtime.background_runner_max_concurrent_tasks =
                parse_usize(value, "runtime.background_runner_max_concurrent_tasks")?;
        }
        "chat_max_session_history" => {
            config.runtime.chat_max_session_history =
                parse_usize(value, "runtime.chat_max_session_history")?;
        }
        _ => {
            return Err(fields::unknown_domain_field(
                "runtime",
                field,
                fields::VALID_RUNTIME_FIELDS,
            ));
        }
    }
    Ok(())
}
