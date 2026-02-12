use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

const AGENTS_DIR: &str = "agents";
const DEFAULT_AGENT_PROMPT_FILE: &str = "default_agent.md";
const BACKGROUND_AGENT_POLICY_FILE: &str = "background_agent_policy.md";
const AGENT_ID_METADATA_PREFIX: &str = "<!-- restflow-agent-id: ";
const METADATA_SUFFIX: &str = " -->";
/// Environment variable to override the agents directory path (used in tests).
pub const AGENTS_DIR_ENV: &str = "RESTFLOW_AGENTS_DIR";

const DEFAULT_AGENT_PROMPT_ASSET: &str = include_str!("../assets/agents/default_agent.md");
const BACKGROUND_AGENT_POLICY_ASSET: &str =
    include_str!("../assets/agents/background_agent_policy.md");

pub fn ensure_prompt_templates() -> Result<()> {
    ensure_prompt_template_file(DEFAULT_AGENT_PROMPT_FILE, DEFAULT_AGENT_PROMPT_ASSET)?;
    ensure_prompt_template_file(BACKGROUND_AGENT_POLICY_FILE, BACKGROUND_AGENT_POLICY_ASSET)?;
    Ok(())
}

pub fn load_default_main_agent_prompt() -> Result<String> {
    let path = ensure_prompt_template_file(DEFAULT_AGENT_PROMPT_FILE, DEFAULT_AGENT_PROMPT_ASSET)?;
    fs::read_to_string(&path).with_context(|| {
        format!(
            "Failed to read default main agent prompt: {}",
            path.display()
        )
    })
}

pub fn load_background_agent_policy(background_task_id: Option<&str>) -> Result<String> {
    let path =
        ensure_prompt_template_file(BACKGROUND_AGENT_POLICY_FILE, BACKGROUND_AGENT_POLICY_ASSET)?;
    let content = fs::read_to_string(&path).with_context(|| {
        format!(
            "Failed to read background agent policy prompt: {}",
            path.display()
        )
    })?;
    Ok(apply_task_id_placeholder(&content, background_task_id))
}

pub fn load_agent_prompt(agent_id: &str) -> Result<Option<String>> {
    let id = validate_agent_id(agent_id)?;
    let Some(path) = find_agent_prompt_path_by_id(id)? else {
        return Ok(None);
    };

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read agent prompt: {}", path.display()))?;
    let parsed = parse_prompt_file_content(&content);
    if parsed.body.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(parsed.body))
    }
}

pub fn ensure_agent_prompt_file(
    agent_id: &str,
    agent_name: &str,
    prompt_override: Option<&str>,
) -> Result<PathBuf> {
    ensure_prompt_templates()?;
    let id = validate_agent_id(agent_id)?;
    let path = resolve_prompt_path_for_write(id, agent_name)?;

    let prompt_body = if let Some(prompt) = prompt_override {
        prompt.to_string()
    } else if path.exists() {
        let existing = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read existing agent prompt: {}", path.display()))?;
        parse_prompt_file_content(&existing).body
    } else {
        load_default_main_agent_prompt()?
    };

    let serialized = serialize_prompt_file(id, &prompt_body);
    fs::write(&path, serialized)
        .with_context(|| format!("Failed to write agent prompt: {}", path.display()))?;
    Ok(path)
}

pub fn delete_agent_prompt_file(agent_id: &str) -> Result<()> {
    let id = validate_agent_id(agent_id)?;
    if let Some(path) = find_agent_prompt_path_by_id(id)? {
        fs::remove_file(&path)
            .with_context(|| format!("Failed to remove agent prompt file: {}", path.display()))?;
    }
    Ok(())
}

