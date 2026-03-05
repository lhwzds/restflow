//! Shared runtime default constants used across crates.

/// Default maximum ReAct iterations for agent/sub-agent execution.
pub const DEFAULT_AGENT_MAX_ITERATIONS: usize = 100;

/// Default timeout (seconds) for sub-agent execution.
pub const DEFAULT_SUBAGENT_TIMEOUT_SECS: u64 = 3600;

/// Default cap for maximum parallel sub-agents.
pub const DEFAULT_MAX_PARALLEL_SUBAGENTS: usize = 200;

/// Default sub-agent nesting depth.
pub const DEFAULT_SUBAGENT_MAX_DEPTH: usize = 1;
