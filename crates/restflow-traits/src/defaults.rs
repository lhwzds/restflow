//! Shared runtime default constants used across crates.

/// Default maximum ReAct iterations for agent/sub-agent execution.
pub const DEFAULT_AGENT_MAX_ITERATIONS: usize = 100;

/// Default maximum tool calls per agent run.
pub const DEFAULT_AGENT_MAX_TOOL_CALLS: usize = 200;

/// Default maximum tool calls per background-agent run.
pub const DEFAULT_BACKGROUND_MAX_TOOL_CALLS: usize = 100;

/// Default timeout (seconds) for the executor wrapper around tool calls.
pub const DEFAULT_AGENT_TOOL_TIMEOUT_SECS: u64 = 300;

/// Default timeout (seconds) for a single LLM request.
pub const DEFAULT_AGENT_LLM_TIMEOUT_SECS: u64 = 600;

/// Default timeout (seconds) for bash tool execution.
pub const DEFAULT_AGENT_BASH_TIMEOUT_SECS: u64 = 300;

/// Default timeout (seconds) for Python tool execution.
pub const DEFAULT_AGENT_PYTHON_TIMEOUT_SECS: u64 = 120;

/// Default timeout (seconds) for browser tool execution.
pub const DEFAULT_AGENT_BROWSER_TIMEOUT_SECS: u64 = 120;

/// Default timeout (seconds) for approval requests.
pub const DEFAULT_AGENT_APPROVAL_TIMEOUT_SECS: u64 = 300;

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

/// Default TTL (seconds) for finished process sessions.
pub const DEFAULT_PROCESS_SESSION_TTL_SECS: u64 = 30 * 60;

/// Default maximum length of tool results kept in agent context.
pub const DEFAULT_AGENT_MAX_TOOL_RESULT_LENGTH: usize = 4_000;

/// Default maximum number of tool calls allowed to run concurrently.
pub const DEFAULT_AGENT_MAX_TOOL_CONCURRENCY: usize = 100;

/// Default fallback context window used when model metadata is unavailable.
pub const DEFAULT_AGENT_CONTEXT_WINDOW_TOKENS: usize = 128_000;

/// Default max characters to keep from a pruned tool result.
pub const DEFAULT_AGENT_PRUNE_TOOL_MAX_CHARS: usize = 2_048;

/// Default number of recent tokens to preserve during context compaction.
pub const DEFAULT_AGENT_COMPACT_PRESERVE_TOKENS: usize = 20_000;

/// Default maximum total bytes loaded from workspace instruction files.
pub const DEFAULT_WORKSPACE_CONTEXT_MAX_TOTAL_BYTES: usize = 100_000;

/// Default maximum bytes loaded from a single workspace instruction file.
pub const DEFAULT_WORKSPACE_CONTEXT_MAX_FILE_BYTES: usize = 50_000;

/// Default event limit for background progress queries.
pub const DEFAULT_BG_PROGRESS_EVENT_LIMIT: usize = 10;

/// Default message list limit for background agents.
pub const DEFAULT_BG_MESSAGE_LIST_LIMIT: usize = 50;

/// Default trace list limit for background agents.
pub const DEFAULT_BG_TRACE_LIST_LIMIT: usize = 50;

/// Default trailing line limit for background trace reads.
pub const DEFAULT_BG_TRACE_LINE_LIMIT: usize = 200;

/// Default number of results returned by web search when no limit is specified.
pub const DEFAULT_API_WEB_SEARCH_RESULTS: usize = 5;

/// Hard cap for web search results per request.
pub const MAX_API_WEB_SEARCH_RESULTS: usize = 10;

/// Default timeout (milliseconds) for diagnostics collection.
pub const DEFAULT_API_DIAGNOSTICS_TIMEOUT_MS: u64 = 5_000;

/// Default file cache entry cap for agent session caches.
pub const DEFAULT_AGENT_CACHE_FILE_MAX_ENTRIES: usize = 100;

/// Default max file size cached for agent session caches.
pub const DEFAULT_AGENT_CACHE_FILE_MAX_BYTES: usize = 1_000_000;

/// Default permission cache TTL in seconds.
pub const DEFAULT_AGENT_CACHE_PERMISSION_TTL_SECS: u64 = 3_600;

/// Default search cache TTL in seconds.
pub const DEFAULT_AGENT_CACHE_SEARCH_TTL_SECS: u64 = 30;

/// Default search cache entry cap.
pub const DEFAULT_AGENT_CACHE_SEARCH_MAX_ENTRIES: usize = 50;
