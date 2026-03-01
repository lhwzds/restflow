//! Agent type definitions for spawnable sub-agents.
//!
//! This module defines the available agent types that can be spawned
//! by the main agent, including their capabilities and system prompts.

use crate::runtime::agent::main_agent_default_tool_names;
use crate::storage::{AgentStorage, agent::StoredAgent};
use restflow_ai::agent::{SubagentDefLookup, SubagentDefSnapshot, SubagentDefSummary};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::warn;
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
#[derive(Clone)]
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

    /// Build a registry from persisted agents in storage.
    pub fn from_agents(agents: &[StoredAgent]) -> Self {
        let mut registry = Self::new();
        for stored in agents {
            registry.register(Self::from_stored_agent(stored));
        }
        registry
    }

    /// Register an agent definition
    pub fn register(&mut self, definition: AgentDefinition) {
        self.definitions.insert(definition.id.clone(), definition);
    }

    /// Get an agent definition by ID
    pub fn get(&self, id: &str) -> Option<&AgentDefinition> {
        let query = id.trim();
        if query.is_empty() {
            return None;
        }

        if let Some(definition) = self.definitions.get(query) {
            return Some(definition);
        }

        let prefix_matches: Vec<&AgentDefinition> = self
            .definitions
            .values()
            .filter(|definition| definition.id.starts_with(query))
            .collect();
        if prefix_matches.len() == 1 {
            return prefix_matches.first().copied();
        }

        let normalized_query = normalize_identifier(query);
        if normalized_query.is_empty() {
            return None;
        }

        let normalized_matches: Vec<&AgentDefinition> = self
            .definitions
            .values()
            .filter(|definition| {
                normalize_identifier(&definition.id) == normalized_query
                    || normalize_identifier(&definition.name) == normalized_query
            })
            .collect();
        if normalized_matches.len() == 1 {
            return normalized_matches.first().copied();
        }

        None
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

    fn from_stored_agent(stored: &StoredAgent) -> AgentDefinition {
        let default_tools = main_agent_default_tool_names();
        let allowed_tools = stored
            .agent
            .tools
            .clone()
            .filter(|tools| !tools.is_empty())
            .unwrap_or(default_tools);
        let prompt = stored
            .agent
            .prompt
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| format!("You are {}.", stored.name));
        let model = stored
            .agent
            .model
            .as_ref()
            .map(|value| value.as_serialized_str().to_string());

        AgentDefinition {
            id: stored.id.clone(),
            name: stored.name.clone(),
            description: summarize_prompt(stored.agent.prompt.as_deref()),
            system_prompt: prompt,
            allowed_tools,
            model,
            max_iterations: None,
            callable: true,
            tags: vec!["stored".to_string()],
        }
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

/// Dynamic sub-agent lookup backed by persisted agent storage.
///
/// This keeps `spawn_agent` definitions in sync with runtime agent CRUD
/// without requiring daemon restart.
#[derive(Clone)]
pub struct StorageBackedSubagentLookup {
    agent_storage: AgentStorage,
    fallback: AgentDefinitionRegistry,
}

impl StorageBackedSubagentLookup {
    pub fn new(agent_storage: AgentStorage) -> Self {
        Self {
            agent_storage,
            fallback: AgentDefinitionRegistry::with_builtins(),
        }
    }

    fn load_registry(&self) -> Option<AgentDefinitionRegistry> {
        match self.agent_storage.list_agents() {
            Ok(agents) => Some(AgentDefinitionRegistry::from_agents(&agents)),
            Err(error) => {
                warn!(error = %error, "Failed to load sub-agent definitions from storage");
                None
            }
        }
    }
}

impl SubagentDefLookup for StorageBackedSubagentLookup {
    fn lookup(&self, id: &str) -> Option<SubagentDefSnapshot> {
        if let Some(registry) = self.load_registry()
            && let Some(snapshot) = registry.lookup(id)
        {
            return Some(snapshot);
        }
        self.fallback.lookup(id)
    }

    fn list_callable(&self) -> Vec<SubagentDefSummary> {
        if let Some(registry) = self.load_registry() {
            let callable = registry.list_callable();
            if !callable.is_empty() {
                return callable;
            }
        }
        self.fallback.list_callable()
    }
}

fn summarize_prompt(prompt: Option<&str>) -> String {
    let Some(prompt) = prompt else {
        return "Stored agent definition".to_string();
    };

    let first_line = prompt
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .trim_start_matches('#')
        .trim();
    if first_line.is_empty() {
        return "Stored agent definition".to_string();
    }

    if first_line.chars().count() <= 120 {
        first_line.to_string()
    } else {
        format!("{}...", first_line.chars().take(120).collect::<String>())
    }
}

fn normalize_identifier(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut previous_dash = false;

    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
            previous_dash = false;
            continue;
        }

        if !previous_dash {
            normalized.push('-');
            previous_dash = true;
        }
    }

    normalized.trim_matches('-').to_string()
}

/// Built-in agent definitions.
/// These are now minimal placeholders - actual prompts are loaded from ~/.restflow/agents/.
/// The registry is populated from database records at runtime.
pub fn builtin_agents() -> Vec<AgentDefinition> {
    vec![]
}

#[cfg(test)]
mod tests {
    use super::{AgentDefinitionRegistry, builtin_agents};
    use crate::models::{AIModel, AgentNode};
    use crate::storage::agent::StoredAgent;
    use restflow_ai::agent::SubagentDefLookup;

    fn stored_agent(
        id: &str,
        name: &str,
        prompt: Option<&str>,
        tools: Option<Vec<String>>,
        model: Option<AIModel>,
    ) -> StoredAgent {
        StoredAgent {
            id: id.to_string(),
            name: name.to_string(),
            agent: AgentNode {
                model,
                prompt: prompt.map(str::to_string),
                tools,
                ..Default::default()
            },
            prompt_file: None,
            created_at: None,
            updated_at: None,
        }
    }

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

    #[test]
    fn test_registry_from_agents_supports_id_and_name_lookup() {
        let stored = stored_agent(
            "agent-1",
            "Research Coder",
            Some("# Research specialist\nFocus on code and docs"),
            Some(vec!["web_search".to_string(), "file".to_string()]),
            Some(AIModel::MiniMaxM25CodingPlan),
        );
        let registry = AgentDefinitionRegistry::from_agents(&[stored]);

        assert!(registry.get("agent-1").is_some());
        assert!(registry.get("Research Coder").is_some());
        assert!(registry.get("research-coder").is_some());

        let snapshot = registry.lookup("research-coder").unwrap();
        assert_eq!(
            snapshot.default_model.as_deref(),
            Some("minimax-coding-plan-m2-5")
        );
        assert!(snapshot.allowed_tools.contains(&"web_search".to_string()));
    }

    #[test]
    fn test_registry_from_agents_falls_back_to_default_tools() {
        let stored = stored_agent("agent-2", "No Tool Agent", Some("Prompt"), None, None);
        let registry = AgentDefinitionRegistry::from_agents(&[stored]);
        let snapshot = registry.lookup("agent-2").unwrap();
        assert!(!snapshot.allowed_tools.is_empty());
    }

    #[test]
    fn test_name_lookup_returns_none_when_ambiguous() {
        let agents = vec![
            stored_agent("a-1", "Data Reviewer", Some("Prompt A"), None, None),
            stored_agent("a-2", "data-reviewer", Some("Prompt B"), None, None),
        ];
        let registry = AgentDefinitionRegistry::from_agents(&agents);
        assert!(registry.get("data-reviewer").is_none());
    }
}
