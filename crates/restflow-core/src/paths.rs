use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

const RESTFLOW_DIR: &str = ".restflow";
const DB_FILE: &str = "restflow.db";
const CONFIG_FILE: &str = "config.json";
const MASTER_KEY_FILE: &str = "master.key";
const LOGS_DIR: &str = "logs";
const SKILLS_DIR: &str = "skills";

/// Environment variable to override the RestFlow directory.
const RESTFLOW_DIR_ENV: &str = "RESTFLOW_DIR";

#[derive(Debug, Clone)]
pub struct SkillMigrationReport {
    pub user_dir: PathBuf,
    pub legacy_dirs: Vec<PathBuf>,
    pub copied_skill_dirs: usize,
}

/// Resolve the RestFlow configuration directory.
/// Priority: RESTFLOW_DIR env var > ~/.restflow/
pub fn resolve_restflow_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var(RESTFLOW_DIR_ENV)
        && !dir.trim().is_empty()
    {
        return Ok(PathBuf::from(dir));
    }
    dirs::home_dir()
        .map(|h| h.join(RESTFLOW_DIR))
        .ok_or_else(|| anyhow::anyhow!("Failed to determine home directory"))
}

/// Ensure the RestFlow directory exists and return its path.
pub fn ensure_restflow_dir() -> Result<PathBuf> {
    let dir = resolve_restflow_dir()?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Get the database path: ~/.restflow/restflow.db
pub fn database_path() -> Result<PathBuf> {
    Ok(resolve_restflow_dir()?.join(DB_FILE))
}

/// Ensure database path exists and return as string.
pub fn ensure_database_path() -> Result<PathBuf> {
    Ok(ensure_restflow_dir()?.join(DB_FILE))
}

/// Convenience helper returning the database path as a UTF-8 string.
pub fn ensure_database_path_string() -> Result<String> {
    Ok(ensure_database_path()?.to_string_lossy().into_owned())
}

/// Get the config file path: ~/.restflow/config.json
pub fn config_path() -> Result<PathBuf> {
    Ok(resolve_restflow_dir()?.join(CONFIG_FILE))
}

/// Get the master key path: ~/.restflow/master.key
pub fn master_key_path() -> Result<PathBuf> {
    Ok(resolve_restflow_dir()?.join(MASTER_KEY_FILE))
}

/// Get the logs directory: ~/.restflow/logs/
pub fn logs_dir() -> Result<PathBuf> {
    let dir = resolve_restflow_dir()?.join(LOGS_DIR);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// User-global skills directory: ~/.restflow/skills/
pub fn user_skills_dir() -> Result<PathBuf> {
    let dir = ensure_restflow_dir()?.join(SKILLS_DIR);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Legacy workspace-local skills directory alias.
///
/// Workspace-level skills are no longer supported. This function now maps to
/// the user-global path `~/.restflow/skills/` for backward compatibility.
pub fn workspace_skills_dir() -> Result<PathBuf> {
    user_skills_dir()
}

/// Detect legacy workspace skill directories and copy missing skill folders
/// into the user-global skills directory.
pub fn migrate_legacy_workspace_skills_to_user() -> Result<SkillMigrationReport> {
    let user_dir = user_skills_dir()?;
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    migrate_legacy_workspace_skills_from(&cwd, &user_dir)
}

fn migrate_legacy_workspace_skills_from(
    start: &Path,
    user_dir: &Path,
) -> Result<SkillMigrationReport> {
    let legacy_dirs = discover_legacy_workspace_skill_dirs(start, user_dir);
    let copied_skill_dirs = copy_legacy_skill_dirs(&legacy_dirs, user_dir)?;
    Ok(SkillMigrationReport {
        user_dir: user_dir.to_path_buf(),
        legacy_dirs,
        copied_skill_dirs,
    })
}

fn discover_legacy_workspace_skill_dirs(start: &Path, user_dir: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let current_candidate = start.join(RESTFLOW_DIR).join(SKILLS_DIR);
    if current_candidate != user_dir {
        candidates.push(current_candidate);
    }

    if let Some(repo_root) = find_repo_root(start) {
        let repo_candidate = repo_root.join(RESTFLOW_DIR).join(SKILLS_DIR);
        if repo_candidate != user_dir && !candidates.contains(&repo_candidate) {
            candidates.push(repo_candidate);
        }
    }

    candidates
}

fn find_repo_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

fn copy_legacy_skill_dirs(legacy_dirs: &[PathBuf], user_dir: &Path) -> Result<usize> {
    let mut copied = 0usize;
    for legacy_dir in legacy_dirs {
        if !legacy_dir.exists() || !legacy_dir.is_dir() {
            continue;
        }

        for entry in fs::read_dir(legacy_dir).with_context(|| {
            format!(
                "Failed to read legacy workspace skills directory: {}",
                legacy_dir.display()
            )
        })? {
            let entry = entry?;
            let source = entry.path();
            if !source.is_dir() {
                continue;
            }
            if !source.join("SKILL.md").exists() {
                continue;
            }

            let target = user_dir.join(entry.file_name());
            if target.exists() {
                continue;
            }

            copy_dir_recursive(&source, &target).with_context(|| {
                format!(
                    "Failed to migrate skill directory '{}' to '{}'",
                    source.display(),
                    target.display()
                )
            })?;
            copied += 1;
        }
    }
    Ok(copied)
}

fn copy_dir_recursive(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else {
            fs::copy(&source_path, &target_path)?;
        }
    }
    Ok(())
}

/// IPC socket path: ~/.restflow/restflow.sock
pub fn socket_path() -> Result<PathBuf> {
    Ok(ensure_restflow_dir()?.join("restflow.sock"))
}

/// Daemon PID file path: ~/.restflow/daemon.pid
pub fn daemon_pid_path() -> Result<PathBuf> {
    Ok(ensure_restflow_dir()?.join("daemon.pid"))
}

/// Daemon log file path: ~/.restflow/logs/daemon.log
pub fn daemon_log_path() -> Result<PathBuf> {
    Ok(logs_dir()?.join("daemon.log"))
}

/// Ensure the RestFlow data directory exists and return its path.
#[deprecated(note = "Use ensure_restflow_dir instead")]
pub fn ensure_data_dir() -> Result<PathBuf> {
    ensure_restflow_dir()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    #[test]
    fn test_default_restflow_dir() {
        let _lock = env_lock();
        unsafe { std::env::remove_var(RESTFLOW_DIR_ENV) };
        let dir = resolve_restflow_dir().unwrap();
        assert!(dir.ends_with(RESTFLOW_DIR));
    }

    #[test]
    fn test_env_override() {
        let _lock = env_lock();
        unsafe { std::env::set_var(RESTFLOW_DIR_ENV, "/tmp/test-restflow") };
        let dir = resolve_restflow_dir().unwrap();
        assert_eq!(dir, PathBuf::from("/tmp/test-restflow"));
        unsafe { std::env::remove_var(RESTFLOW_DIR_ENV) };
    }

    #[test]
    fn test_database_path() {
        let _lock = env_lock();
        unsafe { std::env::remove_var(RESTFLOW_DIR_ENV) };
        let path = database_path().unwrap();
        assert!(path.ends_with(DB_FILE));
        assert!(path.parent().unwrap().ends_with(RESTFLOW_DIR));
    }

    #[test]
    fn test_migrate_legacy_workspace_skills_to_user_copies_missing_skill_dirs() {
        let _lock = env_lock();
        let old_cwd = std::env::current_dir().unwrap();

        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(workspace.join(".git")).unwrap();
        let workspace_skill_dir = workspace
            .join(".restflow")
            .join("skills")
            .join("legacy-skill");
        fs::create_dir_all(&workspace_skill_dir).unwrap();
        fs::write(
            workspace_skill_dir.join("SKILL.md"),
            "---\nname: Legacy\n---\n\n# Legacy",
        )
        .unwrap();

        let user_state = temp.path().join("user-state");
        unsafe { std::env::set_var(RESTFLOW_DIR_ENV, &user_state) };
        std::env::set_current_dir(&workspace).unwrap();

        let report = migrate_legacy_workspace_skills_to_user().unwrap();

        let migrated_skill = user_state
            .join("skills")
            .join("legacy-skill")
            .join("SKILL.md");
        assert!(migrated_skill.exists());
        assert_eq!(report.copied_skill_dirs, 1);
        assert!(
            report
                .legacy_dirs
                .iter()
                .any(|dir| dir.ends_with(".restflow/skills"))
        );

        std::env::set_current_dir(old_cwd).unwrap();
        unsafe { std::env::remove_var(RESTFLOW_DIR_ENV) };
    }

    #[test]
    fn test_migrate_legacy_workspace_skills_to_user_does_not_overwrite_existing() {
        let _lock = env_lock();
        let old_cwd = std::env::current_dir().unwrap();

        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(workspace.join(".git")).unwrap();
        let legacy_skill_dir = workspace.join(".restflow").join("skills").join("same-id");
        fs::create_dir_all(&legacy_skill_dir).unwrap();
        fs::write(
            legacy_skill_dir.join("SKILL.md"),
            "---\nname: Legacy\n---\n\n# Legacy",
        )
        .unwrap();

        let user_state = temp.path().join("user-state");
        let user_skill_dir = user_state.join("skills").join("same-id");
        fs::create_dir_all(&user_skill_dir).unwrap();
        fs::write(
            user_skill_dir.join("SKILL.md"),
            "---\nname: User\n---\n\n# User",
        )
        .unwrap();

        unsafe { std::env::set_var(RESTFLOW_DIR_ENV, &user_state) };
        std::env::set_current_dir(&workspace).unwrap();

        let report = migrate_legacy_workspace_skills_to_user().unwrap();
        let content = fs::read_to_string(user_skill_dir.join("SKILL.md")).unwrap();
        assert!(content.contains("name: User"));
        assert_eq!(report.copied_skill_dirs, 0);

        std::env::set_current_dir(old_cwd).unwrap();
        unsafe { std::env::remove_var(RESTFLOW_DIR_ENV) };
    }
}
