#[cfg(unix)]
use super::*;

#[cfg(unix)]
impl IpcClient {
    pub async fn list_background_agents(
        &mut self,
        status: Option<String>,
    ) -> Result<Vec<BackgroundAgent>> {
        self.request_typed(IpcRequest::ListBackgroundAgents { status })
            .await
    }

    pub async fn get_background_agent(&mut self, id: String) -> Result<Option<BackgroundAgent>> {
        match self.request(IpcRequest::GetBackgroundAgent { id }).await? {
            IpcResponse::Success(value) => Ok(Some(serde_json::from_value(value)?)),
            IpcResponse::Error { code: 404, .. } => Ok(None),
            IpcResponse::Error {
                code,
                message,
                details,
            } => {
                bail!(Self::format_ipc_error(code, &message, details))
            }
            IpcResponse::Pong => bail!("Unexpected Pong response"),
        }
    }

    pub async fn create_background_agent(
        &mut self,
        spec: BackgroundAgentSpec,
    ) -> Result<BackgroundAgent> {
        self.request_typed(IpcRequest::CreateBackgroundAgent { spec })
            .await
    }

    pub async fn update_background_agent(
        &mut self,
        id: String,
        patch: BackgroundAgentPatch,
    ) -> Result<BackgroundAgent> {
        self.request_typed(IpcRequest::UpdateBackgroundAgent { id, patch })
            .await
    }

    pub async fn delete_background_agent(&mut self, id: String) -> Result<bool> {
        #[derive(serde::Deserialize)]
        struct DeleteResponse {
            deleted: bool,
        }
        let resp: DeleteResponse = self
            .request_typed(IpcRequest::DeleteBackgroundAgent { id })
            .await?;
        Ok(resp.deleted)
    }

    pub async fn control_background_agent(
        &mut self,
        id: String,
        action: BackgroundAgentControlAction,
    ) -> Result<BackgroundAgent> {
        self.request_typed(IpcRequest::ControlBackgroundAgent { id, action })
            .await
    }

    pub async fn get_background_agent_history(
        &mut self,
        id: String,
    ) -> Result<Vec<BackgroundAgentEvent>> {
        self.request_typed(IpcRequest::GetBackgroundAgentHistory { id })
            .await
    }
}
