//! Agent tools module for rig-core integration
//!
//! These tools implement the rig::tool::Tool trait for use with LLM agents.

mod math_tools;
mod time_tool;

pub use math_tools::AddTool;
pub use time_tool::GetTimeTool;