pub fn cleanup_orphan_agent_prompt_files(active_agent_ids: &[String]) -> Result<usize> {
    let active_ids: std::collections::HashSet<&str> =
        active_agent_ids.iter().map(String::as_str).collect();
    let agents_dir = ensure_agents_dir()?;
    let mut deleted = 0usize;

    for entry in fs::read_dir(&agents_dir)
        .with_context(|| format!("Failed to read agents directory: {}", agents_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }

        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };

        if stem == DEFAULT_AGENT_PROMPT_FILE.trim_end_matches(".md")
            || stem == BACKGROUND_AGENT_POLICY_FILE.trim_end_matches(".md")
        {
            continue;
        }
        let file_content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read prompt file: {}", path.display()))?;
        let parsed = parse_prompt_file_content(&file_content);

        let should_delete = if let Some(owner_id) = parsed.agent_id.as_deref() {
            !active_ids.contains(owner_id)
        } else if Uuid::parse_str(stem).is_ok() {
            // Legacy file named by full agent ID with no metadata.
            !active_ids.contains(stem)
        } else {
            // Preserve non-agent Markdown files in the folder.
            false
        };

        if should_delete {
            fs::remove_file(&path).with_context(|| {
                format!("Failed to remove orphan prompt file: {}", path.display())
            })?;
            deleted += 1;
        }
    }

    Ok(deleted)
}

fn apply_task_id_placeholder(content: &str, background_task_id: Option<&str>) -> String {
    let task_id = background_task_id.unwrap_or("unknown");
    content
        .replace("{{task_id}}", task_id)
        .replace("{{background_task_id}}", task_id)
}

fn resolve_agents_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var(AGENTS_DIR_ENV)
        && !dir.trim().is_empty()
    {
        return Ok(PathBuf::from(dir));
    }

    Ok(crate::paths::ensure_restflow_dir()?.join(AGENTS_DIR))
}

fn ensure_agents_dir() -> Result<PathBuf> {
    let dir = resolve_agents_dir()?;
    fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create agents directory: {}", dir.display()))?;
    Ok(dir)
}

fn ensure_prompt_template_file(file_name: &str, default_content: &str) -> Result<PathBuf> {
    let path = ensure_agents_dir()?.join(file_name);
    if !path.exists() {
        fs::write(&path, default_content).with_context(|| {
            format!(
                "Failed to write default prompt template '{}' to {}",
                file_name,
                path.display()
            )
        })?;
    }
    Ok(path)
}

fn validate_agent_id(agent_id: &str) -> Result<&str> {
    let id = agent_id.trim();
    if id.is_empty() {
        anyhow::bail!("Agent ID is empty; cannot resolve prompt file path");
    }
    // Reject path traversal characters to prevent directory escape
    if id.contains('/') || id.contains('\\') || id.contains("..") || id.contains('\0') {
        anyhow::bail!(
            "Agent ID '{}' contains invalid characters (path separators or '..' sequences)",
            id
        );
    }
    Ok(id)
}

