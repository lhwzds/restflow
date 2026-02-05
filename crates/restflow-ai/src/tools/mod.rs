//! AI Tools module - Agent tool implementations
//!
//! This module provides tools that can be used by AI agents.
//! Tools implement the `Tool` trait for integration with the agent executor.

mod bash;
mod email;
mod file;
mod file_memory;
mod file_tracker;
mod http;
mod mcp_cache;
mod memory_search;
mod process;
mod python;
mod registry;
mod skill;
mod telegram;
mod traits;

pub use bash::{BashInput, BashOutput, BashTool};
pub use email::EmailTool;
pub use file::{FileAction, FileTool};
pub use file_memory::{
    DeleteMemoryTool, FileMemoryConfig, ListMemoryTool, MemoryEntry, MemoryEntryMeta,
    ReadMemoryTool, SaveMemoryTool,
};
pub use http::HttpTool;
pub use mcp_cache::{McpServerConfig, get_mcp_tools, invalidate_mcp_cache};
pub use memory_search::{MemorySearchMatch, MemorySearchTool, SemanticMemory};
pub use process::{ProcessLog, ProcessManager, ProcessPollResult, ProcessSessionInfo, ProcessTool};
pub use python::PythonTool;
pub use registry::ToolRegistry;
pub use skill::SkillTool;
pub use telegram::{TelegramTool, send_telegram_notification};
pub use traits::{SkillContent, SkillInfo, SkillProvider, Tool, ToolOutput, ToolSchema};

/// Create a registry with default tools
pub fn default_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(BashTool::new());
    registry.register(FileTool::new());
    registry.register(HttpTool::new());
    registry.register(PythonTool::new());
    registry.register(EmailTool::new());
    registry.register(TelegramTool::new());
    registry
}
