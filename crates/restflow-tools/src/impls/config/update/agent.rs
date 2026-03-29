use serde_json::Value;

use restflow_traits::config_types::ConfigDocument;

use crate::Result;

use super::super::fields;
use super::super::parse::{
    parse_optional_string_list, parse_optional_timeout, parse_u64, parse_usize,
};

pub(crate) fn apply(field: &str, value: &Value, config: &mut ConfigDocument) -> Result<()> {
    match field {
        "tool_timeout_secs" => {
            config.agent.tool_timeout_secs = parse_u64(value, "agent.tool_timeout_secs")?;
        }
        "llm_timeout_secs" => {
            config.agent.llm_timeout_secs =
                parse_optional_timeout(value, "agent.llm_timeout_secs")?;
        }
        "bash_timeout_secs" => {
            config.agent.bash_timeout_secs = parse_u64(value, "agent.bash_timeout_secs")?;
        }
        "python_timeout_secs" => {
            config.agent.python_timeout_secs = parse_u64(value, "agent.python_timeout_secs")?;
        }
        "browser_timeout_secs" => {
            config.agent.browser_timeout_secs = parse_u64(value, "agent.browser_timeout_secs")?;
        }
        "process_session_ttl_secs" => {
            config.agent.process_session_ttl_secs =
                parse_u64(value, "agent.process_session_ttl_secs")?;
        }
        "approval_timeout_secs" => {
            config.agent.approval_timeout_secs = parse_u64(value, "agent.approval_timeout_secs")?;
        }
        "max_iterations" => {
            config.agent.max_iterations = parse_usize(value, "agent.max_iterations")?;
        }
        "max_depth" => {
            config.agent.max_depth = parse_usize(value, "agent.max_depth")?;
        }
        "subagent_timeout_secs" => {
            config.agent.subagent_timeout_secs = parse_u64(value, "agent.subagent_timeout_secs")?;
        }
        "max_parallel_subagents" => {
            config.agent.max_parallel_subagents =
                parse_usize(value, "agent.max_parallel_subagents")?;
        }
        "max_tool_calls" => {
            config.agent.max_tool_calls = parse_usize(value, "agent.max_tool_calls")?;
        }
        "max_tool_concurrency" => {
            config.agent.max_tool_concurrency = parse_usize(value, "agent.max_tool_concurrency")?;
        }
        "max_tool_result_length" => {
            config.agent.max_tool_result_length =
                parse_usize(value, "agent.max_tool_result_length")?;
        }
        "prune_tool_max_chars" => {
            config.agent.prune_tool_max_chars = parse_usize(value, "agent.prune_tool_max_chars")?;
        }
        "compact_preserve_tokens" => {
            config.agent.compact_preserve_tokens =
                parse_usize(value, "agent.compact_preserve_tokens")?;
        }
        "max_wall_clock_secs" => {
            config.agent.max_wall_clock_secs =
                parse_optional_timeout(value, "agent.max_wall_clock_secs")?;
        }
        "default_task_timeout_secs" => {
            config.agent.default_task_timeout_secs =
                parse_u64(value, "agent.default_task_timeout_secs")?;
        }
        "default_max_duration_secs" => {
            config.agent.default_max_duration_secs =
                parse_u64(value, "agent.default_max_duration_secs")?;
        }
        "fallback_models" => {
            config.agent.fallback_models =
                parse_optional_string_list(value, "agent.fallback_models")?;
        }
        _ => {
            return Err(fields::unknown_domain_field(
                "agent",
                field,
                fields::VALID_AGENT_FIELDS,
            ));
        }
    }
    Ok(())
}
