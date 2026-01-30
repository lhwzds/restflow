//! Main agent tools for sub-agent management and skill loading.
//!
//! This module provides tools that the main agent can use:
//! - `spawn_agent`: Spawn a sub-agent to work on a task in parallel
//! - `wait_agents`: Wait for one or more sub-agents to complete
//! - `list_agents`: List available agent types and running agents
//! - `use_skill`: Load and activate a skill

pub mod list_agents;
pub mod spawn_agent;
pub mod use_skill;
pub mod wait_agents;

pub use list_agents::ListAgentsTool;
pub use spawn_agent::SpawnAgentTool;
pub use use_skill::UseSkillTool;
pub use wait_agents::WaitAgentsTool;
