//! AI Tools module - Agent tool implementations
//!
//! This module provides tools that can be used by AI agents.
//! Tools implement the `Tool` trait for integration with the agent executor.

mod bash;
mod email;
mod file;
mod http;
mod python;
mod registry;
mod skill;
mod telegram;
mod traits;

pub use bash::{BashTool, BashInput, BashOutput};
pub use email::EmailTool;
pub use file::{FileTool, FileAction};
pub use http::HttpTool;
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
