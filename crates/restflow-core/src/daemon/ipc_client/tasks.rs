#[cfg(unix)]
use super::*;
#[cfg(unix)]
use crate::boundary::task::{core_patch_to_contract, core_spec_to_contract};
#[cfg(unix)]
use crate::daemon::request_mapper::to_contract;
#[cfg(unix)]
use restflow_traits::DeleteWithIdResponse;
#[cfg(unix)]
use restflow_traits::request::TaskFromSessionRequest;

#[cfg(unix)]
impl IpcClient {
    pub async fn list_tasks(&mut self, status: Option<String>) -> Result<Vec<Task>> {
        self.request_typed(IpcRequest::ListTasks { status }).await
    }

    pub async fn get_task(&mut self, id: String) -> Result<Option<Task>> {
        self.request_optional(IpcRequest::GetTask { id }).await
    }

    pub async fn create_task(&mut self, spec: TaskSpec) -> Result<Task> {
        let spec = core_spec_to_contract(spec)?;
        self.request_typed(IpcRequest::CreateTask { spec }).await
    }

    pub async fn create_task_from_session(
        &mut self,
        request: TaskFromSessionRequest,
    ) -> Result<crate::models::TaskConversionResult> {
        self.request_typed(IpcRequest::CreateTaskFromSession { request })
            .await
    }

    pub async fn update_task(&mut self, id: String, patch: TaskPatch) -> Result<Task> {
        let patch = core_patch_to_contract(patch)?;
        self.request_typed(IpcRequest::UpdateTask { id, patch })
            .await
    }

    pub async fn delete_task(
        &mut self,
        id: String,
        approval_id: Option<String>,
    ) -> Result<DeleteWithIdResponse> {
        self.request_typed(IpcRequest::DeleteTask { id, approval_id })
            .await
    }

    pub async fn control_task(
        &mut self,
        id: String,
        action: TaskControlAction,
        approval_id: Option<String>,
    ) -> Result<Task> {
        let action = to_contract(action)?;
        self.request_typed(IpcRequest::ControlTask {
            id,
            action,
            approval_id,
        })
        .await
    }

    pub async fn get_task_history(&mut self, id: String) -> Result<Vec<TaskEvent>> {
        self.request_typed(IpcRequest::GetTaskHistory { id }).await
    }
}
