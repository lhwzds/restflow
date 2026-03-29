//! RestFlow Traits - Shared trait definitions and core abstractions.
//!
//! This crate provides the shared interfaces used across the RestFlow workspace:
//! - Tool trait, ToolError, ToolRegistry, Toolset
//! - SecurityGate, SecurityDecision, ToolAction
//! - NetworkAllowlist, NetworkEcosystem, SSRF validation
//! - SkillProvider and skill data types
//! - 18+ store traits (MemoryStore, AgentStore, SessionStore, etc.)
//! - Sub-agent data types and lookup traits

pub mod assessment;
pub mod batch_template;
pub mod boundary;
pub mod cache;
pub mod config_types;
pub mod defaults;
pub mod error;
pub mod filtered;
#[cfg(feature = "http-client")]
pub mod http_client;
pub mod llm;
pub mod model;
pub mod network;
pub mod orchestrator;
pub mod registry;
pub mod security;
pub mod skill;
pub mod steer;
pub mod store;
pub mod subagent;
pub mod text;
pub mod tool;
pub mod toolset;
pub mod wrapper;

// ── Top-level re-exports ─────────────────────────────────────────────

// Error types
pub use error::{Result as ToolResult, ToolError};

// Assessment types
pub use assessment::{
    AgentOperationAssessor, AssessmentModelRef, BackgroundAgentCommandOutcome, OperationAssessment,
    OperationAssessmentIntent, OperationAssessmentIssue, OperationAssessmentStatus,
};

// Tool trait and core types
pub use tool::{SecretResolver, Tool, ToolErrorCategory, ToolOutput, ToolSchema, check_security};

// Registry and toolset
pub use registry::ToolRegistry;
pub use toolset::{Toolset, ToolsetContext};

// Wrappers
pub use wrapper::{RateLimitWrapper, TimeoutWrapper, ToolWrapper, WrappedTool};

// Filtered toolset
pub use filtered::{FilteredToolset, ToolPredicate};

// Security
pub use network::{
    NetworkAllowlist, NetworkEcosystem, is_restricted_ip, resolve_and_validate_url, validate_url,
};
pub use security::{SecurityDecision, SecurityGate, ToolAction};

// Skill types
pub use skill::{SkillContent, SkillInfo, SkillProvider, SkillRecord, SkillUpdate};

// Store traits
pub use store::{
    AgentCreateRequest, AgentStore, AgentUpdateRequest, AuthProfileCreateRequest, AuthProfileStore,
    AuthProfileTestRequest, BackgroundAgentControlRequest, BackgroundAgentConvertSessionRequest,
    BackgroundAgentCreateRequest, BackgroundAgentDeliverableListRequest,
    BackgroundAgentMessageListRequest, BackgroundAgentMessageRequest,
    BackgroundAgentProgressRequest, BackgroundAgentStore, BackgroundAgentTraceListRequest,
    BackgroundAgentTraceReadRequest, BackgroundAgentUpdateRequest, ConfigStore, CredentialInput,
    DeliverableStore, DiagnosticsProvider, KvStore, MarketplaceStore, MemoryClearRequest,
    MemoryCompactRequest, MemoryExportRequest, MemoryManager, MemoryStore, OpsProvider, ProcessLog,
    ProcessManager, ProcessPollResult, ProcessSessionInfo, ReplySender, SecretStore,
    SecurityQueryProvider, SessionCreateRequest, SessionListFilter, SessionSearchQuery,
    SessionStore, TerminalStore, TriggerStore, UnifiedMemorySearch, WorkItemPatch,
    WorkItemProvider, WorkItemQuery, WorkItemRecord, WorkItemSpec, WorkItemStatus,
};

// Shared orchestration contracts
pub use batch_template::{RuntimeTaskPayload, TeamTemplateDocument};
pub use orchestrator::{AgentOrchestrator, ExecutionMode, ExecutionOutcome, ExecutionPlan};

