#[cfg(unix)]
use super::*;
#[cfg(unix)]
use crate::boundary::background_agent::{core_patch_to_contract, core_spec_to_contract};
#[cfg(unix)]
use crate::daemon::request_mapper::to_contract;
use restflow_contracts::DeleteWithIdResponse;
#[cfg(unix)]
use restflow_contracts::request::TaskFromSessionRequest;

#[cfg(unix)]
impl IpcClient {
    pub async fn list_tasks(&mut self, status: Option<String>) -> Result<Vec<BackgroundAgent>> {
        self.request_typed(IpcRequest::ListTasks { status }).await
    }

    pub async fn get_task(&mut self, id: String) -> Result<Option<BackgroundAgent>> {
        self.request_optional(IpcRequest::GetTask { id }).await
    }

    pub async fn create_task(&mut self, spec: BackgroundAgentSpec) -> Result<BackgroundAgent> {
        let spec = core_spec_to_contract(spec)?;
        self.request_typed(IpcRequest::CreateTask { spec }).await
    }

    pub async fn create_task_from_session(
        &mut self,
        request: TaskFromSessionRequest,
    ) -> Result<crate::models::BackgroundAgentConversionResult> {
        self.request_typed(IpcRequest::CreateTaskFromSession { request })
            .await
    }

    pub async fn update_task(
        &mut self,
        id: String,
        patch: BackgroundAgentPatch,
    ) -> Result<BackgroundAgent> {
        let patch = core_patch_to_contract(patch)?;
        self.request_typed(IpcRequest::UpdateTask { id, patch })
            .await
    }

    pub async fn delete_task(&mut self, id: String) -> Result<DeleteWithIdResponse> {
        self.request_typed(IpcRequest::DeleteTask { id }).await
    }

    pub async fn control_task(
        &mut self,
        id: String,
        action: BackgroundAgentControlAction,
    ) -> Result<BackgroundAgent> {
        let action = to_contract(action)?;
        self.request_typed(IpcRequest::ControlTask { id, action })
            .await
    }

    pub async fn get_task_history(&mut self, id: String) -> Result<Vec<BackgroundAgentEvent>> {
        self.request_typed(IpcRequest::GetTaskHistory { id }).await
    }

    pub async fn list_background_agents(
        &mut self,
        status: Option<String>,
    ) -> Result<Vec<BackgroundAgent>> {
        self.list_tasks(status).await
    }

    pub async fn get_background_agent(&mut self, id: String) -> Result<Option<BackgroundAgent>> {
        self.get_task(id).await
    }

    pub async fn create_background_agent(
        &mut self,
        spec: BackgroundAgentSpec,
    ) -> Result<BackgroundAgent> {
        self.create_task(spec).await
    }

    pub async fn convert_session_to_background_agent(
        &mut self,
        request: TaskFromSessionRequest,
    ) -> Result<crate::models::BackgroundAgentConversionResult> {
        self.create_task_from_session(request).await
    }

    pub async fn update_background_agent(
        &mut self,
        id: String,
        patch: BackgroundAgentPatch,
    ) -> Result<BackgroundAgent> {
        self.update_task(id, patch).await
    }

    pub async fn delete_background_agent(&mut self, id: String) -> Result<DeleteWithIdResponse> {
        self.delete_task(id).await
    }

    pub async fn control_background_agent(
        &mut self,
        id: String,
        action: BackgroundAgentControlAction,
    ) -> Result<BackgroundAgent> {
        self.control_task(id, action).await
    }

    pub async fn get_background_agent_history(
        &mut self,
        id: String,
    ) -> Result<Vec<BackgroundAgentEvent>> {
        self.get_task_history(id).await
    }
}
