//! Agent type definitions for spawnable sub-agents.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Agent definition describing a spawnable agent type
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Built-in agent definitions
pub fn builtin_agents() -> Vec<AgentDefinition> {
    vec![
        AgentDefinition {
            id: "researcher".to_string(),
            name: "Researcher".to_string(),
            description: "Conducts research, gathers information, and synthesizes findings. \
                         Use for tasks requiring information gathering and analysis."
                .to_string(),
            system_prompt: RESEARCHER_PROMPT.to_string(),
            allowed_tools: vec!["http_request".to_string(), "file".to_string()],
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
            system_prompt: CODER_PROMPT.to_string(),
            allowed_tools: vec![
                "file".to_string(),
                "bash".to_string(),
                "http_request".to_string(),
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
            system_prompt: REVIEWER_PROMPT.to_string(),
            allowed_tools: vec!["file".to_string(), "http_request".to_string()],
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
            system_prompt: WRITER_PROMPT.to_string(),
            allowed_tools: vec!["file".to_string()],
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
            system_prompt: ANALYST_PROMPT.to_string(),
            allowed_tools: vec!["file".to_string(), "bash".to_string()],
            model: None,
            max_iterations: Some(15),
            callable: true,
            tags: vec!["analysis".to_string(), "data".to_string()],
        },
    ]
}

const RESEARCHER_PROMPT: &str = r#"You are a skilled researcher agent.

Your capabilities:
- Gather information from various sources
- Synthesize findings into coherent summaries
- Identify key insights and patterns
- Provide well-sourced, accurate information

Guidelines:
- Be thorough but focused on the specific research question
- Cite sources when possible
- Distinguish between facts and opinions
- Acknowledge uncertainty when information is incomplete
- Structure your findings clearly

When given a research task, break it down into:
1. Key questions to answer
2. Information gathering steps
3. Synthesis and analysis
4. Clear conclusions with supporting evidence
"#;

const CODER_PROMPT: &str = r#"You are an expert coding agent.

Your capabilities:
- Write clean, efficient, well-documented code
- Debug and fix issues
- Refactor and improve existing code
- Implement features according to specifications

Guidelines:
- Follow language best practices and conventions
- Write readable, maintainable code
- Include appropriate error handling
- Add comments for complex logic
- Test your changes when possible

When given a coding task:
1. Understand the requirements fully
2. Plan your approach
3. Implement incrementally
4. Verify your changes work correctly
5. Clean up and document
"#;

const REVIEWER_PROMPT: &str = r#"You are a thorough code/content reviewer agent.

Your capabilities:
- Identify bugs, issues, and potential problems
- Suggest improvements for quality and maintainability
- Check for security vulnerabilities
- Verify adherence to best practices

Guidelines:
- Be constructive in your feedback
- Prioritize issues by severity
- Explain why something is problematic
- Suggest specific fixes when possible
- Acknowledge good practices too

When reviewing:
1. Understand the context and purpose
2. Systematically examine the content
3. Categorize issues (critical, major, minor)
4. Provide actionable feedback
5. Summarize overall assessment
"#;

const WRITER_PROMPT: &str = r#"You are a skilled writing agent.

Your capabilities:
- Create clear, engaging written content
- Write technical documentation
- Draft reports and summaries
- Adapt tone and style to audience

Guidelines:
- Write clearly and concisely
- Structure content logically
- Use appropriate formatting
- Maintain consistent style
- Proofread for errors

When creating content:
1. Understand the audience and purpose
2. Outline the structure
3. Draft the content
4. Review and refine
5. Final polish
"#;

const ANALYST_PROMPT: &str = r#"You are a data analysis agent.

Your capabilities:
- Analyze datasets and extract insights
- Create visualizations when helpful
- Perform statistical analysis
- Identify trends and patterns

Guidelines:
- Be rigorous in your analysis
- Validate data quality
- Use appropriate methods
- Present findings clearly
- Acknowledge limitations

When analyzing data:
1. Understand the question being asked
2. Explore and validate the data
3. Apply appropriate analysis methods
4. Interpret results in context
5. Present clear conclusions
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_agents() {
        let agents = builtin_agents();
        assert!(!agents.is_empty());
        assert!(agents.iter().any(|a| a.id == "researcher"));
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
}
