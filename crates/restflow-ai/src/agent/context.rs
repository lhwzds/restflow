//! Agent context building utilities.
//!
//! Collects context from multiple sources and formats it for prompt injection.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::{debug, warn};

use crate::text_utils::floor_char_boundary;

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
                    let end = floor_char_boundary(&mem.content, 500);
                    format!("{}...", &mem.content[..end])
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
                let end = floor_char_boundary(ws_context, 2000);
                format!("{}...\n[truncated]", &ws_context[..end])
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

/// Configuration for workspace context discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextDiscoveryConfig {
    /// List of paths to search (files or directories).
    pub paths: Vec<PathBuf>,
    /// Whether to recursively scan directories.
    pub scan_directories: bool,
    /// Case-insensitive deduplication.
    pub case_insensitive_dedup: bool,
    /// Maximum total size of loaded context (bytes).
    pub max_total_size: usize,
    /// Maximum size per file (bytes).
    pub max_file_size: usize,
}

impl Default for ContextDiscoveryConfig {
    fn default() -> Self {
        Self {
            paths: vec![
                // Claude/Anthropic
                "CLAUDE.md".into(),
                "CLAUDE.local.md".into(),
                ".claude/".into(),
                // RestFlow specific
                "AGENTS.md".into(),
                "AGENTS.local.md".into(),
                ".restflow/instructions.md".into(),
                // Cursor
                ".cursorrules".into(),
                ".cursor/rules/".into(),
                // GitHub Copilot
                ".github/copilot-instructions.md".into(),
                // OpenCode compatibility
                "opencode.md".into(),
                "OpenCode.md".into(),
                // Generic
                "AI_INSTRUCTIONS.md".into(),
                ".ai/instructions.md".into(),
            ],
            scan_directories: true,
            case_insensitive_dedup: true,
            max_total_size: 100_000,
            max_file_size: 50_000,
        }
    }
}

/// Result of context discovery.
#[derive(Debug, Clone)]
pub struct DiscoveredContext {
    /// Combined context content.
    pub content: String,
    /// List of loaded files.
    pub loaded_files: Vec<PathBuf>,
    /// Total bytes loaded.
    pub total_bytes: usize,
}

/// Loads workspace context from configured paths.
pub struct ContextLoader {
    config: ContextDiscoveryConfig,
    workdir: PathBuf,
}

impl ContextLoader {
    pub fn new(config: ContextDiscoveryConfig, workdir: PathBuf) -> Self {
        Self { config, workdir }
    }

    /// Discover and load all context files.
    pub async fn load(&self) -> DiscoveredContext {
        let mut seen_paths: HashSet<String> = HashSet::new();
        let mut contents: Vec<(PathBuf, String)> = Vec::new();
        let mut total_bytes = 0usize;

        for path_pattern in &self.config.paths {
            let full_path = if path_pattern.is_absolute() {
                path_pattern.clone()
            } else {
                self.workdir.join(path_pattern)
            };

            match fs::metadata(&full_path).await {
                Ok(meta) if meta.is_dir() && self.config.scan_directories => {
                    if let Ok(dir_contents) = self.scan_directory(&full_path).await {
                        for (file_path, content) in dir_contents {
                            if self.is_duplicate(&mut seen_paths, &file_path) {
                                continue;
                            }
                            if total_bytes + content.len() <= self.config.max_total_size {
                                total_bytes += content.len();
                                contents.push((file_path, content));
                            }
                        }
                    }
                }
                Ok(meta) if meta.is_file() => {
                    if self.is_duplicate(&mut seen_paths, &full_path) {
                        continue;
                    }
                    if let Ok(content) = self.load_file(&full_path).await
                        && total_bytes + content.len() <= self.config.max_total_size
                    {
                        total_bytes += content.len();
                        contents.push((full_path, content));
                    }
                }
                _ => {
                    debug!(path = %full_path.display(), "Context path not found, skipping");
                }
            }
        }

        contents = self.prioritize_instruction_sources(contents);
        total_bytes = contents.iter().map(|(_, content)| content.len()).sum();

        let loaded_files: Vec<PathBuf> = contents.iter().map(|(p, _)| p.clone()).collect();
        let content = self.format_content(&contents);

        DiscoveredContext {
            content,
            loaded_files,
            total_bytes,
        }
    }

