use serde_json::Value;

use restflow_traits::config_types::ConfigDocument;

use crate::Result;

use super::super::parse::{parse_optional_timeout, parse_string_list, parse_u32, parse_u64};

pub(crate) fn supports_key(key: &str) -> bool {
    matches!(
        key,
        "system.worker_count"
            | "system.task_timeout_seconds"
            | "system.stall_timeout_seconds"
            | "system.background_api_timeout_seconds"
            | "system.chat_response_timeout_seconds"
            | "system.max_retries"
            | "system.chat_session_retention_days"
            | "system.background_task_retention_days"
            | "system.checkpoint_retention_days"
            | "system.memory_chunk_retention_days"
            | "system.log_file_retention_days"
            | "system.experimental_features"
    )
}

pub(crate) fn apply(key: &str, value: &Value, config: &mut ConfigDocument) -> Result<()> {
    match key {
        "system.worker_count" => {
            config.system.worker_count = parse_u64(value, key)? as usize;
        }
        "system.task_timeout_seconds" => {
            config.system.task_timeout_seconds = parse_u64(value, key)?;
        }
        "system.stall_timeout_seconds" => {
            config.system.stall_timeout_seconds = parse_u64(value, key)?;
        }
        "system.background_api_timeout_seconds" => {
            config.system.background_api_timeout_seconds = parse_optional_timeout(value, key)?;
        }
        "system.chat_response_timeout_seconds" => {
            config.system.chat_response_timeout_seconds = parse_optional_timeout(value, key)?;
        }
        "system.max_retries" => {
            config.system.max_retries = parse_u32(value, key)?;
        }
        "system.chat_session_retention_days" => {
            config.system.chat_session_retention_days = parse_u32(value, key)?;
        }
        "system.background_task_retention_days" => {
            config.system.background_task_retention_days = parse_u32(value, key)?;
        }
        "system.checkpoint_retention_days" => {
            config.system.checkpoint_retention_days = parse_u32(value, key)?;
        }
        "system.memory_chunk_retention_days" => {
            config.system.memory_chunk_retention_days = parse_u32(value, key)?;
        }
        "system.log_file_retention_days" => {
            config.system.log_file_retention_days = parse_u32(value, key)?;
        }
        "system.experimental_features" => {
            config.system.experimental_features = parse_string_list(value, key)?;
        }
        _ => unreachable!("unsupported system config key: {key}"),
    }
    Ok(())
}
