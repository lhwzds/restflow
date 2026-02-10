use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

const AGENTS_DIR: &str = "agents";
const DEFAULT_AGENT_PROMPT_FILE: &str = "default_agent.md";
const BACKGROUND_AGENT_POLICY_FILE: &str = "background_agent_policy.md";
/// Environment variable to override the agents directory path (used in tests).
pub const AGENTS_DIR_ENV: &str = "RESTFLOW_AGENTS_DIR";

const DEFAULT_AGENT_PROMPT_ASSET: &str = include_str!("../assets/default_agent.md");
const BACKGROUND_AGENT_POLICY_ASSET: &str = include_str!("../assets/background_agent_policy.md");

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
    ensure_agent_prompt_file(agent_id, None)?;
    let path = agent_prompt_path(agent_id)?;
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read agent prompt: {}", path.display()))?;
    if content.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(content))
    }
}

pub fn ensure_agent_prompt_file(agent_id: &str, prompt_override: Option<&str>) -> Result<PathBuf> {
    ensure_prompt_templates()?;
    let path = agent_prompt_path(agent_id)?;
    if let Some(prompt) = prompt_override {
        fs::write(&path, prompt)
            .with_context(|| format!("Failed to write agent prompt: {}", path.display()))?;
        return Ok(path);
    }

    if !path.exists() {
        let default_prompt = load_default_main_agent_prompt()?;
        fs::write(&path, default_prompt)
            .with_context(|| format!("Failed to initialize agent prompt: {}", path.display()))?;
    }

    Ok(path)
}

pub fn delete_agent_prompt_file(agent_id: &str) -> Result<()> {
    let path = agent_prompt_path(agent_id)?;
    if path.exists() {
        fs::remove_file(&path)
            .with_context(|| format!("Failed to remove agent prompt file: {}", path.display()))?;
    }
    Ok(())
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

fn agent_prompt_path(agent_id: &str) -> Result<PathBuf> {
    let id = agent_id.trim();
    if id.is_empty() {
        anyhow::bail!("Agent ID is empty; cannot resolve prompt file path");
    }
    // Reject path traversal characters to prevent directory escape
    if id.contains('/')
        || id.contains('\\')
        || id.contains("..")
        || id.contains('\0')
    {
        anyhow::bail!(
            "Agent ID '{}' contains invalid characters (path separators or '..' sequences)",
            id
        );
    }

    let agents_dir = ensure_agents_dir()?;
    let path = agents_dir.join(format!("{id}.md"));

    if !path.starts_with(&agents_dir) {
        anyhow::bail!("Resolved prompt path escapes agents directory");
    }

    Ok(path)
}

/// Shared lock for tests that mutate the RESTFLOW_AGENTS_DIR env var.
/// All tests that set/remove this env var MUST acquire this lock first
/// to avoid cross-module race conditions.
#[cfg(test)]
pub fn agents_dir_env_lock() -> std::sync::MutexGuard<'static, ()> {
    use std::sync::{Mutex, OnceLock};
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
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

        let path = ensure_agent_prompt_file("agent-1", None).unwrap();
        assert!(path.exists());
        let content = fs::read_to_string(path).unwrap();
        assert!(!content.trim().is_empty());

        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_load_agent_prompt_returns_override_content() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var(AGENTS_DIR_ENV, temp.path()) };

        ensure_agent_prompt_file("agent-2", Some("Custom prompt")).unwrap();
        let loaded = load_agent_prompt("agent-2").unwrap();
        assert_eq!(loaded.as_deref(), Some("Custom prompt"));

        unsafe { std::env::remove_var(AGENTS_DIR_ENV) };
    }

    #[test]
    fn test_agent_prompt_path_rejects_path_traversal() {
        assert!(agent_prompt_path("../etc/passwd").is_err());
        assert!(agent_prompt_path("foo/bar").is_err());
        assert!(agent_prompt_path("foo\\bar").is_err());
        assert!(agent_prompt_path("foo..bar").is_err());
        assert!(agent_prompt_path("foo\0bar").is_err());
    }

    #[test]
    fn test_agent_prompt_path_accepts_valid_ids() {
        assert!(agent_prompt_path("my-agent").is_ok());
        assert!(agent_prompt_path("agent_1").is_ok());
        assert!(agent_prompt_path("default").is_ok());
        assert!(agent_prompt_path("550e8400-e29b-41d4-a716-446655440000").is_ok());
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
