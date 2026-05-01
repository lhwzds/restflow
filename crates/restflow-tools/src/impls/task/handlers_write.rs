use crate::impls::operation_assessment::guarded_confirmation_required_output;
use crate::{Result, ToolError, ToolOutput};
use restflow_contracts::request::{
    DurabilityMode as ContractDurabilityMode, ExecutionMode as ContractExecutionMode,
    MemoryConfig as ContractMemoryConfig, NotificationConfig as ContractNotificationConfig,
    ResourceLimits as ContractResourceLimits, TaskSchedule as ContractTaskSchedule,
};
use restflow_traits::store::{
    TaskConvertSessionRequest, TaskCreateRequest, TaskDeleteRequest, TaskMessageRequest, TaskStore,
    TaskUpdateRequest,
};

use super::TaskTool;

#[allow(clippy::too_many_arguments)]
pub(super) async fn execute_create(
    tool: &TaskTool,
    name: String,
    agent_id: String,
    chat_session_id: Option<String>,
    schedule: ContractTaskSchedule,
    input: Option<String>,
    input_template: Option<String>,
    timeout_secs: Option<u64>,
    durability_mode: Option<ContractDurabilityMode>,
    memory: Option<ContractMemoryConfig>,
    memory_scope: Option<String>,
    resource_limits: Option<ContractResourceLimits>,
    preview: bool,
    approval_id: Option<String>,
) -> Result<ToolOutput> {
    tool.write_guard()?;
    let request = TaskCreateRequest {
        name,
        agent_id,
        chat_session_id,
        schedule,
        input,
        input_template,
        timeout_secs,
        durability_mode,
        memory,
        memory_scope,
        resource_limits,
        preview,
        approval_id,
    };
    let result = TaskStore::create_task(tool.store.as_ref(), request)
        .map_err(|e| ToolError::Tool(format!("Failed to create task: {e}.")))?;
    if let Some(output) = guarded_confirmation_required_output(&result) {
        return Ok(output);
    }
    Ok(ToolOutput::success(result))
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn execute_convert_session(
    tool: &TaskTool,
    session_id: String,
    name: Option<String>,
    schedule: Option<ContractTaskSchedule>,
    input: Option<String>,
    timeout_secs: Option<u64>,
    durability_mode: Option<ContractDurabilityMode>,
    memory: Option<ContractMemoryConfig>,
    memory_scope: Option<String>,
    resource_limits: Option<ContractResourceLimits>,
    run_now: Option<bool>,
    preview: bool,
    approval_id: Option<String>,
) -> Result<ToolOutput> {
    tool.write_guard()?;
    let request = TaskConvertSessionRequest {
        session_id,
        name,
        schedule,
        input,
        timeout_secs,
        durability_mode,
        memory,
        memory_scope,
        resource_limits,
        run_now,
        preview,
        approval_id,
    };
    let result = TaskStore::convert_session_to_task(tool.store.as_ref(), request)
        .map_err(|e| ToolError::Tool(format!("Failed to convert session into task: {e}.")))?;
    if let Some(output) = guarded_confirmation_required_output(&result) {
        return Ok(output);
    }
    Ok(ToolOutput::success(result))
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn execute_promote_to_background(
    tool: &TaskTool,
    session_id: Option<String>,
    name: Option<String>,
    schedule: Option<ContractTaskSchedule>,
    input: Option<String>,
    timeout_secs: Option<u64>,
    durability_mode: Option<ContractDurabilityMode>,
    memory: Option<ContractMemoryConfig>,
    memory_scope: Option<String>,
    resource_limits: Option<ContractResourceLimits>,
    run_now: Option<bool>,
    preview: bool,
    approval_id: Option<String>,
) -> Result<ToolOutput> {
    tool.write_guard()?;
    let session_id = session_id.ok_or_else(|| {
        ToolError::Tool(
            "promote_to_background requires session_id (runtime should auto-inject it for interactive chat sessions)"
                .to_string(),
        )
    })?;
    let request = TaskConvertSessionRequest {
        session_id,
        name,
        schedule,
        input,
        timeout_secs,
        durability_mode,
        memory,
        memory_scope,
        resource_limits,
        run_now,
        preview,
        approval_id,
    };
    let result = TaskStore::convert_session_to_task(tool.store.as_ref(), request)
        .map_err(|e| ToolError::Tool(format!("Failed to promote session into task: {e}.")))?;
    if let Some(output) = guarded_confirmation_required_output(&result) {
        return Ok(output);
    }
    Ok(ToolOutput::success(result))
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn execute_update(
    tool: &TaskTool,
    id: String,
    name: Option<String>,
    description: Option<String>,
    agent_id: Option<String>,
    chat_session_id: Option<String>,
    input: Option<String>,
    input_template: Option<String>,
    schedule: Option<ContractTaskSchedule>,
    notification: Option<ContractNotificationConfig>,
    execution_mode: Option<ContractExecutionMode>,
    timeout_secs: Option<u64>,
    durability_mode: Option<ContractDurabilityMode>,
    memory: Option<ContractMemoryConfig>,
    memory_scope: Option<String>,
    resource_limits: Option<ContractResourceLimits>,
    preview: bool,
    approval_id: Option<String>,
) -> Result<ToolOutput> {
    tool.write_guard()?;
    let request = TaskUpdateRequest {
        id,
        name,
        description,
        agent_id,
        chat_session_id,
        input,
        input_template,
        schedule,
        notification,
        execution_mode,
        timeout_secs,
        durability_mode,
        memory,
        memory_scope,
        resource_limits,
        preview,
        approval_id,
    };
    let result = TaskStore::update_task(tool.store.as_ref(), request)
        .map_err(|e| ToolError::Tool(format!("Failed to update task: {e}.")))?;
    if let Some(output) = guarded_confirmation_required_output(&result) {
        return Ok(output);
    }
    Ok(ToolOutput::success(result))
}

pub(super) async fn execute_delete(
    tool: &TaskTool,
    id: String,
    preview: bool,
    approval_id: Option<String>,
) -> Result<ToolOutput> {
    tool.write_guard()?;
    let request = TaskDeleteRequest {
        id,
        preview,
        approval_id,
    };
    let result = TaskStore::delete_task(tool.store.as_ref(), request)
        .map_err(|e| ToolError::Tool(format!("Failed to delete task: {e}.")))?;
    if let Some(output) = guarded_confirmation_required_output(&result) {
        return Ok(output);
    }
    Ok(ToolOutput::success(result))
}

pub(super) fn execute_send_message(
    tool: &TaskTool,
    id: String,
    message: String,
    source: Option<String>,
) -> Result<ToolOutput> {
    tool.write_guard()?;
    let result = TaskStore::send_task_message(
        tool.store.as_ref(),
        TaskMessageRequest {
            id,
            message,
            source,
        },
    )
    .map_err(|e| ToolError::Tool(format!("Failed to send task message: {e}.")))?;
    Ok(ToolOutput::success(result))
}
