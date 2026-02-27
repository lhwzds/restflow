use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::warn;
use uuid::Uuid;

const AGENTS_DIR: &str = "agents";
const DEFAULT_AGENT_PROMPT_FILE: &str = "default.md";
const BACKGROUND_AGENT_POLICY_FILE: &str = "background_agent.md";
const AGENT_ID_METADATA_PREFIX: &str = "<!-- restflow-agent-id: ";
const METADATA_SUFFIX: &str = " -->";
/// Environment variable to override the agents directory path (used in tests).
pub const AGENTS_DIR_ENV: &str = "RESTFLOW_AGENTS_DIR";

const DEFAULT_AGENT_PROMPT_ASSET: &str = include_str!("../assets/agents/default.md");
const BACKGROUND_AGENT_POLICY_ASSET: &str =
    include_str!("../assets/agents/background_agent.md");

pub fn ensure_prompt_templates() -> Result<()> {
    ensure_prompt_template_file(BACKGROUND_AGENT_POLICY_FILE, BACKGROUND_AGENT_POLICY_ASSET)?;
    Ok(())
}

pub fn load_default_main_agent_prompt() -> Result<String> {
    Ok(DEFAULT_AGENT_PROMPT_ASSET.to_string())
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedAgentPrompt {
    pub content: Option<String>,
    pub prompt_file: Option<String>,
}

/// Legacy lookup by agent ID only.
///
/// This only resolves legacy prompt files (`<agent_id>.md` or metadata-tagged files).
/// New prompt loading should use [`load_agent_prompt_for_agent`].
pub fn load_agent_prompt(agent_id: &str) -> Result<Option<String>> {
    let id = validate_agent_id(agent_id)?;
    let Some(path) = find_agent_prompt_path_by_id(id)? else {
        return Ok(None);
    };

    let Some(content) = read_prompt_file_if_exists(&path)? else {
        return Ok(None);
    };
    let parsed = parse_prompt_file_content(&content);
    if parsed.body.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(parsed.body))
    }
}

pub fn load_agent_prompt_for_agent(
    agent_id: &str,
    agent_name: &str,
    prompt_file: Option<&str>,
) -> Result<LoadedAgentPrompt> {
    let id = validate_agent_id(agent_id)?;
    let Some(path) = resolve_prompt_path_for_read(id, agent_name, prompt_file)? else {
        return Ok(LoadedAgentPrompt {
            content: None,
            prompt_file: None,
        });
    };

    let Some(content) = read_prompt_file_if_exists(&path)? else {
        return Ok(LoadedAgentPrompt {
            content: None,
            prompt_file: None,
        });
    };

    let parsed = parse_prompt_file_content(&content);
    if parsed.agent_id.is_some() {
        // One-time migration: strip legacy metadata marker from file content.
        fs::write(&path, serialize_prompt_file(id, &parsed.body))
            .with_context(|| format!("Failed to migrate legacy prompt file: {}", path.display()))?;
    }

    Ok(LoadedAgentPrompt {
        content: if parsed.body.trim().is_empty() {
            None
        } else {
            Some(parsed.body)
        },
        prompt_file: Some(extract_prompt_file_name(&path)?),
    })
}

fn read_prompt_file_if_exists(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => {
            Err(error).with_context(|| format!("Failed to read agent prompt: {}", path.display()))
        }
    }
}

