//! Agent type definitions for spawnable sub-agents.
//!
//! This module defines the available agent types that can be spawned
//! by the main agent, including their capabilities and system prompts.

use restflow_ai::agent::{SubagentDefLookup, SubagentDefSnapshot, SubagentDefSummary};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ts_rs::TS;

/// Agent definition describing a spawnable agent type
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentDefinition {
    /// Unique identifier (e.g., "researcher", "coder")
    pub id: String,

    /// Display name
    pub name: String,

    /// Description of when to use this agent
    pub description: String,

    /// System prompt for the agent
    pub system_prompt: String,

    /// List of allowed tool names
    pub allowed_tools: Vec<String>,

    /// Optional specific model to use
    pub model: Option<String>,

    /// Maximum iterations for ReAct loop
    pub max_iterations: Option<u32>,

    /// Whether this agent can be spawned by other agents
    pub callable: bool,

    /// Tags for categorization
    pub tags: Vec<String>,
}

/// Registry of available agent definitions
pub struct AgentDefinitionRegistry {
    definitions: HashMap<String, AgentDefinition>,
}

impl AgentDefinitionRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            definitions: HashMap::new(),
        }
    }

    /// Create a registry with built-in agent definitions
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();
        for def in builtin_agents() {
            registry.register(def);
        }
        registry
    }

    /// Register an agent definition
    pub fn register(&mut self, definition: AgentDefinition) {
        self.definitions.insert(definition.id.clone(), definition);
    }

    /// Get an agent definition by ID
    pub fn get(&self, id: &str) -> Option<&AgentDefinition> {
        self.definitions.get(id)
    }

    /// List all agent definitions
    pub fn list(&self) -> Vec<&AgentDefinition> {
        self.definitions.values().collect()
    }

    /// List callable agent definitions
    pub fn callable(&self) -> Vec<&AgentDefinition> {
        self.definitions.values().filter(|d| d.callable).collect()
    }

    /// Find agents by tag
    pub fn by_tag(&self, tag: &str) -> Vec<&AgentDefinition> {
        self.definitions
            .values()
            .filter(|d| d.tags.contains(&tag.to_string()))
            .collect()
    }
}

impl Default for AgentDefinitionRegistry {
    fn default() -> Self {
        Self::with_builtins()
    }
}

impl SubagentDefLookup for AgentDefinitionRegistry {
    fn lookup(&self, id: &str) -> Option<SubagentDefSnapshot> {
        self.get(id).map(|def| SubagentDefSnapshot {
            name: def.name.clone(),
            system_prompt: def.system_prompt.clone(),
            allowed_tools: def.allowed_tools.clone(),
            max_iterations: def.max_iterations,
            default_model: def.model.clone(),
        })
    }

    fn list_callable(&self) -> Vec<SubagentDefSummary> {
        self.callable()
            .into_iter()
            .map(|def| SubagentDefSummary {
                id: def.id.clone(),
                name: def.name.clone(),
                description: def.description.clone(),
                tags: def.tags.clone(),
            })
            .collect()
    }
}

/// Built-in agent definitions.
/// These are now minimal placeholders - actual prompts are loaded from ~/.restflow/agents/.
/// The registry is populated from database records at runtime.
pub fn builtin_agents() -> Vec<AgentDefinition> {
    vec![]
}

mod tests {
    

    #[test]
    fn test_builtin_agents_empty() {
        // No built-in agents - they are loaded from ~/.restflow/agents/ at runtime
        let agents = builtin_agents();
        assert!(agents.is_empty());
    }

    #[test]
    fn test_registry_empty() {
        let registry = AgentDefinitionRegistry::with_builtins();
        // No built-in agents
        assert!(registry.list().is_empty());
        assert!(registry.callable().is_empty());
    }

    #[test]
    fn test_registry_by_tag_empty() {
        let registry = AgentDefinitionRegistry::with_builtins();
        let coding_agents = registry.by_tag("coding");
        assert!(coding_agents.is_empty());
    }
}
