use anyhow::Result;
use std::path::PathBuf;

// Re-export shared path utilities from restflow-storage (single source of truth)
pub use restflow_storage::paths::{ensure_restflow_dir, master_key_path, resolve_restflow_dir};

const DB_FILE: &str = "restflow.db";
const CONFIG_FILE: &str = "config.json";
const LOGS_DIR: &str = "logs";
const SKILLS_DIR: &str = "skills";

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

/// IPC socket path: ~/.restflow/restflow.sock
pub fn socket_path() -> Result<PathBuf> {
    Ok(ensure_restflow_dir()?.join("restflow.sock"))
}

/// Daemon PID file path: ~/.restflow/daemon.pid
pub fn daemon_pid_path() -> Result<PathBuf> {
    Ok(ensure_restflow_dir()?.join("daemon.pid"))
}

/// Daemon lock file path: ~/.restflow/daemon.lock
pub fn daemon_lock_path() -> Result<PathBuf> {
    Ok(ensure_restflow_dir()?.join("daemon.lock"))
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
        unsafe { std::env::remove_var("RESTFLOW_DIR") };
        let dir = resolve_restflow_dir().unwrap();
        assert!(dir.ends_with(".restflow"));
    }

    #[test]
    fn test_env_override() {
        let _lock = env_lock();
        unsafe { std::env::set_var("RESTFLOW_DIR", "/tmp/test-restflow") };
        let dir = resolve_restflow_dir().unwrap();
        assert_eq!(dir, PathBuf::from("/tmp/test-restflow"));
        unsafe { std::env::remove_var("RESTFLOW_DIR") };
    }

    #[test]
    fn test_database_path() {
        let _lock = env_lock();
        unsafe { std::env::remove_var("RESTFLOW_DIR") };
        let path = database_path().unwrap();
        assert!(path.ends_with(DB_FILE));
        assert!(path.parent().unwrap().ends_with(".restflow"));
    }

    #[test]
    fn test_daemon_lock_path() {
        let _lock = env_lock();
        unsafe { std::env::remove_var("RESTFLOW_DIR") };
        let path = daemon_lock_path().unwrap();
        assert!(path.ends_with("daemon.lock"));
        assert!(path.parent().unwrap().ends_with(".restflow"));
    }
}
