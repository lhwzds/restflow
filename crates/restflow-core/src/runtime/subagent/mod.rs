//! Sub-agent definitions and tracking.

pub mod definition;
pub mod spawn;
pub mod tracker;

pub use definition::{AgentDefinition, AgentDefinitionRegistry, builtin_agents};
pub use spawn::{SpawnHandle, SpawnPriority, SpawnRequest, SubagentConfig, spawn_subagent};
pub use tracker::{
    SubagentCompletion, SubagentResult, SubagentState, SubagentStatus, SubagentTracker,
};
