#[cfg(unix)]
use super::*;
#[cfg(unix)]
use crate::boundary::background_agent::{
    core_patch_to_contract, core_spec_to_contract, store_convert_request_to_contract,
};
#[cfg(unix)]
use crate::daemon::request_mapper::to_contract;
use restflow_contracts::DeleteWithIdResponse;
#[cfg(unix)]
use restflow_traits::BackgroundAgentCommandOutcome;
#[cfg(unix)]
use restflow_traits::store::{BackgroundAgentConvertSessionRequest, BackgroundAgentDeleteRequest};

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
        preview: bool,
        confirmation_token: Option<String>,
    ) -> Result<BackgroundAgentCommandOutcome<BackgroundAgent>> {
        let spec = core_spec_to_contract(spec)?;
        self.request_typed(IpcRequest::CreateBackgroundAgent {
            spec,
            preview,
            confirmation_token,
        })
        .await
    }

    pub async fn convert_session_to_background_agent(
        &mut self,
        request: BackgroundAgentConvertSessionRequest,
        preview: bool,
        confirmation_token: Option<String>,
    ) -> Result<BackgroundAgentCommandOutcome<crate::models::BackgroundAgentConversionResult>> {
        let request = store_convert_request_to_contract(request)?;
        self.request_typed(IpcRequest::ConvertSessionToBackgroundAgent {
            request,
            preview,
            confirmation_token,
        })
        .await
    }

    pub async fn update_background_agent(
        &mut self,
        id: String,
        patch: BackgroundAgentPatch,
        preview: bool,
        confirmation_token: Option<String>,
    ) -> Result<BackgroundAgentCommandOutcome<BackgroundAgent>> {
        let patch = core_patch_to_contract(patch)?;
        self.request_typed(IpcRequest::UpdateBackgroundAgent {
            id,
            patch,
            preview,
            confirmation_token,
        })
        .await
    }

    pub async fn delete_background_agent(
        &mut self,
        id: String,
        preview: bool,
        confirmation_token: Option<String>,
    ) -> Result<BackgroundAgentCommandOutcome<DeleteWithIdResponse>> {
        let request = BackgroundAgentDeleteRequest {
            id,
            preview,
            confirmation_token,
        };
        self.request_typed(IpcRequest::DeleteBackgroundAgent {
            id: request.id,
            preview: request.preview,
            confirmation_token: request.confirmation_token,
        })
        .await
    }

    pub async fn control_background_agent(
        &mut self,
        id: String,
        action: BackgroundAgentControlAction,
        preview: bool,
        confirmation_token: Option<String>,
    ) -> Result<BackgroundAgentCommandOutcome<BackgroundAgent>> {
        let action = to_contract(action)?;
        self.request_typed(IpcRequest::ControlBackgroundAgent {
            id,
            action,
            preview,
            confirmation_token,
        })
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
