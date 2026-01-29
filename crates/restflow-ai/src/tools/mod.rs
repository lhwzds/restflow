//! AI Tools module - Agent tool implementations
//!
//! This module provides tools that can be used by AI agents.
//! Tools implement the `Tool` trait for integration with the agent executor.

mod email;
mod file_memory;
mod http;
mod python;
mod registry;
mod skill;
mod telegram;
mod traits;

pub use email::EmailTool;
pub use file_memory::{
    DeleteMemoryTool, FileMemoryConfig, ListMemoryTool, MemoryEntry, MemoryEntryMeta,
    ReadMemoryTool, SaveMemoryTool,
};
pub use http::HttpTool;
pub use python::PythonTool;
pub use registry::ToolRegistry;
pub use skill::SkillTool;
pub use telegram::{TelegramTool, send_telegram_notification};
pub use traits::{SkillContent, SkillInfo, SkillProvider, Tool, ToolOutput, ToolSchema};

/// Create a registry with default tools
pub fn default_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(HttpTool::new());
    registry.register(PythonTool::new());
    registry.register(EmailTool::new());
    registry.register(TelegramTool::new());
    registry
}
