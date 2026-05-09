use serde_json::Value;

use types::config_types::ConfigDocument;

use crate::Result;

use super::super::fields;
use super::super::parse::{parse_u32, parse_usize};

pub(crate) fn apply(field: &str, value: &Value, config: &mut ConfigDocument) -> Result<()> {
    match field {
        "session_list_limit" => {
            config.api.session_list_limit = parse_u32(value, "api.session_list_limit")?;
        }
        "task_progress_event_limit" => {
            config.api.task_progress_event_limit =
                parse_usize(value, "api.task_progress_event_limit")?;
        }
        "task_message_list_limit" => {
            config.api.task_message_list_limit = parse_usize(value, "api.task_message_list_limit")?;
        }
        _ => {
            return Err(fields::unknown_domain_field(
                "api",
                field,
                fields::VALID_API_FIELDS,
            ));
        }
    }
    Ok(())
}
