#[cfg(unix)]
use super::*;
#[cfg(unix)]
use crate::boundary::background_agent::{core_patch_to_contract, core_spec_to_contract};
#[cfg(unix)]
use crate::daemon::request_mapper::to_contract;
use restflow_contracts::DeleteWithIdResponse;
#[cfg(unix)]
use restflow_contracts::request::BackgroundAgentConvertSessionRequest;

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
        self.request_optional(IpcRequest::GetBackgroundAgent { id })
            .await
    }

    pub async fn create_background_agent(
        &mut self,
        spec: BackgroundAgentSpec,
    ) -> Result<BackgroundAgent> {
        let spec = core_spec_to_contract(spec)?;
        self.request_typed(IpcRequest::CreateBackgroundAgent { spec })
            .await
    }

    pub async fn convert_session_to_background_agent(
        &mut self,
        request: BackgroundAgentConvertSessionRequest,
    ) -> Result<crate::models::BackgroundAgentConversionResult> {
        self.request_typed(IpcRequest::ConvertSessionToBackgroundAgent { request })
            .await
    }

    pub async fn update_background_agent(
        &mut self,
        id: String,
        patch: BackgroundAgentPatch,
    ) -> Result<BackgroundAgent> {
        let patch = core_patch_to_contract(patch)?;
        self.request_typed(IpcRequest::UpdateBackgroundAgent { id, patch })
            .await
    }

    pub async fn delete_background_agent(&mut self, id: String) -> Result<DeleteWithIdResponse> {
        self.request_typed(IpcRequest::DeleteBackgroundAgent { id })
            .await
    }

    pub async fn control_background_agent(
        &mut self,
        id: String,
        action: BackgroundAgentControlAction,
    ) -> Result<BackgroundAgent> {
        let action = to_contract(action)?;
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
