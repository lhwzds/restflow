use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const AGENTS_DIR: &str = "agents";
const TASK_POLICY_FILE: &str = "task.md";
/// Environment variable to override the agents directory path (used in tests).
pub const AGENTS_DIR_ENV: &str = "RESTFLOW_AGENTS_DIR";

const DEFAULT_AGENT_PROMPT_ASSET: &str = include_str!("../assets/agents/default.md");
const TASK_POLICY_ASSET: &str = include_str!("../assets/agents/task.md");

pub fn ensure_prompt_templates() -> Result<()> {
    ensure_prompt_template_file(TASK_POLICY_FILE, TASK_POLICY_ASSET)?;
    Ok(())
}

pub fn load_default_main_agent_prompt() -> Result<String> {
    Ok(DEFAULT_AGENT_PROMPT_ASSET.to_string())
}

pub fn load_task_policy(task_id: Option<&str>) -> Result<String> {
    let path = ensure_prompt_template_file(TASK_POLICY_FILE, TASK_POLICY_ASSET)?;
    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read task policy prompt: {}", path.display()))?;
    Ok(apply_task_id_placeholder(&content, task_id))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedAgentPrompt {
    pub content: Option<String>,
    pub prompt_file: Option<String>,
}

pub fn load_agent_prompt_for_agent(
    agent_id: &str,
    agent_name: &str,
    prompt_file: Option<&str>,
) -> Result<LoadedAgentPrompt> {
    validate_agent_id(agent_id)?;
    let Some(path) = resolve_prompt_path_for_read(agent_name, prompt_file)? else {
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

    Ok(LoadedAgentPrompt {
        content: if content.trim().is_empty() {
            None
        } else {
            Some(content)
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

pub fn ensure_agent_prompt_file(
    agent_id: &str,
    agent_name: &str,
    current_prompt_file: Option<&str>,
    prompt_override: Option<&str>,
) -> Result<PathBuf> {
    ensure_prompt_templates()?;
    validate_agent_id(agent_id)?;
    let path = resolve_prompt_path_for_write(agent_name, current_prompt_file)?;

    if let Some(prompt) = prompt_override {
        fs::write(&path, prompt)
            .with_context(|| format!("Failed to write agent prompt: {}", path.display()))?;
        return Ok(path);
    }

    if path.exists() {
        return Ok(path);
    }

    let default_prompt = load_default_main_agent_prompt()?;
    fs::write(&path, default_prompt)
        .with_context(|| format!("Failed to initialize agent prompt: {}", path.display()))?;
    Ok(path)
}

pub fn delete_agent_prompt_file_for_agent(
    agent_id: &str,
    _agent_name: &str,
    prompt_file: Option<&str>,
) -> Result<()> {
    validate_agent_id(agent_id)?;
    if let Some(prompt_file) = prompt_file
        && let Some(path) = resolve_prompt_path_from_file_name(prompt_file)?
        && path.exists()
    {
        fs::remove_file(&path)
            .with_context(|| format!("Failed to remove agent prompt file: {}", path.display()))?;
    }
    Ok(())
}

fn apply_task_id_placeholder(content: &str, task_id: Option<&str>) -> String {
    let task_id = task_id.unwrap_or("unknown");
    let replacements = HashMap::from([("{{task_id}}", task_id), ("{{task_id}}", task_id)]);
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

    Ok(None)
}

fn resolve_prompt_path_for_write(agent_name: &str, prompt_file: Option<&str>) -> Result<PathBuf> {
    let agents_dir = ensure_agents_dir()?;
    let desired = agents_dir.join(format!("{}.md", sanitize_agent_file_stem(agent_name)));
    let current_from_prompt_file = if let Some(prompt_file) = prompt_file {
        resolve_prompt_path_from_file_name(prompt_file)?.filter(|path| path.exists())
    } else {
        None
    };
    let current = current_from_prompt_file;

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
        // Reuse an existing name-based prompt file when the agent has no stored prompt file.
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

/// Shared lock for tests that mutate the RESTFLOW_AGENTS_DIR env var.
/// All tests that set/remove this env var MUST acquire this lock first
/// to avoid cross-module race conditions.
#[cfg(any(test, feature = "test-utils"))]
pub fn agents_dir_env_lock() -> std::sync::MutexGuard<'static, ()> {
    agents_dir_env_lock_impl()
}

#[cfg(test)]
fn agents_dir_env_lock_impl() -> std::sync::MutexGuard<'static, ()> {
    restflow_test_support::agents_env_lock()
}

#[cfg(all(not(test), feature = "test-utils"))]
fn agents_dir_env_lock_impl() -> std::sync::MutexGuard<'static, ()> {
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
    fn test_load_task_policy_replaces_task_id() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var(AGENTS_DIR_ENV, temp.path()) };

        let content = load_task_policy(Some("task-123")).unwrap();
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
        assert!(!temp.path().join("default.md").exists());
        assert!(temp.path().join(TASK_POLICY_FILE).exists());

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
    fn test_ensure_agent_prompt_file_preserves_plain_body() {
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
        let content = "{{task_id}} - {{task_id}}";
        let malicious_task_id = "injected{{task_id}}"; // If double-substitution happens, this would become "injectedinjected{{task_id}}"
        let result = apply_task_id_placeholder(content, Some(malicious_task_id));

        // Should NOT perform second substitution - the {{task_id}} in the value should remain as-is
        assert_eq!(result, "injected{{task_id}} - injected{{task_id}}");
    }

    #[test]
    fn test_apply_task_id_placeholder_handles_none() {
        let content = "{{task_id}} - {{task_id}}";
        let result = apply_task_id_placeholder(content, None);
        assert_eq!(result, "unknown - unknown");
    }
}