pub fn load_all_agent_prompts() -> Result<std::collections::HashMap<String, String>> {
    let agents_dir = ensure_agents_dir()?;
    let mut selected: std::collections::HashMap<String, PromptSelection> =
        std::collections::HashMap::new();

    for entry in fs::read_dir(&agents_dir)
        .with_context(|| format!("Failed to read agents directory: {}", agents_dir.display()))?
    {
        let Ok(entry) = entry else {
            warn!("Skipping unreadable entry in agents directory");
            continue;
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        let Some(stem) = path
            .file_stem()
            .and_then(|value| value.to_str())
            .map(|value| value.to_string())
        else {
            continue;
        };
        if stem == DEFAULT_AGENT_PROMPT_FILE.trim_end_matches(".md")
            || stem == BACKGROUND_AGENT_POLICY_FILE.trim_end_matches(".md")
        {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(err) => {
                warn!(
                    path = %path.display(),
                    error = %err,
                    "Skipping unreadable agent prompt file"
                );
                continue;
            }
        };
        let parsed = parse_prompt_file_content(&content);
        let Some(owner_id) = owner_id_for_prompt(&parsed, &stem) else {
            continue;
        };
        let rank = selection_rank(&owner_id, &parsed, &stem);
        let body = parsed.body;
        let path_key = path.to_string_lossy().to_string();

        match selected.get_mut(&owner_id) {
            Some(existing) => {
                if (rank, path_key.as_str()) < (existing.rank, existing.path_key.as_str()) {
                    warn!(
                        agent_id = %owner_id,
                        old = %existing.path_key,
                        new = %path_key,
                        "Multiple prompt files found for agent; selecting deterministic candidate"
                    );
                    *existing = PromptSelection {
                        rank,
                        path_key,
                        body,
                    };
                }
            }
            None => {
                selected.insert(
                    owner_id,
                    PromptSelection {
                        rank,
                        path_key,
                        body,
                    },
                );
            }
        }
    }

    let mut prompts = std::collections::HashMap::new();
    for (agent_id, selection) in selected {
        prompts.insert(agent_id, selection.body);
    }
    Ok(prompts)
}

pub fn ensure_agent_prompt_file(
    agent_id: &str,
    agent_name: &str,
    current_prompt_file: Option<&str>,
    prompt_override: Option<&str>,
) -> Result<PathBuf> {
    ensure_prompt_templates()?;
    let id = validate_agent_id(agent_id)?;
    let path = resolve_prompt_path_for_write(id, agent_name, current_prompt_file)?;

    if let Some(prompt) = prompt_override {
        let serialized = serialize_prompt_file(id, prompt);
        fs::write(&path, serialized)
            .with_context(|| format!("Failed to write agent prompt: {}", path.display()))?;
        return Ok(path);
    }

    if path.exists() {
        let existing = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read existing agent prompt: {}", path.display()))?;
        let parsed = parse_prompt_file_content(&existing);
        if parsed.agent_id.is_some() {
            // One-time migration for legacy metadata format.
            let serialized = serialize_prompt_file(id, &parsed.body);
            fs::write(&path, serialized).with_context(|| {
                format!(
                    "Failed to migrate agent prompt metadata: {}",
                    path.display()
                )
            })?;
        }
        return Ok(path);
    }

    let default_prompt = load_default_main_agent_prompt()?;
    let serialized = serialize_prompt_file(id, &default_prompt);
    fs::write(&path, serialized)
        .with_context(|| format!("Failed to initialize agent prompt: {}", path.display()))?;
    Ok(path)
}

pub fn delete_agent_prompt_file_for_agent(
    agent_id: &str,
    _agent_name: &str,
    prompt_file: Option<&str>,
) -> Result<()> {
    let id = validate_agent_id(agent_id)?;
    if let Some(prompt_file) = prompt_file
        && let Some(path) = resolve_prompt_path_from_file_name(prompt_file)?
        && path.exists()
    {
        fs::remove_file(&path)
            .with_context(|| format!("Failed to remove agent prompt file: {}", path.display()))?;
        return Ok(());
    }

    if let Some(path) = find_agent_prompt_path_by_id(id)? {
        fs::remove_file(&path)
            .with_context(|| format!("Failed to remove agent prompt file: {}", path.display()))?;
    }
    Ok(())
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
        let file_content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(err) => {
                warn!(
                    path = %path.display(),
                    error = %err,
                    "Skipping unreadable prompt file during orphan cleanup"
                );
                continue;
            }
        };
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
            match fs::remove_file(&path) {
                Ok(_) => {
                    deleted += 1;
                }
                Err(err) => {
                    warn!(
                        path = %path.display(),
                        error = %err,
                        "Failed to remove orphan prompt file; skipping"
                    );
                }
            }
        }
    }

    Ok(deleted)
}

