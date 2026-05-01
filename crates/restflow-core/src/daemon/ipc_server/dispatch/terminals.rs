use super::super::*;
use restflow_contracts::OkResponse;

impl IpcServer {
    pub(super) async fn handle_list_terminal_sessions(core: &Arc<AppCore>) -> IpcResponse {
        match core.storage.terminal_sessions.list() {
            Ok(sessions) => IpcResponse::success(sessions),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_terminal_session(
        core: &Arc<AppCore>,
        id: String,
    ) -> IpcResponse {
        match core.storage.terminal_sessions.get(&id) {
            Ok(Some(session)) => IpcResponse::success(session),
            Ok(None) => IpcResponse::not_found("Terminal session"),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_create_terminal_session(core: &Arc<AppCore>) -> IpcResponse {
        let name = match core.storage.terminal_sessions.get_next_name() {
            Ok(name) => name,
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        let id = format!("terminal-{}", Uuid::new_v4());
        let session = TerminalSession::new(id, name);
        match core.storage.terminal_sessions.create(&session) {
            Ok(()) => IpcResponse::success(session),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_rename_terminal_session(
        core: &Arc<AppCore>,
        id: String,
        name: String,
    ) -> IpcResponse {
        let mut session = match core.storage.terminal_sessions.get(&id) {
            Ok(Some(session)) => session,
            Ok(None) => return IpcResponse::not_found("Terminal session"),
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        session.rename(name);
        match core.storage.terminal_sessions.update(&id, &session) {
            Ok(()) => IpcResponse::success(session),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_update_terminal_session(
        core: &Arc<AppCore>,
        id: String,
        name: Option<String>,
        working_directory: Option<String>,
        startup_command: Option<String>,
    ) -> IpcResponse {
        let mut session = match core.storage.terminal_sessions.get(&id) {
            Ok(Some(session)) => session,
            Ok(None) => return IpcResponse::not_found("Terminal session"),
            Err(err) => return IpcResponse::error(500, err.to_string()),
        };
        if let Some(name) = name {
            session.rename(name);
        }
        session.set_config(working_directory, startup_command);
        match core.storage.terminal_sessions.update(&id, &session) {
            Ok(()) => IpcResponse::success(session),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_save_terminal_session(
        core: &Arc<AppCore>,
        session: TerminalSession,
    ) -> IpcResponse {
        match core.storage.terminal_sessions.update(&session.id, &session) {
            Ok(()) => IpcResponse::success(session),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_delete_terminal_session(
        core: &Arc<AppCore>,
        id: String,
    ) -> IpcResponse {
        match core.storage.terminal_sessions.delete(&id) {
            Ok(()) => IpcResponse::success(OkResponse { ok: true }),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_mark_all_terminal_sessions_stopped(
        core: &Arc<AppCore>,
    ) -> IpcResponse {
        match core.storage.terminal_sessions.mark_all_stopped() {
            Ok(count) => IpcResponse::success(count),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }
}
