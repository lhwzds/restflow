//! Path utilities for RestFlow directory resolution.
//!
//! This is the canonical source for shared path functions. Re-exported by
//! restflow-core for convenience.

use anyhow::Result;
use std::path::PathBuf;

const RESTFLOW_DIR: &str = ".restflow";
const MASTER_KEY_FILE: &str = "master.key";

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

/// Get the master key path: ~/.restflow/master.key
pub fn master_key_path() -> Result<PathBuf> {
    Ok(resolve_restflow_dir()?.join(MASTER_KEY_FILE))
}