// Sub-agent types
pub use subagent::{
    ContractSubagentSpawnRequest, InlineSubagentConfig, SpawnHandle, SpawnPriority, SpawnRequest,
    SubagentCompletion, SubagentConfig, SubagentDefLookup, SubagentDefSnapshot, SubagentDefSummary,
    SubagentEffectiveLimits, SubagentLimitSource, SubagentManager, SubagentResult, SubagentSpawner,
    SubagentState, SubagentStatus,
};

// LLM switching
pub use llm::{ClientKind, LlmProvider, LlmSwitcher, SwapResult};

// Shared model/provider normalization
pub use model::ModelProvider;

// Shared steer/runtime control types
pub use steer::{SteerCommand, SteerMessage, SteerSource};

// Shared default constants
pub use defaults::{
    DEFAULT_AGENT_APPROVAL_TIMEOUT_SECS, DEFAULT_AGENT_BASH_TIMEOUT_SECS,
    DEFAULT_AGENT_BROWSER_TIMEOUT_SECS, DEFAULT_AGENT_CACHE_FILE_MAX_BYTES,
    DEFAULT_AGENT_CACHE_FILE_MAX_ENTRIES, DEFAULT_AGENT_CACHE_PERMISSION_TTL_SECS,
    DEFAULT_AGENT_CACHE_SEARCH_MAX_ENTRIES, DEFAULT_AGENT_CACHE_SEARCH_TTL_SECS,
    DEFAULT_AGENT_COMPACT_PRESERVE_TOKENS, DEFAULT_AGENT_CONTEXT_WINDOW_TOKENS,
    DEFAULT_AGENT_LLM_TIMEOUT_SECS, DEFAULT_AGENT_MAX_DURATION_SECS, DEFAULT_AGENT_MAX_ITERATIONS,
    DEFAULT_AGENT_MAX_TOOL_CALLS, DEFAULT_AGENT_MAX_TOOL_CONCURRENCY,
    DEFAULT_AGENT_MAX_TOOL_RESULT_LENGTH, DEFAULT_AGENT_PRUNE_TOOL_MAX_CHARS,
    DEFAULT_AGENT_PYTHON_TIMEOUT_SECS, DEFAULT_AGENT_TASK_TIMEOUT_SECS,
    DEFAULT_AGENT_TOOL_TIMEOUT_SECS, DEFAULT_API_DIAGNOSTICS_TIMEOUT_MS,
    DEFAULT_API_WEB_SEARCH_RESULTS, DEFAULT_BACKGROUND_MAX_TOOL_CALLS,
    DEFAULT_BACKGROUND_RUNNER_MAX_CONCURRENT_TASKS, DEFAULT_BACKGROUND_RUNNER_POLL_INTERVAL_MS,
    DEFAULT_BG_MESSAGE_LIST_LIMIT, DEFAULT_BG_PROGRESS_EVENT_LIMIT, DEFAULT_BG_TRACE_LINE_LIMIT,
    DEFAULT_BG_TRACE_LIST_LIMIT, DEFAULT_CHAT_MAX_SESSION_HISTORY, DEFAULT_GITHUB_CACHE_TTL_SECS,
    DEFAULT_MARKETPLACE_CACHE_TTL_SECS, DEFAULT_MAX_PARALLEL_SUBAGENTS,
    DEFAULT_PROCESS_SESSION_TTL_SECS, DEFAULT_SUBAGENT_MAX_DEPTH, DEFAULT_SUBAGENT_TIMEOUT_SECS,
    DEFAULT_TELEGRAM_API_TIMEOUT_SECS, DEFAULT_TELEGRAM_POLLING_TIMEOUT_SECS,
    DEFAULT_WORKSPACE_CONTEXT_MAX_FILE_BYTES, DEFAULT_WORKSPACE_CONTEXT_MAX_TOTAL_BYTES,
    MAX_API_WEB_SEARCH_RESULTS,
};

// Cache types
pub use cache::{AgentCache, CachedSearchResult, SearchMatch};

// Shared text helpers
pub use text::floor_char_boundary;
