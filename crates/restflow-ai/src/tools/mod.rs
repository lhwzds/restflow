//! AI Tools module
//!
//! Core abstractions (Tool trait, ToolError, ToolRegistry, SecurityGate, etc.)
//! are defined in `restflow-traits`. This module re-exports them and adds
//! runtime wrappers such as `LoggingWrapper`.

pub mod wrapper;

// Re-export core abstractions from restflow-traits
pub use restflow_traits::error::{Result as ToolResult, ToolError};
pub use restflow_traits::filtered::{FilteredToolset, ToolPredicate};
pub use restflow_traits::registry::ToolRegistry;
pub use restflow_traits::tool::{
    SecretResolver, Tool, ToolErrorCategory, ToolOutput, ToolSchema, check_security,
};
pub use restflow_traits::toolset::{Toolset, ToolsetContext};
pub use restflow_traits::wrapper::{RateLimitWrapper, TimeoutWrapper, ToolWrapper, WrappedTool};
pub use wrapper::LoggingWrapper;

// Re-export skill types
pub use restflow_traits::skill::{
    SkillContent, SkillInfo, SkillProvider, SkillRecord, SkillUpdate,
};

// Re-export store traits
pub use restflow_traits::store::{
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
