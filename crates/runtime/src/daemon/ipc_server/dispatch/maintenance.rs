use super::super::*;
use types::CleanupReportResponse;

impl IpcServer {
    pub(super) async fn handle_run_cleanup(core: &Arc<AppCore>) -> IpcResponse {
        match crate::services::cleanup::run_cleanup(core).await {
            Ok(report) => IpcResponse::success(CleanupReportResponse {
                chat_sessions: report.chat_sessions,
                tasks: report.tasks,
                audit_events: report.audit_events,
                daemon_log_files: report.daemon_log_files,
            }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }
}
