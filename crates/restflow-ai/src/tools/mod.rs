//! AI Tools module - Agent tool implementations
//!
//! This module provides tools that can be used by AI agents.
//! Tools implement the `Tool` trait for integration with the agent executor.

mod email;
mod http;
mod python;
mod registry;
mod skill;
mod traits;

pub use email::EmailTool;
pub use http::HttpTool;
pub use python::PythonTool;
pub use registry::ToolRegistry;
pub use skill::SkillTool;
pub use traits::{SkillContent, SkillInfo, SkillProvider, Tool, ToolOutput, ToolSchema};

/// Create a registry with default tools
pub fn default_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(HttpTool::new());
    registry.register(PythonTool::new());
    registry.register(EmailTool::new());
    registry
}
