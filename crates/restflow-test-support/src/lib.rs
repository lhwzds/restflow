use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard, OnceLock};

use tempfile::TempDir;

const RESTFLOW_DIR_ENV: &str = "RESTFLOW_DIR";
const RESTFLOW_GLOBAL_CONFIG_ENV: &str = "RESTFLOW_GLOBAL_CONFIG";
const RESTFLOW_WORKSPACE_CONFIG_ENV: &str = "RESTFLOW_WORKSPACE_CONFIG";
const RESTFLOW_MASTER_KEY_ENV: &str = "RESTFLOW_MASTER_KEY";
const RESTFLOW_AGENTS_DIR_ENV: &str = "RESTFLOW_AGENTS_DIR";

const ENV_KEYS: &[&str] = &[
    RESTFLOW_DIR_ENV,
    RESTFLOW_GLOBAL_CONFIG_ENV,
    RESTFLOW_WORKSPACE_CONFIG_ENV,
    RESTFLOW_MASTER_KEY_ENV,
    RESTFLOW_AGENTS_DIR_ENV,
];

#[derive(Debug)]
struct SavedEnv {
    key: &'static str,
    value: Option<OsString>,
}

/// Serialize tests that mutate RestFlow process-global environment.
pub fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Serialize tests that mutate the legacy agents directory override.
pub fn agents_env_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Isolated RestFlow state root for tests.
///
/// Production defaults still resolve to `~/.restflow`; this helper only scopes
/// tests that opt into it.
#[derive(Debug)]
pub struct RestflowTestEnv {
    _lock: MutexGuard<'static, ()>,
    _agents_lock: MutexGuard<'static, ()>,
    root: TempDir,
    global_config: PathBuf,
    workspace_config: PathBuf,
    agents_dir: PathBuf,
    agents_dir_override: bool,
    saved: Vec<SavedEnv>,
}

impl RestflowTestEnv {
    pub fn new() -> Self {
        Self::create(false)
    }

    pub fn with_agents_dir_override() -> Self {
        Self::create(true)
    }

    fn create(agents_dir_override: bool) -> Self {
        let lock = env_lock();
        let agents_lock = agents_env_lock();
        let root = tempfile::tempdir().expect("restflow test root should be created");
        let global_config = root.path().join("config.toml");
        let workspace_config = root.path().join("workspace-config.toml");
        let agents_dir = root.path().join("agents");
        std::fs::create_dir_all(&agents_dir).expect("agents dir should be created");

        let saved = ENV_KEYS
            .iter()
            .map(|key| SavedEnv {
                key,
                value: std::env::var_os(key),
            })
            .collect::<Vec<_>>();

        unsafe {
            std::env::set_var(RESTFLOW_DIR_ENV, root.path());
            std::env::set_var(RESTFLOW_GLOBAL_CONFIG_ENV, &global_config);
            std::env::set_var(RESTFLOW_WORKSPACE_CONFIG_ENV, &workspace_config);
            std::env::remove_var(RESTFLOW_MASTER_KEY_ENV);
            if agents_dir_override {
                std::env::set_var(RESTFLOW_AGENTS_DIR_ENV, &agents_dir);
            } else {
                std::env::remove_var(RESTFLOW_AGENTS_DIR_ENV);
            }
        }

        Self {
            _lock: lock,
            _agents_lock: agents_lock,
            root,
            global_config,
            workspace_config,
            agents_dir,
            agents_dir_override,
            saved,
        }
    }

    pub fn root(&self) -> &Path {
        self.root.path()
    }

    pub fn db_path(&self, file_name: &str) -> PathBuf {
        self.root.path().join(file_name)
    }

    pub fn agents_dir(&self) -> &Path {
        &self.agents_dir
    }

    pub fn apply_to_command<'a>(&self, command: &'a mut Command) -> &'a mut Command {
        command
            .env(RESTFLOW_DIR_ENV, self.root.path())
            .env(RESTFLOW_GLOBAL_CONFIG_ENV, &self.global_config)
            .env(RESTFLOW_WORKSPACE_CONFIG_ENV, &self.workspace_config)
            .env_remove(RESTFLOW_MASTER_KEY_ENV);

        if self.agents_dir_override {
            command.env(RESTFLOW_AGENTS_DIR_ENV, &self.agents_dir);
        } else {
            command.env_remove(RESTFLOW_AGENTS_DIR_ENV);
        }

        command
    }
}

impl Default for RestflowTestEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for RestflowTestEnv {
    fn drop(&mut self) {
        unsafe {
            for saved in self.saved.iter().rev() {
                match saved.value.as_ref() {
                    Some(value) => std::env::set_var(saved.key, value),
                    None => std::env::remove_var(saved.key),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolates_restflow_state_env() {
        let env = RestflowTestEnv::new();

        assert_eq!(
            std::env::var_os(RESTFLOW_DIR_ENV).as_deref(),
            Some(env.root().as_os_str())
        );
        assert!(std::env::var_os(RESTFLOW_MASTER_KEY_ENV).is_none());
        assert!(std::env::var_os(RESTFLOW_AGENTS_DIR_ENV).is_none());
    }

    #[test]
    fn can_opt_into_agents_dir_override() {
        let env = RestflowTestEnv::with_agents_dir_override();

        assert_eq!(
            std::env::var_os(RESTFLOW_AGENTS_DIR_ENV).as_deref(),
            Some(env.agents_dir().as_os_str())
        );
    }
}
