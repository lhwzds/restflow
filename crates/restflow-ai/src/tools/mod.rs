//! AI Tools module - Agent tool implementations using rig
//!
//! This module provides tools that can be used by AI agents.
//! Tools implement rig's `Tool` trait for integration with LLM providers.

pub mod math_tools;
pub mod time_tool;

pub use math_tools::AddTool;
pub use time_tool::GetTimeTool;
