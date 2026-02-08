//! AI Tools module - Agent tool implementations
//!
//! This module provides tools that can be used by AI agents.
//! Tools implement the `Tool` trait for integration with the agent executor.

use std::sync::Arc;

mod agent_crud;
mod auth_profile;
mod bash;
mod config;
mod diagnostics;
mod email;
mod file;
mod file_memory;
mod file_tracker;
mod http;
mod jina_reader;
mod mcp_cache;
mod memory_mgmt;
mod memory_search;
mod patch;
mod process;
mod python;
mod registry;
mod reply;
mod secrets;
mod session;
mod skill;
mod switch_model;
mod background_agent;
mod telegram;
mod traits;
mod transcribe;
mod vision;
mod web_fetch;
mod web_search;

use file_tracker::FileTracker;

pub use agent_crud::{AgentCreateRequest, AgentCrudTool, AgentStore, AgentUpdateRequest};
pub use auth_profile::{
    AuthProfileCreateRequest, AuthProfileStore, AuthProfileTestRequest, AuthProfileTool,
    CredentialInput,
};
pub use bash::{BashInput, BashOutput, BashTool};
pub use config::ConfigTool;
pub use diagnostics::{DiagnosticsProvider, DiagnosticsTool};
pub use email::EmailTool;
pub use file::{FileAction, FileTool};
pub use file_memory::{
    DeleteMemoryTool, FileMemoryConfig, FileMemoryStore, ListMemoryTool, MemoryEntry,
    MemoryEntryMeta, MemoryStore, ReadMemoryTool, SaveMemoryTool,
};
pub use http::HttpTool;
pub use jina_reader::JinaReaderTool;
pub use mcp_cache::{McpServerConfig, get_mcp_tools, invalidate_mcp_cache};
pub use memory_mgmt::{
    MemoryClearRequest, MemoryCompactRequest, MemoryExportRequest, MemoryManagementTool,
    MemoryManager,
};
pub use memory_search::{MemorySearchMatch, MemorySearchTool, SemanticMemory};
pub use patch::PatchTool;
pub use process::{ProcessLog, ProcessManager, ProcessPollResult, ProcessSessionInfo, ProcessTool};
pub use python::PythonTool;
pub use registry::ToolRegistry;
pub use reply::{ReplySender, ReplyTool};
pub use secrets::SecretsTool;
pub use session::{
    SessionCreateRequest, SessionListFilter, SessionSearchQuery, SessionStore, SessionTool,
};
pub use skill::SkillTool;
pub use switch_model::SwitchModelTool;
pub use background_agent::{
    BackgroundAgentControlRequest, BackgroundAgentCreateRequest, BackgroundAgentMessageListRequest,
    BackgroundAgentMessageRequest, BackgroundAgentProgressRequest, BackgroundAgentStore,
    BackgroundAgentTool, BackgroundAgentUpdateRequest,
};
pub use telegram::{TelegramTool, send_telegram_notification};
pub use traits::{
    SecretResolver, SkillContent, SkillInfo, SkillProvider, SkillRecord, SkillUpdate, Tool,
    ToolOutput, ToolSchema,
};
pub use transcribe::TranscribeTool;
pub use vision::VisionTool;
pub use web_fetch::WebFetchTool;
pub use web_search::WebSearchTool;

/// Create a registry with default tools
pub fn default_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    let tracker = Arc::new(FileTracker::new());

    registry.register(BashTool::new());
    registry.register(FileTool::with_tracker(tracker.clone()));
    registry.register(PatchTool::new(tracker));
    registry.register(HttpTool::new());
    registry.register(PythonTool::new());
    registry.register(EmailTool::new());
    registry.register(TelegramTool::new());
    registry.register(WebSearchTool::new());
    registry.register(WebFetchTool::new());
    registry.register(JinaReaderTool::new());
    registry
}

/// Create a registry with default tools and diagnostics support.
pub fn default_registry_with_diagnostics(provider: Arc<dyn DiagnosticsProvider>) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    let tracker = Arc::new(FileTracker::new());

    registry.register(BashTool::new());
    registry.register(
        FileTool::with_tracker(tracker.clone()).with_diagnostics_provider(provider.clone()),
    );
    registry.register(PatchTool::new(tracker));
    registry.register(HttpTool::new());
    registry.register(PythonTool::new());
    registry.register(EmailTool::new());
    registry.register(TelegramTool::new());
    registry.register(DiagnosticsTool::new(provider));
    registry.register(WebSearchTool::new());
    registry.register(WebFetchTool::new());
    registry.register(JinaReaderTool::new());
    registry
}
