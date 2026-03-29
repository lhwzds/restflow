use super::super::*;
use restflow_contracts::{CleanupReportResponse, SessionSourceMigrationResponse};

impl IpcServer {
    pub(super) async fn handle_run_cleanup(core: &Arc<AppCore>) -> IpcResponse {
        match crate::services::cleanup::run_cleanup(core).await {
            Ok(report) => IpcResponse::success(CleanupReportResponse {
                chat_sessions: report.chat_sessions,
                background_tasks: report.background_tasks,
                checkpoints: report.checkpoints,
                memory_chunks: report.memory_chunks,
                memory_sessions: report.memory_sessions,
                vector_orphans: report.vector_orphans,
                daemon_log_files: report.daemon_log_files,
            }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_migrate_session_sources(
        core: &Arc<AppCore>,
        dry_run: bool,
    ) -> IpcResponse {
        match core
            .storage
            .chat_sessions
            .migrate_legacy_channel_sources(dry_run)
        {
            Ok(stats) => IpcResponse::success(SessionSourceMigrationResponse {
                dry_run,
                scanned: stats.scanned,
                migrated: stats.migrated,
                skipped: stats.skipped,
                failed: stats.failed,
            }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }
}