fn apply_task_id_placeholder(content: &str, background_task_id: Option<&str>) -> String {
    let task_id = background_task_id.unwrap_or("unknown");
    let replacements = HashMap::from([
        ("{{task_id}}", task_id),
        ("{{background_task_id}}", task_id),
    ]);
    crate::template::render_template_single_pass(content, &replacements)
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
    let mut candidates: Vec<(u8, String, PathBuf)> = Vec::new();
    let legacy_path = agents_dir.join(format!("{agent_id}.md"));
    if legacy_path.exists() {
        candidates.push((
            2,
            legacy_path.to_string_lossy().to_string(),
            legacy_path.clone(),
        ));
    }

    for entry in fs::read_dir(&agents_dir)
        .with_context(|| format!("Failed to read agents directory: {}", agents_dir.display()))?
    {
        let Ok(entry) = entry else {
            warn!("Skipping unreadable entry in agents directory");
            continue;
        };
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
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(err) => {
                warn!(
                    path = %path.display(),
                    error = %err,
                    "Skipping unreadable prompt file while resolving agent prompt path"
                );
                continue;
            }
        };
        let parsed = parse_prompt_file_content(&content);
        if parsed.agent_id.as_deref() == Some(agent_id) {
            let rank = selection_rank(agent_id, &parsed, stem);
            candidates.push((rank, path.to_string_lossy().to_string(), path));
        }
    }

    if candidates.is_empty() {
        return Ok(None);
    }
    candidates.sort_by(|a, b| (a.0, a.1.as_str()).cmp(&(b.0, b.1.as_str())));
    if candidates.len() > 1 {
        warn!(
            agent_id = %agent_id,
            count = candidates.len(),
            "Multiple prompt files match this agent; selecting deterministic candidate"
        );
    }
    Ok(candidates.into_iter().next().map(|(_, _, path)| path))
}

fn resolve_prompt_path_from_file_name(prompt_file: &str) -> Result<Option<PathBuf>> {
    let trimmed = prompt_file.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains("..")
        || trimmed.contains('\0')
    {
        anyhow::bail!("Prompt file name contains invalid characters: {}", trimmed);
    }
    Ok(Some(ensure_agents_dir()?.join(trimmed)))
}

fn extract_prompt_file_name(path: &Path) -> Result<String> {
    path.file_name()
        .and_then(|value| value.to_str())
        .map(ToString::to_string)
        .ok_or_else(|| anyhow::anyhow!("Invalid prompt file path: {}", path.display()))
}

fn resolve_prompt_path_for_read(
    agent_id: &str,
    agent_name: &str,
    prompt_file: Option<&str>,
) -> Result<Option<PathBuf>> {
    if let Some(prompt_file) = prompt_file
        && let Some(path) = resolve_prompt_path_from_file_name(prompt_file)?
        && path.exists()
    {
        return Ok(Some(path));
    }

    let agents_dir = ensure_agents_dir()?;
    let desired = agents_dir.join(format!("{}.md", sanitize_agent_file_stem(agent_name)));
    if desired.exists() {
        return Ok(Some(desired));
    }

    find_agent_prompt_path_by_id(agent_id)
}

fn resolve_prompt_path_for_write(
    agent_id: &str,
    agent_name: &str,
    prompt_file: Option<&str>,
) -> Result<PathBuf> {
    let agents_dir = ensure_agents_dir()?;
    let desired = agents_dir.join(format!("{}.md", sanitize_agent_file_stem(agent_name)));
    let current_from_prompt_file = if let Some(prompt_file) = prompt_file {
        resolve_prompt_path_from_file_name(prompt_file)?.filter(|path| path.exists())
    } else {
        None
    };
    let current = if current_from_prompt_file.is_some() {
        current_from_prompt_file
    } else {
        find_agent_prompt_path_by_id(agent_id)?
    };

    if let Some(current_path) = current {
        if current_path == desired {
            return Ok(current_path);
        }
        if !desired.exists() {
            fs::rename(&current_path, &desired).with_context(|| {
                format!(
                    "Failed to rename agent prompt file from {} to {}",
                    current_path.display(),
                    desired.display()
                )
            })?;
            return Ok(desired);
        }
        let fallback = unique_prompt_path(&agents_dir, agent_name)?;
        if current_path != fallback {
            fs::rename(&current_path, &fallback).with_context(|| {
                format!(
                    "Failed to rename agent prompt file from {} to {}",
                    current_path.display(),
                    fallback.display()
                )
            })?;
        }
        return Ok(fallback);
    }

    if !desired.exists() {
        return Ok(desired);
    }

    if prompt_file.is_none() {
        // Adopt existing name-based file for migration from metadata-less format.
        return Ok(desired);
    }

    unique_prompt_path(&agents_dir, agent_name)
}

