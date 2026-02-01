//! Agent context building utilities.
//!
//! Collects context from multiple sources and formats it for prompt injection.

use std::path::Path;

/// Skill summary for prompt injection.
#[derive(Debug, Clone)]
pub struct SkillSummary {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

/// Memory chunk for context injection.
#[derive(Debug, Clone)]
pub struct MemoryContext {
    pub content: String,
    pub score: f64,
}

/// Built context ready for injection.
#[derive(Debug, Default, Clone)]
pub struct AgentContext {
    /// Available skills (if any).
    pub skills: Vec<SkillSummary>,
    /// Relevant memories from search.
    pub memories: Vec<MemoryContext>,
    /// Content from workspace files (CLAUDE.md, AGENTS.md, etc.).
    pub workspace_context: Option<String>,
    /// Working directory path.
    pub workdir: Option<String>,
}

impl AgentContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_skills(mut self, skills: Vec<SkillSummary>) -> Self {
        self.skills = skills;
        self
    }

    pub fn with_memories(mut self, memories: Vec<MemoryContext>) -> Self {
        self.memories = memories;
        self
    }

    pub fn with_workspace_context(mut self, content: String) -> Self {
        self.workspace_context = Some(content);
        self
    }

    pub fn with_workdir(mut self, path: String) -> Self {
        self.workdir = Some(path);
        self
    }

    /// Format context for system prompt injection.
    pub fn format_for_prompt(&self) -> String {
        let mut sections = Vec::new();

        if !self.skills.is_empty() {
            let mut skill_section = String::from("## Available Skills\n\n");
            skill_section.push_str("Use the skill tool to read skill content before executing.\n\n");
            for skill in &self.skills {
                let desc = skill
                    .description
                    .as_deref()
                    .unwrap_or("No description");
                skill_section.push_str(&format!("- **{}** ({}): {}\n", skill.name, skill.id, desc));
            }
            sections.push(skill_section.trim_end().to_string());
        }

        if !self.memories.is_empty() {
            let mut memory_section = String::from("## Relevant Context\n\n");
            memory_section.push_str("From previous conversations and saved memories:\n\n");
            for mem in &self.memories {
                let content = if mem.content.len() > 500 {
                    format!("{}...", &mem.content[..500])
                } else {
                    mem.content.clone()
                };
                memory_section.push_str(&format!("> {}\n\n", content));
            }
            sections.push(memory_section.trim_end().to_string());
        }

        if let Some(ref ws_context) = self.workspace_context {
            let mut ws_section = String::from("## Workspace Instructions\n\n");
            let content = if ws_context.len() > 2000 {
                format!("{}...\n[truncated]", &ws_context[..2000])
            } else {
                ws_context.clone()
            };
            ws_section.push_str(&content);
            sections.push(ws_section.trim_end().to_string());
        }

        if let Some(ref workdir) = self.workdir {
            sections.push(format!("Working directory: {}", workdir));
        }

        sections.join("\n\n")
    }

    /// Check if context is empty.
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
            && self.memories.is_empty()
            && self.workspace_context.is_none()
            && self.workdir.is_none()
    }
}

/// Load workspace context file (CLAUDE.md, AGENTS.md, etc.).
pub fn load_workspace_context(workdir: &Path) -> Option<String> {
    let candidates = ["CLAUDE.md", "AGENTS.md", ".claude/instructions.md"];

    for filename in candidates {
        let path = workdir.join(filename);
        if path.exists()
            && let Ok(content) = std::fs::read_to_string(&path)
        {
            tracing::debug!(path = %path.display(), "Loaded workspace context");
            return Some(content);
        }
    }

    None
}
