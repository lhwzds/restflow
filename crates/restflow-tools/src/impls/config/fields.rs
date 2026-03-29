use crate::ToolError;

pub(crate) const SUPPORTED_FIELDS: &[&str] = &[
    "system.worker_count",
    "system.task_timeout_seconds",
    "system.stall_timeout_seconds",
    "system.background_api_timeout_seconds",
    "system.chat_response_timeout_seconds",
    "system.max_retries",
    "system.chat_session_retention_days",
    "system.background_task_retention_days",
    "system.checkpoint_retention_days",
    "system.memory_chunk_retention_days",
    "system.log_file_retention_days",
    "system.experimental_features",
    "agent.tool_timeout_secs",
    "agent.llm_timeout_secs",
    "agent.bash_timeout_secs",
    "agent.python_timeout_secs",
    "agent.browser_timeout_secs",
    "agent.process_session_ttl_secs",
    "agent.approval_timeout_secs",
    "agent.max_iterations",
    "agent.max_depth",
    "agent.subagent_timeout_secs",
    "agent.max_parallel_subagents",
    "agent.max_tool_calls",
    "agent.max_tool_concurrency",
    "agent.max_tool_result_length",
    "agent.prune_tool_max_chars",
    "agent.compact_preserve_tokens",
    "agent.max_wall_clock_secs",
    "agent.default_task_timeout_secs",
    "agent.default_max_duration_secs",
    "agent.fallback_models",
    "api.memory_search_limit",
    "api.session_list_limit",
    "api.background_progress_event_limit",
    "api.background_message_list_limit",
    "api.background_trace_list_limit",
    "api.background_trace_line_limit",
    "api.web_search_num_results",
    "api.diagnostics_timeout_ms",
    "runtime.background_runner_poll_interval_ms",
    "runtime.background_runner_max_concurrent_tasks",
    "runtime.chat_max_session_history",
    "channel.telegram_api_timeout_secs",
    "channel.telegram_polling_timeout_secs",
    "registry.github_cache_ttl_secs",
    "registry.marketplace_cache_ttl_secs",
];

pub(crate) const VALID_TOP_LEVEL_FIELDS: &str =
    "system.*, agent.*, api.*, runtime.*, channel.*, registry.*";
pub(crate) const VALID_AGENT_FIELDS: &str = "agent.tool_timeout_secs, agent.llm_timeout_secs, agent.bash_timeout_secs, agent.python_timeout_secs, agent.browser_timeout_secs, agent.process_session_ttl_secs, agent.approval_timeout_secs, agent.max_iterations, agent.max_depth, agent.subagent_timeout_secs, agent.max_parallel_subagents, agent.max_tool_calls, agent.max_tool_concurrency, agent.max_tool_result_length, agent.prune_tool_max_chars, agent.compact_preserve_tokens, agent.max_wall_clock_secs, agent.default_task_timeout_secs, agent.default_max_duration_secs, agent.fallback_models";
pub(crate) const VALID_API_FIELDS: &str = "api.memory_search_limit, api.session_list_limit, api.background_progress_event_limit, api.background_message_list_limit, api.background_trace_list_limit, api.background_trace_line_limit, api.web_search_num_results, api.diagnostics_timeout_ms";
pub(crate) const VALID_RUNTIME_FIELDS: &str = "runtime.background_runner_poll_interval_ms, runtime.background_runner_max_concurrent_tasks, runtime.chat_max_session_history";
pub(crate) const VALID_CHANNEL_FIELDS: &str =
    "channel.telegram_api_timeout_secs, channel.telegram_polling_timeout_secs";
pub(crate) const VALID_REGISTRY_FIELDS: &str =
    "registry.github_cache_ttl_secs, registry.marketplace_cache_ttl_secs";

pub(crate) fn unknown_top_level_field(key: &str) -> ToolError {
    ToolError::Tool(format!(
        "Unknown config field: '{key}'. Valid fields: {VALID_TOP_LEVEL_FIELDS}."
    ))
}

pub(crate) fn unknown_domain_field(domain: &str, field: &str, valid_fields: &str) -> ToolError {
    ToolError::Tool(format!(
        "Unknown {domain} config field: '{domain}.{field}'. Valid {domain} fields: {valid_fields}."
    ))
}
