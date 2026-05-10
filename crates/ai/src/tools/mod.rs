//! AI Tools module
//!
//! Core abstractions (Tool trait, ToolError, ToolRegistry, SecurityGate, etc.)
//! are defined in `types`. This module re-exports them and adds
//! runtime wrappers such as `LoggingWrapper`.

pub mod wrapper;

pub use types::error::{Result as ToolResult, ToolError};
pub use types::skill::{SkillContent, SkillInfo, SkillProvider};
pub use types::store::{
    AgentCreateRequest, AgentStore, AgentUpdateRequest, OpsProvider, ProcessLog, ProcessManager,
    ProcessPollResult, ProcessSessionInfo, ReplySender, SessionCreateRequest, SessionListFilter,
    SessionSearchQuery, SessionStore,
};
pub use types::tool::{
    SecretResolver, Tool, ToolErrorCategory, ToolOutput, ToolSchema, check_security,
};
pub use types::toolset::{
    FilteredToolset, RateLimitWrapper, TimeoutWrapper, ToolPredicate, ToolRegistry, ToolWrapper,
    Toolset, ToolsetContext, WrappedTool,
};
pub use wrapper::LoggingWrapper;