fn unique_prompt_path(agents_dir: &std::path::Path, agent_name: &str) -> Result<PathBuf> {
    let stem = sanitize_agent_file_stem(agent_name);
    for index in 2..1000u16 {
        let candidate = agents_dir.join(format!("{stem}-{index}.md"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    anyhow::bail!(
        "Failed to allocate unique prompt file path for stem '{}'",
        stem
    );
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
    let candidate = if normalized.is_empty() {
        "agent".to_string()
    } else {
        normalized
    };
    if is_windows_reserved_stem(&candidate) {
        format!("{candidate}-agent")
    } else {
        candidate
    }
}

fn serialize_prompt_file(agent_id: &str, prompt_body: &str) -> String {
    let _ = agent_id;
    prompt_body.to_string()
}

fn is_windows_reserved_stem(stem: &str) -> bool {
    let lower = stem.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "con"
            | "prn"
            | "aux"
            | "nul"
            | "com1"
            | "com2"
            | "com3"
            | "com4"
            | "com5"
            | "com6"
            | "com7"
            | "com8"
            | "com9"
            | "lpt1"
            | "lpt2"
            | "lpt3"
            | "lpt4"
            | "lpt5"
            | "lpt6"
            | "lpt7"
            | "lpt8"
            | "lpt9"
    )
}

struct ParsedPromptFileContent {
    agent_id: Option<String>,
    body: String,
}

struct PromptSelection {
    rank: u8,
    path_key: String,
    body: String,
}

fn owner_id_for_prompt(parsed: &ParsedPromptFileContent, stem: &str) -> Option<String> {
    if let Some(agent_id) = parsed.agent_id.as_deref() {
        return Some(agent_id.to_string());
    }
    if Uuid::parse_str(stem).is_ok() {
        return Some(stem.to_string());
    }
    None
}

fn selection_rank(agent_id: &str, parsed: &ParsedPromptFileContent, stem: &str) -> u8 {
    let is_uuid_stem = Uuid::parse_str(stem).is_ok();
    if parsed.agent_id.as_deref() == Some(agent_id) {
        if is_uuid_stem { 1 } else { 0 }
    } else {
        2
    }
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
        if matches!(remaining.first(), Some(line) if line.trim().is_empty()) {
            // Remove only the first separator line after metadata and preserve user formatting.
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
        assert!(!temp.path().join(DEFAULT_AGENT_PROMPT_FILE).exists());
        assert!(temp.path().join(BACKGROUND_AGENT_POLICY_FILE).exists());

        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_ensure_agent_prompt_file_creates_per_agent_markdown() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var(AGENTS_DIR_ENV, temp.path()) };

        let path = ensure_agent_prompt_file(
            "550e8400-e29b-41d4-a716-446655440000",
            "Agent One",
            None,
            None,
        )
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
    fn test_ensure_agent_prompt_file_uses_numeric_suffix_on_name_conflict() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var(AGENTS_DIR_ENV, temp.path()) };

        fs::write(temp.path().join("agent-one.md"), "existing").unwrap();
        let legacy_id = "550e8400-e29b-41d4-a716-446655440000";
        fs::write(temp.path().join(format!("{legacy_id}.md")), "legacy").unwrap();
        let path = ensure_agent_prompt_file(legacy_id, "Agent One", None, None).unwrap();

        assert_eq!(
            path.file_name().and_then(|value| value.to_str()),
            Some("agent-one-2.md")
        );
        let content = fs::read_to_string(path).unwrap();
        assert_eq!(content, "legacy");

        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_load_agent_prompt_returns_override_content() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var(AGENTS_DIR_ENV, temp.path()) };

        let id = "f7e39ba8-f1ed-4e6c-a4f4-1983f671b1d5";
        ensure_agent_prompt_file(id, "My Custom Agent", None, Some("Custom prompt")).unwrap();
        let loaded = load_agent_prompt_for_agent(id, "My Custom Agent", None).unwrap();
        assert_eq!(loaded.content.as_deref(), Some("Custom prompt"));
        assert_eq!(loaded.prompt_file.as_deref(), Some("my-custom-agent.md"));

        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_load_agent_prompt_prefers_named_metadata_file_over_legacy_id_file() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var(AGENTS_DIR_ENV, temp.path()) };

        let id = "f7e39ba8-f1ed-4e6c-a4f4-1983f671b1d5";
        fs::write(temp.path().join(format!("{id}.md")), "legacy content").unwrap();
        fs::write(
            temp.path().join("my-agent.md"),
            format!("{AGENT_ID_METADATA_PREFIX}{id}{METADATA_SUFFIX}\n\nnew content"),
        )
        .unwrap();

        let loaded = load_agent_prompt_for_agent(id, "My Agent", None).unwrap();
        assert_eq!(loaded.content.as_deref(), Some("new content"));
        assert_eq!(loaded.prompt_file.as_deref(), Some("my-agent.md"));

        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_load_all_agent_prompts_uses_deterministic_best_candidate() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var(AGENTS_DIR_ENV, temp.path()) };

        let id = "f7e39ba8-f1ed-4e6c-a4f4-1983f671b1d5";
        fs::write(temp.path().join(format!("{id}.md")), "legacy content").unwrap();
        fs::write(
            temp.path().join("agent-a.md"),
            format!("{AGENT_ID_METADATA_PREFIX}{id}{METADATA_SUFFIX}\n\ncontent a"),
        )
        .unwrap();
        fs::write(
            temp.path().join("agent-b.md"),
            format!("{AGENT_ID_METADATA_PREFIX}{id}{METADATA_SUFFIX}\n\ncontent b"),
        )
        .unwrap();

        let prompts = load_all_agent_prompts().unwrap();
        // agent-a.md is lexicographically smaller than agent-b.md, so it wins deterministically.
        assert_eq!(prompts.get(id).map(String::as_str), Some("content a"));

        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_ensure_agent_prompt_file_migrates_metadata_to_plain_body() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var(AGENTS_DIR_ENV, temp.path()) };

        let id = "d95c9423-42d7-4a13-ad80-ff94e16f8f8a";
        let path =
            ensure_agent_prompt_file(id, "No Rewrite", None, Some("\nLine A\nLine B")).unwrap();
        let _ = ensure_agent_prompt_file(id, "No Rewrite", None, None).unwrap();
        let after = fs::read_to_string(&path).unwrap();
        assert_eq!(after, "\nLine A\nLine B");

        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_load_agent_prompt_missing_does_not_create_file() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var(AGENTS_DIR_ENV, temp.path()) };

        ensure_prompt_templates().unwrap();
        let missing = "750bf7ee";
        let loaded = load_agent_prompt_for_agent(missing, "Missing Agent", None).unwrap();
        assert!(loaded.content.is_none());
        assert!(!temp.path().join(format!("{missing}.md")).exists());

        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_read_prompt_file_if_exists_returns_none_for_deleted_file() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("deleted.md");
        fs::write(&path, "temp").unwrap();
        fs::remove_file(&path).unwrap();

        let loaded = read_prompt_file_if_exists(&path).unwrap();
        assert!(loaded.is_none());
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
        let migrated = ensure_agent_prompt_file(id, "Renamed Agent", None, None).unwrap();

        assert!(!legacy.exists());
        assert_eq!(
            migrated.file_name().and_then(|v| v.to_str()),
            Some("renamed-agent.md")
        );
        let migrated_raw = fs::read_to_string(&migrated).unwrap();
        assert_eq!(migrated_raw, "legacy content");
        let loaded = load_agent_prompt_for_agent(id, "Renamed Agent", None).unwrap();
        assert_eq!(loaded.content.as_deref(), Some("legacy content"));
        assert_eq!(loaded.prompt_file.as_deref(), Some("renamed-agent.md"));

        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_sanitize_agent_file_stem_avoids_windows_reserved_names() {
        assert_eq!(sanitize_agent_file_stem("CON"), "con-agent");
        assert_eq!(sanitize_agent_file_stem("aux"), "aux-agent");
        assert_eq!(sanitize_agent_file_stem("Lpt1"), "lpt1-agent");
        assert_eq!(sanitize_agent_file_stem("Normal Name"), "normal-name");
    }

    #[test]
    fn test_resolve_agents_dir_defaults_to_restflow_home_agents() {
        let _lock = env_lock();
        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
        let expected = crate::paths::resolve_restflow_dir().unwrap().join("agents");
        let actual = resolve_agents_dir().unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_apply_task_id_placeholder_prevents_double_substitution() {
        // Test that a malicious task_id containing placeholder syntax doesn't get re-processed
        let content = "{{task_id}} - {{background_task_id}}";
        let malicious_task_id = "injected{{task_id}}"; // If double-substitution happens, this would become "injectedinjected{{task_id}}"
        let result = apply_task_id_placeholder(content, Some(malicious_task_id));

        // Should NOT perform second substitution - the {{task_id}} in the value should remain as-is
        assert_eq!(result, "injected{{task_id}} - injected{{task_id}}");
    }

    #[test]
    fn test_apply_task_id_placeholder_handles_none() {
        let content = "{{task_id}} - {{background_task_id}}";
        let result = apply_task_id_placeholder(content, None);
        assert_eq!(result, "unknown - unknown");
    }
}