    fn prioritize_instruction_sources(
        &self,
        contents: Vec<(PathBuf, String)>,
    ) -> Vec<(PathBuf, String)> {
        let has_agents_instructions = contents
            .iter()
            .any(|(path, _)| Self::is_agents_instruction_file(path));
        if !has_agents_instructions {
            return contents;
        }

        let prioritized: Vec<(PathBuf, String)> = contents
            .into_iter()
            .filter(|(path, _)| Self::is_agents_instruction_file(path))
            .collect();
        debug!(
            selected_files = prioritized.len(),
            "AGENTS instructions found; skipping fallback instruction files"
        );
        prioritized
    }

    fn is_agents_instruction_file(path: &Path) -> bool {
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            return false;
        };
        file_name.eq_ignore_ascii_case("AGENTS.md")
            || file_name.eq_ignore_ascii_case("AGENTS.local.md")
    }

    async fn scan_directory(&self, dir: &Path) -> Result<Vec<(PathBuf, String)>, std::io::Error> {
        let mut results = Vec::new();
        let mut pending = vec![dir.to_path_buf()];

        while let Some(next_dir) = pending.pop() {
            let mut entries = fs::read_dir(&next_dir).await?;

            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                let meta = entry.metadata().await?;

                if meta.is_dir() {
                    if self.config.scan_directories {
                        pending.push(path);
                    }
                    continue;
                }

                if meta.is_file()
                    && self.should_load_path(&path)
                    && let Ok(content) = self.load_file(&path).await
                {
                    results.push((path, content));
                }
            }
        }

        Ok(results)
    }

    async fn load_file(&self, path: &Path) -> Result<String, std::io::Error> {
        let meta = fs::metadata(path).await?;
        if meta.len() as usize > self.config.max_file_size {
            warn!(
                path = %path.display(),
                size = meta.len(),
                max = self.config.max_file_size,
                "Context file too large, skipping"
            );
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "File too large",
            ));
        }
        fs::read_to_string(path).await
    }

    fn should_load_path(&self, path: &Path) -> bool {
        let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
            return false;
        };
        matches!(ext.to_lowercase().as_str(), "md" | "markdown" | "txt")
    }

    fn format_content(&self, contents: &[(PathBuf, String)]) -> String {
        if contents.is_empty() {
            return String::new();
        }

        let mut result = String::new();

        for (path, content) in contents {
            let relative = path.strip_prefix(&self.workdir).unwrap_or(path);
            result.push_str(&format!("## From: {}\n\n", relative.display()));
            result.push_str(content.trim());
            result.push_str("\n\n---\n\n");
        }

        result
    }

    fn normalize_path(&self, path: &Path) -> String {
        path.to_string_lossy().to_string()
    }

    fn is_duplicate(&self, seen: &mut HashSet<String>, path: &Path) -> bool {
        let normalized = self.normalize_path(path);
        let key = if self.config.case_insensitive_dedup {
            normalized.to_lowercase()
        } else {
            normalized
        };

        if seen.contains(&key) {
            true
        } else {
            seen.insert(key);
            false
        }
    }
}

/// Cached workspace context.
pub struct WorkspaceContextCache {
    cache: tokio::sync::OnceCell<std::sync::Arc<DiscoveredContext>>,
    loader: ContextLoader,
}

impl WorkspaceContextCache {
    pub fn new(config: ContextDiscoveryConfig, workdir: PathBuf) -> Self {
        Self {
            cache: tokio::sync::OnceCell::new(),
            loader: ContextLoader::new(config, workdir),
        }
    }

