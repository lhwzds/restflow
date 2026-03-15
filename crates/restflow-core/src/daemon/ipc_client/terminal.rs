#[cfg(unix)]
use super::*;
#[cfg(unix)]
use restflow_contracts::OkResponse;

#[cfg(unix)]
impl IpcClient {
    pub async fn list_terminal_sessions(&mut self) -> Result<Vec<TerminalSession>> {
        self.request_typed(IpcRequest::ListTerminalSessions).await
    }

    pub async fn get_terminal_session(&mut self, id: String) -> Result<TerminalSession> {
        self.request_typed(IpcRequest::GetTerminalSession { id })
            .await
    }

    pub async fn create_terminal_session(&mut self) -> Result<TerminalSession> {
        self.request_typed(IpcRequest::CreateTerminalSession).await
    }

    pub async fn rename_terminal_session(
        &mut self,
        id: String,
        name: String,
    ) -> Result<TerminalSession> {
        self.request_typed(IpcRequest::RenameTerminalSession { id, name })
            .await
    }

    pub async fn update_terminal_session(
        &mut self,
        id: String,
        name: Option<String>,
        working_directory: Option<String>,
        startup_command: Option<String>,
    ) -> Result<TerminalSession> {
        self.request_typed(IpcRequest::UpdateTerminalSession {
            id,
            name,
            working_directory,
            startup_command,
        })
        .await
    }

    pub async fn save_terminal_session(
        &mut self,
        session: TerminalSession,
    ) -> Result<TerminalSession> {
        self.request_typed(IpcRequest::SaveTerminalSession { session })
            .await
    }

    pub async fn delete_terminal_session(&mut self, id: String) -> Result<()> {
        let _: OkResponse = self
            .request_typed(IpcRequest::DeleteTerminalSession { id })
            .await?;
        Ok(())
    }

    pub async fn mark_all_terminal_sessions_stopped(&mut self) -> Result<usize> {
        self.request_typed(IpcRequest::MarkAllTerminalSessionsStopped)
            .await
    }
}
