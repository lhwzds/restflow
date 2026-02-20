use anyhow::Result;
use std::path::Path;
use std::sync::Arc;
use tracing::debug;

use crate::AppCore;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const DAY_SECS: u64 = 24 * 60 * 60;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct CleanupReport {
    pub chat_sessions: usize,
    pub background_tasks: usize,
    pub checkpoints: usize,
    pub memory_chunks: usize,
    pub memory_sessions: usize,
    pub vector_orphans: usize,
    pub daemon_log_files: usize,
    pub event_log_files: usize,
}

pub async fn run_cleanup(core: &Arc<AppCore>) -> Result<CleanupReport> {
    let config = core.storage.config.get_config()?.unwrap_or_default();
    let now_ms = chrono::Utc::now().timestamp_millis();

    let mut chat_sessions =
        if retention_cutoff(now_ms, config.chat_session_retention_days).is_some() {
            core.storage
                .chat_sessions
                .cleanup_expired(config.chat_session_retention_days, now_ms)?
        } else {
            0
        };
    chat_sessions += core
        .storage
        .chat_sessions
        .cleanup_by_session_retention(now_ms)?
        .deleted;

    let background_tasks =
        if let Some(cutoff) = retention_cutoff(now_ms, config.background_task_retention_days) {
            core.storage.background_agents.cleanup_old_tasks(cutoff)?
        } else {
            0
        };

    let checkpoints = core
        .storage
        .background_agents
        .cleanup_expired_checkpoints()?;

    let memory_chunks =
        if let Some(cutoff) = retention_cutoff(now_ms, config.memory_chunk_retention_days) {
            core.storage.memory.cleanup_old_chunks(cutoff)?
        } else {
            0
        };

    // M2: Clean up empty memory sessions
    let memory_sessions = cleanup_empty_memory_sessions(core)?;

    // M3: Clean up vector orphans if threshold exceeded
    let vector_orphans = cleanup_vectors_if_needed(core)?;

    // L1: Clean up old log files
    let retention_days = config.log_file_retention_days;
    let daemon_log_files = cleanup_daemon_log_files(retention_days).unwrap_or(0);
    let event_log_files = cleanup_event_log_files(retention_days).unwrap_or(0);

    Ok(CleanupReport {
        chat_sessions,
        background_tasks,
        checkpoints,
        memory_chunks,
        memory_sessions,
        vector_orphans,
        daemon_log_files,
        event_log_files,
    })
}

/// M2: Delete memory sessions that have zero chunks.
fn cleanup_empty_memory_sessions(core: &Arc<AppCore>) -> Result<usize> {
    let agents = core.storage.agents.list_agents()?;
    let agent_ids: Vec<String> = agents.iter().map(|a| a.id.clone()).collect();
    cleanup_empty_sessions_for_agents(&core.storage.memory, &agent_ids)
}

/// Inner helper: delete empty sessions for the given agent IDs.
///
/// Separated from `cleanup_empty_memory_sessions` for testability
/// (avoids needing a full `AppCore` in unit tests).
fn cleanup_empty_sessions_for_agents(
    memory: &crate::storage::MemoryStorage,
    agent_ids: &[String],
) -> Result<usize> {
    let mut deleted = 0;

    for agent_id in agent_ids {
        let sessions = memory.list_sessions(agent_id)?;
        for session in sessions {
            let chunks = memory.list_chunks_for_session(&session.id)?;
            if chunks.is_empty() {
                memory.delete_session(&session.id, false)?;
                deleted += 1;
                debug!(session_id = %session.id, "Deleted empty memory session");
            }
        }
    }

    Ok(deleted)
}