fn find_agent_prompt_path_by_id(agent_id: &str) -> Result<Option<PathBuf>> {
    let agents_dir = ensure_agents_dir()?;
    let legacy_path = agents_dir.join(format!("{agent_id}.md"));
    if legacy_path.exists() {
        return Ok(Some(legacy_path));
    }

    for entry in fs::read_dir(&agents_dir)
        .with_context(|| format!("Failed to read agents directory: {}", agents_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        if stem == DEFAULT_AGENT_PROMPT_FILE.trim_end_matches(".md")
            || stem == BACKGROUND_AGENT_POLICY_FILE.trim_end_matches(".md")
        {
            continue;
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read prompt file: {}", path.display()))?;
        let parsed = parse_prompt_file_content(&content);
        if parsed.agent_id.as_deref() == Some(agent_id) {
            return Ok(Some(path));
        }
    }

    Ok(None)
}

fn resolve_prompt_path_for_write(agent_id: &str, agent_name: &str) -> Result<PathBuf> {
    let agents_dir = ensure_agents_dir()?;
    let desired = agents_dir.join(format!("{}.md", sanitize_agent_file_stem(agent_name)));
    let existing = find_agent_prompt_path_by_id(agent_id)?;

    if let Some(existing_path) = existing {
        if existing_path == desired {
            return Ok(existing_path);
        }
        if !desired.exists() {
            fs::rename(&existing_path, &desired).with_context(|| {
                format!(
                    "Failed to rename agent prompt file from {} to {}",
                    existing_path.display(),
                    desired.display()
                )
            })?;
            return Ok(desired);
        }
        if prompt_path_belongs_to_agent(&desired, agent_id)? {
            if existing_path != desired && existing_path.exists() {
                let _ = fs::remove_file(&existing_path);
            }
            return Ok(desired);
        }
        let fallback = unique_prompt_path(&agents_dir, agent_name, agent_id)?;
        if existing_path != fallback {
            fs::rename(&existing_path, &fallback).with_context(|| {
                format!(
                    "Failed to rename agent prompt file from {} to {}",
                    existing_path.display(),
                    fallback.display()
                )
            })?;
        }
        return Ok(fallback);
    }

    if !desired.exists() || prompt_path_belongs_to_agent(&desired, agent_id)? {
        return Ok(desired);
    }

    unique_prompt_path(&agents_dir, agent_name, agent_id)
}

fn unique_prompt_path(
    agents_dir: &std::path::Path,
    agent_name: &str,
    agent_id: &str,
) -> Result<PathBuf> {
    let stem = sanitize_agent_file_stem(agent_name);
    let short_id: String = agent_id.chars().take(8).collect();
    for index in 0..1000u16 {
        let suffix = if index == 0 {
            format!("-{short_id}")
        } else {
            format!("-{short_id}-{index}")
        };
        let candidate = agents_dir.join(format!("{stem}{suffix}.md"));
        if !candidate.exists() || prompt_path_belongs_to_agent(&candidate, agent_id)? {
            return Ok(candidate);
        }
    }
    anyhow::bail!(
        "Failed to allocate unique prompt file path for agent '{}'",
        agent_id
    );
}

fn prompt_path_belongs_to_agent(path: &std::path::Path, agent_id: &str) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read prompt file: {}", path.display()))?;
    let parsed = parse_prompt_file_content(&content);
    if parsed.agent_id.as_deref() == Some(agent_id) {
        return Ok(true);
    }
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    Ok(stem == agent_id)
}

fn sanitize_agent_file_stem(name: &str) -> String {
    let mut stem = String::with_capacity(name.len());
    let mut last_dash = false;
    for ch in name.trim().chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            Some(ch.to_ascii_lowercase())
        } else if ch == '-' || ch == '_' {
            Some(ch)
        } else {
            Some('-')
        };

        if let Some(value) = mapped {
            if value == '-' {
                if last_dash {
                    continue;
                }
                last_dash = true;
            } else {
                last_dash = false;
            }
            stem.push(value);
        }
    }

    let normalized = stem.trim_matches(['-', '_', '.']).to_string();
    if normalized.is_empty() {
        "agent".to_string()
    } else {
        normalized
    }
}

fn serialize_prompt_file(agent_id: &str, prompt_body: &str) -> String {
    format!("{AGENT_ID_METADATA_PREFIX}{agent_id}{METADATA_SUFFIX}\n\n{prompt_body}")
}

struct ParsedPromptFileContent {
    agent_id: Option<String>,
    body: String,
}

fn parse_prompt_file_content(content: &str) -> ParsedPromptFileContent {
    let mut lines = content.lines();
    let first = lines.next();
    if let Some(first_line) = first
        && let Some(raw_id) = first_line
            .trim()
            .strip_prefix(AGENT_ID_METADATA_PREFIX)
            .and_then(|value| value.strip_suffix(METADATA_SUFFIX))
    {
        let mut remaining: Vec<&str> = lines.collect();
        while matches!(remaining.first(), Some(line) if line.trim().is_empty()) {
            remaining.remove(0);
        }
        return ParsedPromptFileContent {
            agent_id: Some(raw_id.trim().to_string()),
            body: remaining.join("\n"),
        };
    }

    ParsedPromptFileContent {
        agent_id: None,
        body: content.to_string(),
    }
}

