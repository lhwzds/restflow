use std::ffi::OsString;
use std::path::{Path, PathBuf};
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
    saved: Vec<SavedEnv>,
}

impl RestflowTestEnv {
    pub fn new() -> Self {
        let lock = env_lock();
        let agents_lock = agents_env_lock();
        let root = tempfile::tempdir().expect("restflow test root should be created");
        let global_config = root.path().join("config.toml");
        let workspace_config = root.path().join("workspace-config.toml");

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
            std::env::remove_var(RESTFLOW_AGENTS_DIR_ENV);
        }

        Self {
            _lock: lock,
            _agents_lock: agents_lock,
            root,
            saved,
        }
    }

    pub fn root(&self) -> &Path {
        self.root.path()
    }

    pub fn db_path(&self, file_name: &str) -> PathBuf {
        self.root.path().join(file_name)
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
