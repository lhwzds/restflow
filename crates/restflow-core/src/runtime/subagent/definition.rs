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

/// Built-in agent definitions
pub fn builtin_agents() -> Vec<AgentDefinition> {
    vec![
        AgentDefinition {
            id: "researcher".to_string(),
            name: "Researcher".to_string(),
            description: "Conducts research, gathers information, and synthesizes findings. \
                         Use for tasks requiring information gathering and analysis."
                .to_string(),
            system_prompt: compose_subagent_system_prompt(RESEARCHER_PROMPT),
            allowed_tools: vec!["http_request".to_string(), "read".to_string()],
            model: None,
            max_iterations: Some(15),
            callable: true,
            tags: vec!["research".to_string(), "analysis".to_string()],
        },
        AgentDefinition {
            id: "coder".to_string(),
            name: "Coder".to_string(),
            description: "Writes, modifies, and debugs code. \
                         Use for programming tasks and code generation."
                .to_string(),
            system_prompt: compose_subagent_system_prompt(CODER_PROMPT),
            allowed_tools: vec![
                "read".to_string(),
                "write".to_string(),
                "bash".to_string(),
                "grep".to_string(),
            ],
            model: None,
            max_iterations: Some(20),
            callable: true,
            tags: vec!["coding".to_string(), "programming".to_string()],
        },
        AgentDefinition {
            id: "reviewer".to_string(),
            name: "Reviewer".to_string(),
            description: "Reviews code, documents, or content for quality and issues. \
                         Use for review and quality assurance tasks."
                .to_string(),
            system_prompt: compose_subagent_system_prompt(REVIEWER_PROMPT),
            allowed_tools: vec!["read".to_string(), "grep".to_string()],
            model: None,
            max_iterations: Some(10),
            callable: true,
            tags: vec!["review".to_string(), "quality".to_string()],
        },
        AgentDefinition {
            id: "writer".to_string(),
            name: "Writer".to_string(),
            description: "Creates written content, documentation, and reports. \
                         Use for content creation and documentation tasks."
                .to_string(),
            system_prompt: compose_subagent_system_prompt(WRITER_PROMPT),
            allowed_tools: vec!["read".to_string(), "write".to_string()],
            model: None,
            max_iterations: Some(10),
            callable: true,
            tags: vec!["writing".to_string(), "documentation".to_string()],
        },
        AgentDefinition {
            id: "analyst".to_string(),
            name: "Analyst".to_string(),
            description: "Analyzes data and provides insights. \
                         Use for data analysis and interpretation tasks."
                .to_string(),
            system_prompt: compose_subagent_system_prompt(ANALYST_PROMPT),
            allowed_tools: vec!["read".to_string(), "python".to_string()],
            model: None,
            max_iterations: Some(15),
            callable: true,
            tags: vec!["analysis".to_string(), "data".to_string()],
        },
    ]
}

// Agent system prompts — loaded from .md files at compile time

const DEFAULT_MAIN_AGENT_PROMPT: &str = include_str!("../../../assets/agents/default_agent.md");
const RESEARCHER_PROMPT: &str = include_str!("../../../assets/agents/subagent_researcher.md");
const CODER_PROMPT: &str = include_str!("../../../assets/agents/subagent_coder.md");
const REVIEWER_PROMPT: &str = include_str!("../../../assets/agents/subagent_reviewer.md");
const WRITER_PROMPT: &str = include_str!("../../../assets/agents/subagent_writer.md");
const ANALYST_PROMPT: &str = include_str!("../../../assets/agents/subagent_analyst.md");

fn compose_subagent_system_prompt(role_prompt: &str) -> String {
    format!(
        "{base}\n\n## Subagent Role Override\n\n{role}\n\nWhen role-specific instructions conflict with the base prompt, follow role-specific instructions.",
        base = DEFAULT_MAIN_AGENT_PROMPT.trim(),
        role = role_prompt.trim(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_agents() {
        let agents = builtin_agents();
        assert!(!agents.is_empty());

        // Check researcher exists
        assert!(agents.iter().any(|a| a.id == "researcher"));

        // Check coder exists
        assert!(agents.iter().any(|a| a.id == "coder"));
    }

    #[test]
    fn test_registry() {
        let registry = AgentDefinitionRegistry::with_builtins();

        assert!(registry.get("researcher").is_some());
        assert!(registry.get("nonexistent").is_none());

        let callable = registry.callable();
        assert!(!callable.is_empty());
        assert!(callable.iter().all(|a| a.callable));
    }

    #[test]
    fn test_registry_by_tag() {
        let registry = AgentDefinitionRegistry::with_builtins();

        let coding_agents = registry.by_tag("coding");
        assert!(!coding_agents.is_empty());
        assert!(coding_agents.iter().any(|a| a.id == "coder"));
    }

    #[test]
    fn test_subagent_prompt_extends_default_prompt() {
        let agents = builtin_agents();
        let coder = agents
            .iter()
            .find(|agent| agent.id == "coder")
            .expect("coder agent should exist");

        assert!(
            coder
                .system_prompt
                .contains("You are a helpful AI assistant powered by RestFlow")
        );
        assert!(
            coder
                .system_prompt
                .contains("You are an expert coding agent.")
        );
        assert!(coder.system_prompt.contains("Subagent Role Override"));
    }
}
