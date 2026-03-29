use serde_json::Value;

use restflow_traits::config_types::ConfigDocument;

use crate::Result;

use super::super::fields;
use super::super::parse::{parse_u32, parse_u64, parse_usize};

pub(crate) fn apply(field: &str, value: &Value, config: &mut ConfigDocument) -> Result<()> {
    match field {
        "memory_search_limit" => {
            config.api.memory_search_limit = parse_u32(value, "api.memory_search_limit")?;
        }
        "session_list_limit" => {
            config.api.session_list_limit = parse_u32(value, "api.session_list_limit")?;
        }
        "background_progress_event_limit" => {
            config.api.background_progress_event_limit =
                parse_usize(value, "api.background_progress_event_limit")?;
        }
        "background_message_list_limit" => {
            config.api.background_message_list_limit =
                parse_usize(value, "api.background_message_list_limit")?;
        }
        "background_trace_list_limit" => {
            config.api.background_trace_list_limit =
                parse_usize(value, "api.background_trace_list_limit")?;
        }
        "background_trace_line_limit" => {
            config.api.background_trace_line_limit =
                parse_usize(value, "api.background_trace_line_limit")?;
        }
        "web_search_num_results" => {
            config.api.web_search_num_results = parse_usize(value, "api.web_search_num_results")?;
        }
        "diagnostics_timeout_ms" => {
            config.api.diagnostics_timeout_ms = parse_u64(value, "api.diagnostics_timeout_ms")?;
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