    pub async fn get(&self) -> std::sync::Arc<DiscoveredContext> {
        self.cache
            .get_or_init(|| async { std::sync::Arc::new(self.loader.load().await) })
            .await
            .clone()
    }

    pub fn invalidate(&mut self) {
        self.cache = tokio::sync::OnceCell::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_floor_char_boundary_ascii() {
        let s = "hello world";
        assert_eq!(floor_char_boundary(s, 5), 5);
    }

    #[test]
    fn test_floor_char_boundary_multibyte() {
        // CJK char '你' is 3 bytes in UTF-8
        let s = "你好世界";
        // byte index 1 is mid-char, should back up to 0
        assert_eq!(floor_char_boundary(s, 1), 0);
        // byte index 4 is mid-char of '好', should back up to 3
        assert_eq!(floor_char_boundary(s, 4), 3);
    }

    #[test]
    fn test_floor_char_boundary_at_len() {
        let s = "hello";
        assert_eq!(floor_char_boundary(s, 100), s.len());
        assert_eq!(floor_char_boundary(s, s.len()), s.len());
    }

    #[test]
    fn test_format_for_prompt_truncates_long_memory() {
        let long_content = "a".repeat(600);
        let ctx = AgentContext::new().with_memories(vec![MemoryContext {
            content: long_content,
            score: 1.0,
        }]);
        let result = ctx.format_for_prompt();
        // The memory section should contain "..." indicating truncation
        assert!(result.contains("..."));
        // Should not contain the full 600-char string
        assert!(!result.contains(&"a".repeat(600)));
    }

    #[test]
    fn test_format_for_prompt_truncates_long_workspace() {
        let long_content = "b".repeat(3000);
        let ctx = AgentContext::new().with_workspace_context(long_content);
        let result = ctx.format_for_prompt();
        assert!(result.contains("[truncated]"));
        assert!(!result.contains(&"b".repeat(3000)));
    }

    #[test]
    fn test_format_for_prompt_multibyte_safe() {
        // Create content with CJK chars that exceeds 500 bytes
        let long_cjk = "你".repeat(200); // 200 * 3 = 600 bytes
        let ctx = AgentContext::new().with_memories(vec![MemoryContext {
            content: long_cjk,
            score: 1.0,
        }]);
        // Should not panic
        let result = ctx.format_for_prompt();
        assert!(result.contains("..."));
    }

    #[tokio::test]
    async fn test_context_loader_prioritizes_agents_files_over_fallbacks() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp.path().join("AGENTS.md"), "agents-content").expect("write AGENTS");
        std::fs::write(temp.path().join("CLAUDE.md"), "claude-content").expect("write CLAUDE");

        let config = ContextDiscoveryConfig {
            paths: vec!["AGENTS.md".into(), "CLAUDE.md".into()],
            scan_directories: false,
            case_insensitive_dedup: true,
            max_total_size: 100_000,
            max_file_size: 50_000,
        };
        let loader = ContextLoader::new(config, temp.path().to_path_buf());
        let discovered = loader.load().await;

        assert_eq!(discovered.loaded_files, vec![temp.path().join("AGENTS.md")]);
        assert!(discovered.content.contains("agents-content"));
        assert!(!discovered.content.contains("claude-content"));
    }

    #[tokio::test]
    async fn test_context_loader_falls_back_when_agents_missing() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp.path().join("CLAUDE.md"), "claude-only").expect("write CLAUDE");

        let config = ContextDiscoveryConfig {
            paths: vec!["AGENTS.md".into(), "CLAUDE.md".into()],
            scan_directories: false,
            case_insensitive_dedup: true,
            max_total_size: 100_000,
            max_file_size: 50_000,
        };
        let loader = ContextLoader::new(config, temp.path().to_path_buf());
        let discovered = loader.load().await;

        assert_eq!(discovered.loaded_files, vec![temp.path().join("CLAUDE.md")]);
        assert!(discovered.content.contains("claude-only"));
    }
}
