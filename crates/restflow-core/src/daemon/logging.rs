use crate::paths;
use anyhow::Result;
use std::fs::File;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct LogPaths {
    pub daemon_log: PathBuf,
}

pub fn resolve_log_paths() -> Result<LogPaths> {
    Ok(LogPaths {
        daemon_log: paths::daemon_log_path()?,
    })
}

pub fn open_daemon_log_append() -> Result<File> {
    let paths = resolve_log_paths()?;
    if let Some(parent) = paths.daemon_log.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths.daemon_log)?;
    Ok(file)
}
