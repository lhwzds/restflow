use crate::AppCore;
use crate::services::session::SessionService;
use anyhow::Result;
use std::path::Path;
use std::sync::Arc;
use tracing::debug;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const DAY_SECS: u64 = 24 * 60 * 60;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct CleanupReport {
    pub chat_sessions: usize,
    pub tasks: usize,
    pub audit_events: usize,
    pub daemon_log_files: usize,
}

pub async fn run_cleanup(core: &Arc<AppCore>) -> Result<CleanupReport> {
    let config = core.storage.config.get_effective_config()?;
    let now_ms = chrono::Utc::now().timestamp_millis();
    let sessions = SessionService::from_storage(&core.storage);

    let mut chat_sessions = 0usize;
    if let Some(cutoff) = retention_cutoff(now_ms, config.chat_session_retention_days) {
        chat_sessions += sessions
            .cleanup_workspace_sessions_older_than(cutoff)?
            .deleted;
    }
    chat_sessions += sessions
        .cleanup_workspace_sessions_by_retention(now_ms)?
        .deleted;

    let tasks = 0;

    let audit_events = 0;

    // L1: Clean up old log files (blocking I/O, offload to spawn_blocking)
    let retention_days = config.log_file_retention_days;
    let daemon_log_files =
        tokio::task::spawn_blocking(move || cleanup_daemon_log_files(retention_days).unwrap_or(0))
            .await
            .unwrap_or(0);
    Ok(CleanupReport {
        chat_sessions,
        tasks,
        audit_events,
        daemon_log_files,
    })
}

/// L1: Delete daemon log files older than retention_days.
///
/// Scans `~/.restflow/logs/` for files matching `daemon.log*` or `restflow.log*`.
fn cleanup_daemon_log_files(retention_days: u32) -> Result<usize> {
    if retention_days == 0 {
        return Ok(0);
    }

    let logs_dir = match crate::paths::logs_dir() {
        Ok(dir) => dir,
        Err(_) => return Ok(0),
    };

    cleanup_old_files_in_dir(&logs_dir, retention_days, |name| {
        name.starts_with("daemon.log") || name.starts_with("restflow.log")
    })
}

/// Delete files older than `retention_days` in `dir` that match the `filter` predicate.
///
/// Returns the number of deleted files. Ignores subdirectories.
pub(crate) fn cleanup_old_files_in_dir(
    dir: &Path,
    retention_days: u32,
    filter: impl Fn(&str) -> bool,
) -> Result<usize> {
    if retention_days == 0 {
        return Ok(0);
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e.into()),
    };

    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(
            retention_days as u64 * DAY_SECS,
        ))
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

    let mut deleted = 0;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        if !filter(&file_name) {
            continue;
        }

        let modified = match entry.metadata().and_then(|m| m.modified()) {
            Ok(t) => t,
            Err(_) => continue,
        };

        if modified < cutoff && std::fs::remove_file(&path).is_ok() {
            deleted += 1;
            debug!(file = %path.display(), "Deleted old log file");
        }
    }

    Ok(deleted)
}

fn retention_cutoff(now_ms: i64, retention_days: u32) -> Option<i64> {
    if retention_days == 0 {
        return None;
    }
    Some(now_ms - (retention_days as i64) * DAY_MS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn retention_cutoff_handles_forever() {
        assert_eq!(retention_cutoff(10_000, 0), None);
    }

    #[test]
    fn retention_cutoff_calculates_ms() {
        assert_eq!(
            retention_cutoff(10_000, 1),
            Some(10_000 - 24 * 60 * 60 * 1000)
        );
    }

    #[test]
    fn test_cleanup_report_default_includes_new_fields() {
        let report = CleanupReport::default();
        assert_eq!(report.daemon_log_files, 0);
    }

    #[test]
    fn test_cleanup_old_files_deletes_old() {
        let temp_dir = TempDir::new().unwrap();
        let dir = temp_dir.path();

        // Create an "old" file and a "new" file
        let old_file = dir.join("daemon.log.2024-01-01");
        let new_file = dir.join("daemon.log.2026-02-01");
        fs::write(&old_file, "old data").unwrap();
        fs::write(&new_file, "new data").unwrap();

        // Set the old file's modified time to 60 days ago
        let old_time = std::time::SystemTime::now()
            .checked_sub(std::time::Duration::from_secs(60 * DAY_SECS))
            .unwrap();
        filetime::set_file_mtime(&old_file, filetime::FileTime::from_system_time(old_time))
            .unwrap();

        let deleted =
            cleanup_old_files_in_dir(dir, 30, |name| name.starts_with("daemon.log")).unwrap();

        assert_eq!(deleted, 1);
        assert!(!old_file.exists(), "old file should be deleted");
        assert!(new_file.exists(), "new file should remain");
    }

    #[test]
    fn test_cleanup_old_files_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        let deleted = cleanup_old_files_in_dir(temp_dir.path(), 30, |_| true).unwrap();
        assert_eq!(deleted, 0);
    }

    #[test]
    fn test_cleanup_old_files_nonexistent_dir() {
        let temp_dir = TempDir::new().unwrap();
        let missing = temp_dir.path().join("nonexistent");
        let deleted = cleanup_old_files_in_dir(&missing, 30, |_| true).unwrap();
        assert_eq!(deleted, 0);
    }

    #[test]
    fn test_cleanup_old_files_zero_retention_skips() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("test.log"), "data").unwrap();
        let deleted = cleanup_old_files_in_dir(temp_dir.path(), 0, |_| true).unwrap();
        assert_eq!(deleted, 0);
    }
}
