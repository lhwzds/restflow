//! Sub-agent definitions and tracking.
//!
//! Canonical implementations for definition and tracker live in restflow-core.
//! Only spawn.rs remains local (Tauri uses simple LLM-only execution).

pub mod spawn;

pub use restflow_core::runtime::subagent::{
    AgentDefinition, AgentDefinitionRegistry, SubagentCompletion, SubagentConfig, SubagentResult,
    SubagentState, SubagentStatus, SubagentTracker, builtin_agents,
};
pub use spawn::{SpawnHandle, SpawnPriority, SpawnRequest, spawn_subagent};
