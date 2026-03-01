//! Sub-agent definitions.
//!
//! Runtime implementations (SubagentTracker, spawn_subagent) live in restflow-ai.

pub mod definition;

pub use definition::{
    AgentDefinition, AgentDefinitionRegistry, StorageBackedSubagentLookup, builtin_agents,
};
