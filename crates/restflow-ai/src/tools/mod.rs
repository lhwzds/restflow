//! AI Tools module
//!
//! Core abstractions (Tool trait, ToolError, ToolRegistry, SecurityGate, etc.)
//! are defined here. Tool implementations live in `restflow-tools`.

// Core abstractions (defined here)
pub mod error;
pub mod filtered;
pub mod registry;
pub mod skill_types;
pub mod store_traits;
pub mod toolset;
pub mod traits;
pub mod wrapper;

// Re-export core abstractions
pub use error::{Result as ToolResult, ToolError};
pub use traits::{
    SecretResolver, Tool, ToolErrorCategory, ToolOutput, ToolSchema, check_security,
};
pub use registry::ToolRegistry;
pub use toolset::{Toolset, ToolsetContext};
pub use wrapper::{LoggingWrapper, RateLimitWrapper, TimeoutWrapper, ToolWrapper, WrappedTool};
pub use filtered::FilteredToolset;

// Re-export skill types
pub use skill_types::{SkillContent, SkillInfo, SkillProvider, SkillRecord, SkillUpdate};

// Re-export store traits
pub use store_traits::{
    AgentCreateRequest, AgentStore, AgentUpdateRequest,
    AuthProfileCreateRequest, AuthProfileStore, AuthProfileTestRequest, CredentialInput,
    BackgroundAgentControlRequest, BackgroundAgentCreateRequest,
    BackgroundAgentDeliverableListRequest, BackgroundAgentMessageListRequest,
    BackgroundAgentMessageRequest, BackgroundAgentProgressRequest,
    BackgroundAgentScratchpadListRequest, BackgroundAgentScratchpadReadRequest,
    BackgroundAgentStore, BackgroundAgentUpdateRequest,
    DeliverableStore,
    DiagnosticsProvider,
    MemoryClearRequest, MemoryCompactRequest, MemoryExportRequest, MemoryManager, MemoryStore,
    ProcessLog, ProcessManager, ProcessPollResult, ProcessSessionInfo,
    ReplySender,
    SessionCreateRequest, SessionListFilter, SessionSearchQuery, SessionStore,
    WorkspaceNotePatch, WorkspaceNoteProvider, WorkspaceNoteQuery, WorkspaceNoteRecord,
    WorkspaceNoteSpec, WorkspaceNoteStatus,
};
