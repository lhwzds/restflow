//! RestFlow Traits - Shared trait definitions and core abstractions.
//!
//! This crate provides the shared interfaces used across the RestFlow workspace:
//! - Tool trait, ToolError, ToolRegistry, Toolset
//! - SecurityGate, SecurityDecision, ToolAction
//! - NetworkAllowlist, NetworkEcosystem, SSRF validation
//! - SkillProvider and skill data types
//! - 18+ store traits (MemoryStore, AgentStore, SessionStore, etc.)
//! - Sub-agent data types and lookup traits

pub mod cache;
pub mod error;
pub mod filtered;
#[cfg(feature = "http-client")]
pub mod http_client;
pub mod llm;
pub mod network;
pub mod registry;
pub mod security;
pub mod skill;
pub mod store;
pub mod subagent;
pub mod tool;
pub mod toolset;
pub mod wrapper;

// ── Top-level re-exports ─────────────────────────────────────────────

// Error types
pub use error::{Result as ToolResult, ToolError};

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
    AuthProfileTestRequest, BackgroundAgentControlRequest, BackgroundAgentCreateRequest,
    BackgroundAgentDeliverableListRequest, BackgroundAgentMessageListRequest,
    BackgroundAgentMessageRequest, BackgroundAgentProgressRequest, BackgroundAgentStore,
    BackgroundAgentTraceListRequest, BackgroundAgentTraceReadRequest, BackgroundAgentUpdateRequest,
    CredentialInput, DeliverableStore, DiagnosticsProvider, KvStore, MarketplaceStore,
    MemoryClearRequest, MemoryCompactRequest, MemoryExportRequest, MemoryManager, MemoryStore,
    OpsProvider, ProcessLog, ProcessManager, ProcessPollResult, ProcessSessionInfo, ReplySender,
    SecurityQueryProvider, SessionCreateRequest, SessionListFilter, SessionSearchQuery,
    SessionStore, TerminalStore, TriggerStore, UnifiedMemorySearch, WorkItemPatch,
    WorkItemProvider, WorkItemQuery, WorkItemRecord, WorkItemSpec, WorkItemStatus,
};

// Sub-agent types
pub use subagent::{
    SpawnHandle, SpawnPriority, SpawnRequest, SubagentCompletion, SubagentConfig,
    SubagentDefLookup, SubagentDefSnapshot, SubagentDefSummary, SubagentManager, SubagentResult,
    SubagentSpawner, SubagentState, SubagentStatus,
};

// LLM switching
pub use llm::{LlmSwitcher, SwapResult};

// Cache types
pub use cache::{AgentCache, CachedSearchResult, SearchMatch};
