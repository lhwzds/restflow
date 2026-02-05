//! Agent context building utilities.
//!
//! Collects context from multiple sources and formats it for prompt injection.

use once_cell::sync::OnceCell;
use std::collections::HashSet;
use std::fs;
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
            skill_section
                .push_str("Use the skill tool to read skill content before executing.\n\n");
            for skill in &self.skills {
                let desc = skill.description.as_deref().unwrap_or("No description");
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

/// Default workspace context paths to scan (order matters).
const DEFAULT_CONTEXT_PATHS: &[&str] = &[
    "CLAUDE.md",
    "CLAUDE.local.md",
    ".claude/",
    "AGENTS.md",
    "AGENTS.local.md",
    ".restflow/instructions.md",
    ".cursorrules",
    ".cursor/rules/",
    ".github/copilot-instructions.md",
    "AI_INSTRUCTIONS.md",
];

static CONTEXT_CONTENT: OnceCell<String> = OnceCell::new();

/// Load workspace context files once per process.
pub fn get_project_context(workdir: &Path) -> &str {
    CONTEXT_CONTENT.get_or_init(|| load_context(workdir, DEFAULT_CONTEXT_PATHS))
}

/// Load workspace context file content if present.
pub fn load_workspace_context(workdir: &Path) -> Option<String> {
    let context = get_project_context(workdir);
    if context.trim().is_empty() {
        None
    } else {
        Some(context.to_string())
    }
}

fn load_context(workdir: &Path, paths: &[&str]) -> String {
    let mut seen: HashSet<String> = HashSet::new();
    let mut sections: Vec<String> = Vec::new();

    for path_pattern in paths {
        let full_path = workdir.join(path_pattern);
        let lower = full_path.to_string_lossy().to_lowercase();
        if seen.contains(&lower) {
            continue;
        }
        seen.insert(lower);

        if path_pattern.ends_with('/') {
            if let Ok(entries) = fs::read_dir(&full_path) {
                let mut files: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
                files.sort();
                for path in files {
                    if !path.is_file() || !is_text_file(&path) {
                        continue;
                    }
                    let lower = path.to_string_lossy().to_lowercase();
                    if seen.contains(&lower) {
                        continue;
                    }
                    seen.insert(lower);

                    if let Ok(content) = fs::read_to_string(&path) {
                        let relative = path.strip_prefix(workdir).unwrap_or(&path);
                        sections.push(format!(
                            "# From: {}\n\n{}",
                            relative.display(),
                            content.trim()
                        ));
                    }
                }
            }
        } else if full_path.is_file() {
            if let Ok(content) = fs::read_to_string(&full_path) {
                let relative = full_path.strip_prefix(workdir).unwrap_or(&full_path);
                sections.push(format!(
                    "# From: {}\n\n{}",
                    relative.display(),
                    content.trim()
                ));
            }
        }
    }

    if sections.is_empty() {
        return String::new();
    }

    format!(
        "# Project-Specific Instructions\n\nFollow the instructions below:\n\n{}",
        sections.join("\n\n---\n\n")
    )
}

fn is_text_file(path: &Path) -> bool {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("md") | Some("txt") | Some("markdown") => true,
        _ => path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == ".cursorrules"),
    }
}
