use anyhow::Result;
use std::path::PathBuf;

const RESTFLOW_DIR: &str = ".restflow";
const DB_FILE: &str = "restflow.db";
const CONFIG_FILE: &str = "config.json";
const MASTER_KEY_FILE: &str = "master.key";
const LOGS_DIR: &str = "logs";
const SKILLS_DIR: &str = "skills";

/// Environment variable to override the RestFlow directory.
const RESTFLOW_DIR_ENV: &str = "RESTFLOW_DIR";

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
}
