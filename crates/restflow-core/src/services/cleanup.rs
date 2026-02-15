use anyhow::Result;
use std::sync::Arc;

use crate::AppCore;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct CleanupReport {
    pub chat_sessions: usize,
    pub background_tasks: usize,
    pub checkpoints: usize,
    pub memory_chunks: usize,
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

    Ok(CleanupReport {
        chat_sessions,
        background_tasks,
        checkpoints,
        memory_chunks,
    })
}

fn retention_cutoff(now_ms: i64, retention_days: u32) -> Option<i64> {
    if retention_days == 0 {
        return None;
    }
    Some(now_ms - (retention_days as i64) * DAY_MS)
}

#[cfg(test)]
mod tests {
    use super::retention_cutoff;

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
}
