//! Sub-agent definitions and tracking.

pub mod definition;
pub mod spawn;
pub mod tracker;

pub use definition::{builtin_agents, AgentDefinition, AgentDefinitionRegistry};
pub use spawn::{SpawnHandle, SpawnPriority, SpawnRequest, SubagentConfig, spawn_subagent};
pub use tracker::{SubagentCompletion, SubagentResult, SubagentState, SubagentStatus, SubagentTracker};