/// M3: Clean up vector orphans when the count exceeds a threshold.
fn cleanup_vectors_if_needed(core: &Arc<AppCore>) -> Result<usize> {
    if let Some(stats) = core.storage.memory.vector_stats()? {
        let threshold = std::cmp::max(stats.active_count / 10, 100);
        if stats.orphan_count > threshold {
            debug!(
                orphans = stats.orphan_count,
                threshold,
                "Vector orphan threshold exceeded, running cleanup"
            );
            if let Some(cleaned) = core.storage.memory.cleanup_vector_orphans()? {
                return Ok(cleaned);
            }
        }
    }
    Ok(0)
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

/// L1: Delete event log files older than retention_days.
///
/// Scans `~/.restflow/logs/` for `.jsonl` files (task event logs).
fn cleanup_event_log_files(retention_days: u32) -> Result<usize> {
    if retention_days == 0 {
        return Ok(0);
    }

    let logs_dir = match crate::paths::logs_dir() {
        Ok(dir) => dir,
        Err(_) => return Ok(0),
    };

    cleanup_old_files_in_dir(&logs_dir, retention_days, |name| {
        name.ends_with(".jsonl")
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
        assert_eq!(report.memory_sessions, 0);
        assert_eq!(report.vector_orphans, 0);
        assert_eq!(report.daemon_log_files, 0);
        assert_eq!(report.event_log_files, 0);
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
        filetime::set_file_mtime(
            &old_file,
            filetime::FileTime::from_system_time(old_time),
        )
        .unwrap();

        let deleted = cleanup_old_files_in_dir(dir, 30, |name| {
            name.starts_with("daemon.log")
        })
        .unwrap();

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

    // --- M2 integration tests ---

    fn create_test_memory_storage() -> (crate::storage::MemoryStorage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = std::sync::Arc::new(redb::Database::create(db_path).unwrap());
        (crate::storage::MemoryStorage::new(db).unwrap(), temp_dir)
    }

    #[test]
    fn test_cleanup_empty_sessions_deletes_empty_keeps_non_empty() {
        use crate::models::memory::{MemoryChunk, MemorySession};

        let (storage, _tmp) = create_test_memory_storage();
        let agent_id = "test-agent";

        // Create an empty session (no chunks)
        let empty_session =
            MemorySession::new(agent_id.to_string(), "Empty Session".to_string());
        storage.create_session(&empty_session).unwrap();

        // Create a session with one chunk
        let non_empty_session =
            MemorySession::new(agent_id.to_string(), "Non-empty Session".to_string());
        storage.create_session(&non_empty_session).unwrap();
        let chunk = MemoryChunk::new(agent_id.to_string(), "Some content".to_string())
            .with_session(non_empty_session.id.clone());
        storage.store_chunk(&chunk).unwrap();

        // Verify both sessions exist
        let sessions = storage.list_sessions(agent_id).unwrap();
        assert_eq!(sessions.len(), 2);

        // Run cleanup
        let deleted = cleanup_empty_sessions_for_agents(
            &storage,
            &[agent_id.to_string()],
        )
        .unwrap();

        assert_eq!(deleted, 1, "should delete exactly one empty session");

        // Verify only the non-empty session remains
        let remaining = storage.list_sessions(agent_id).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, non_empty_session.id);
    }

    #[test]
    fn test_cleanup_empty_sessions_no_agents_returns_zero() {
        let (storage, _tmp) = create_test_memory_storage();
        let deleted =
            cleanup_empty_sessions_for_agents(&storage, &[]).unwrap();
        assert_eq!(deleted, 0);
    }

    #[test]
    fn test_cleanup_empty_sessions_all_have_chunks() {
        use crate::models::memory::{MemoryChunk, MemorySession};

        let (storage, _tmp) = create_test_memory_storage();
        let agent_id = "agent-1";

        // Create two sessions, each with a chunk
        for i in 0..2 {
            let session = MemorySession::new(
                agent_id.to_string(),
                format!("Session {}", i),
            );
            storage.create_session(&session).unwrap();
            let chunk = MemoryChunk::new(
                agent_id.to_string(),
                format!("Content {}", i),
            )
            .with_session(session.id.clone());
            storage.store_chunk(&chunk).unwrap();
        }

        let deleted = cleanup_empty_sessions_for_agents(
            &storage,
            &[agent_id.to_string()],
        )
        .unwrap();

        assert_eq!(deleted, 0, "no sessions should be deleted");
        assert_eq!(storage.list_sessions(agent_id).unwrap().len(), 2);
    }

    #[test]
    fn test_cleanup_empty_sessions_multiple_agents() {
        use crate::models::memory::{MemoryChunk, MemorySession};

        let (storage, _tmp) = create_test_memory_storage();

        // Agent A: 1 empty session
        let sess_a =
            MemorySession::new("agent-a".to_string(), "A session".to_string());
        storage.create_session(&sess_a).unwrap();

        // Agent B: 1 session with chunk
        let sess_b =
            MemorySession::new("agent-b".to_string(), "B session".to_string());
        storage.create_session(&sess_b).unwrap();
        let chunk = MemoryChunk::new("agent-b".to_string(), "B content".to_string())
            .with_session(sess_b.id.clone());
        storage.store_chunk(&chunk).unwrap();

        let deleted = cleanup_empty_sessions_for_agents(
            &storage,
            &["agent-a".to_string(), "agent-b".to_string()],
        )
        .unwrap();

        assert_eq!(deleted, 1);
        assert!(
            storage.list_sessions("agent-a").unwrap().is_empty(),
            "agent-a empty session deleted"
        );
        assert_eq!(
            storage.list_sessions("agent-b").unwrap().len(),
            1,
            "agent-b session with chunk preserved"
        );
    }
}
