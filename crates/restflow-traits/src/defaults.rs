//! Shared runtime default constants used across crates.

/// Default maximum ReAct iterations for agent/sub-agent execution.
pub const DEFAULT_AGENT_MAX_ITERATIONS: usize = 100;

/// Default maximum tool calls per agent run.
pub const DEFAULT_AGENT_MAX_TOOL_CALLS: usize = 200;

/// Default maximum tool calls per background-agent run.
pub const DEFAULT_BACKGROUND_MAX_TOOL_CALLS: usize = 100;

/// Default timeout (seconds) for sub-agent execution.
pub const DEFAULT_SUBAGENT_TIMEOUT_SECS: u64 = 3600;

/// Default cap for maximum parallel sub-agents.
pub const DEFAULT_MAX_PARALLEL_SUBAGENTS: usize = 200;

/// Default sub-agent nesting depth.
pub const DEFAULT_SUBAGENT_MAX_DEPTH: usize = 1;

/// Default timeout (seconds) for background task execution.
pub const DEFAULT_AGENT_TASK_TIMEOUT_SECS: u64 = 1800;

/// Default maximum execution duration (seconds) for background tasks.
pub const DEFAULT_AGENT_MAX_DURATION_SECS: u64 = 1800;

/// Default event limit for background progress queries.
pub const DEFAULT_BG_PROGRESS_EVENT_LIMIT: usize = 10;

/// Default message list limit for background agents.
pub const DEFAULT_BG_MESSAGE_LIST_LIMIT: usize = 50;

/// Default trace list limit for background agents.
pub const DEFAULT_BG_TRACE_LIST_LIMIT: usize = 50;

/// Default trailing line limit for background trace reads.
pub const DEFAULT_BG_TRACE_LINE_LIMIT: usize = 200;