/// Shared lock for tests that mutate the RESTFLOW_AGENTS_DIR env var.
/// All tests that set/remove this env var MUST acquire this lock first
/// to avoid cross-module race conditions.
#[cfg(test)]
pub fn agents_dir_env_lock() -> std::sync::MutexGuard<'static, ()> {
    use std::sync::{Mutex, OnceLock};
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        agents_dir_env_lock()
    }

    #[test]
    fn test_load_background_agent_policy_replaces_task_id() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var(AGENTS_DIR_ENV, temp.path()) };

        let content = load_background_agent_policy(Some("task-123")).unwrap();
        assert!(content.contains("task-123"));
        assert!(!content.contains("{{task_id}}"));

        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_ensure_prompt_templates_creates_files() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var(AGENTS_DIR_ENV, temp.path()) };

        ensure_prompt_templates().unwrap();
        assert!(temp.path().join(DEFAULT_AGENT_PROMPT_FILE).exists());
        assert!(temp.path().join(BACKGROUND_AGENT_POLICY_FILE).exists());

        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_ensure_agent_prompt_file_creates_per_agent_markdown() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var(AGENTS_DIR_ENV, temp.path()) };

        let path =
            ensure_agent_prompt_file("550e8400-e29b-41d4-a716-446655440000", "Agent One", None)
                .unwrap();
        assert!(path.exists());
        assert_eq!(
            path.file_name().and_then(|v| v.to_str()),
            Some("agent-one.md")
        );
        let content = fs::read_to_string(path).unwrap();
        assert!(!content.trim().is_empty());

        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_load_agent_prompt_returns_override_content() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var(AGENTS_DIR_ENV, temp.path()) };

        let id = "f7e39ba8-f1ed-4e6c-a4f4-1983f671b1d5";
        ensure_agent_prompt_file(id, "My Custom Agent", Some("Custom prompt")).unwrap();
        let loaded = load_agent_prompt(id).unwrap();
        assert_eq!(loaded.as_deref(), Some("Custom prompt"));

        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_load_agent_prompt_missing_does_not_create_file() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var(AGENTS_DIR_ENV, temp.path()) };

        ensure_prompt_templates().unwrap();
        let missing = "750bf7ee";
        let loaded = load_agent_prompt(missing).unwrap();
        assert!(loaded.is_none());
        assert!(!temp.path().join(format!("{missing}.md")).exists());

        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_cleanup_orphan_agent_prompt_files_removes_only_orphans() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var(AGENTS_DIR_ENV, temp.path()) };

        ensure_prompt_templates().unwrap();

        let active = "750bf7ee-91fa-47b2-9498-25007fd99919".to_string();
        let orphan = "016e8f2a-944d-4126-af6f-f19b0110d8d6".to_string();
        fs::write(temp.path().join(format!("{active}.md")), "active").unwrap();
        fs::write(temp.path().join(format!("{orphan}.md")), "orphan").unwrap();
        fs::write(temp.path().join("custom-note.md"), "keep").unwrap();

        let deleted = cleanup_orphan_agent_prompt_files(std::slice::from_ref(&active)).unwrap();
        assert_eq!(deleted, 1);
        assert!(temp.path().join(format!("{active}.md")).exists());
        assert!(!temp.path().join(format!("{orphan}.md")).exists());
        assert!(temp.path().join("custom-note.md").exists());

        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_agent_prompt_path_rejects_path_traversal() {
        assert!(validate_agent_id("../etc/passwd").is_err());
        assert!(validate_agent_id("foo/bar").is_err());
        assert!(validate_agent_id("foo\\bar").is_err());
        assert!(validate_agent_id("foo..bar").is_err());
        assert!(validate_agent_id("foo\0bar").is_err());
    }

    #[test]
    fn test_agent_prompt_path_accepts_valid_ids() {
        assert!(validate_agent_id("my-agent").is_ok());
        assert!(validate_agent_id("agent_1").is_ok());
        assert!(validate_agent_id("default").is_ok());
        assert!(validate_agent_id("550e8400-e29b-41d4-a716-446655440000").is_ok());
    }

    #[test]
    fn test_ensure_agent_prompt_file_migrates_legacy_id_filename() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var(AGENTS_DIR_ENV, temp.path()) };

        let id = "4a14a8dd-5a5e-4f12-ae3e-fe40476d8f9b";
        let legacy = temp.path().join(format!("{id}.md"));
        fs::write(&legacy, "legacy content").unwrap();
        let migrated = ensure_agent_prompt_file(id, "Renamed Agent", None).unwrap();

        assert!(!legacy.exists());
        assert_eq!(
            migrated.file_name().and_then(|v| v.to_str()),
            Some("renamed-agent.md")
        );
        let loaded = load_agent_prompt(id).unwrap();
        assert_eq!(loaded.as_deref(), Some("legacy content"));

        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_resolve_agents_dir_defaults_to_restflow_home_agents() {
        let _lock = env_lock();
        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
        let expected = crate::paths::resolve_restflow_dir().unwrap().join("agents");
        let actual = resolve_agents_dir().unwrap();
        assert_eq!(actual, expected);
    }
}
