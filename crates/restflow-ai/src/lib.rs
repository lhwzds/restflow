//! RestFlow AI - Rust-powered AI Agent Framework
//!
//! This crate provides:
//! - Dynamic workflow definition (WorkflowDef)
//! - Graph engine with runtime decisions
//! - Agent loop (ReAct pattern)
//! - Tool registry and execution
//! - Evaluation engine
//! - Python SDK via PyO3
//!
//! NOTE: This crate is currently a placeholder structure.
//! Full implementation will be added in future iterations.

pub mod agent;
pub mod eval;
pub mod graph;
pub mod tools;
pub mod workflow_def;

#[cfg(feature = "python")]
pub mod python;

// Re-export commonly used types
pub use graph::{Graph, GraphNode};
pub use workflow_def::WorkflowDef;

// Re-export tools from this crate
pub use tools::{AddTool, GetTimeTool};
