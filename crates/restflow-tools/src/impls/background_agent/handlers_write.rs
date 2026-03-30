use crate::impls::operation_assessment::{enforce_confirmation, preview_output};
use restflow_contracts::request::{
    DurabilityMode as ContractDurabilityMode, ExecutionMode as ContractExecutionMode,
    MemoryConfig as ContractMemoryConfig, NotificationConfig as ContractNotificationConfig,
    ResourceLimits as ContractResourceLimits, TaskSchedule as ContractTaskSchedule,
};
use serde_json::json;

use crate::{Result, ToolError, ToolOutput};
use restflow_traits::OperationAssessmentIntent;
use restflow_traits::store::{
    BackgroundAgentConvertSessionRequest, BackgroundAgentCreateRequest,
    BackgroundAgentMessageRequest, BackgroundAgentUpdateRequest,
};

use super::BackgroundAgentTool;
use super::team::{delete_team, save_team_workers};
use super::types::BackgroundBatchWorkerSpec;

pub(super) async fn execute_save_team(
    tool: &BackgroundAgentTool,
    team: String,
    workers: Vec<BackgroundBatchWorkerSpec>,
    preview: bool,
    confirmation_token: Option<String>,
) -> Result<ToolOutput> {
    tool.write_guard()?;
    let assessor = tool.assessor()?;
    let assessment = assessor
        .assess_background_agent_template(
            "save_team",
            OperationAssessmentIntent::Save,
            workers
                .iter()
                .filter_map(|worker| worker.agent_id.clone())
                .collect(),
            true,
        )
        .await?;
    if preview {
        return Ok(preview_output(assessment));
    }
    enforce_confirmation(&assessment, confirmation_token.as_deref())?;
    let store = tool.team_store()?;
    let payload = save_team_workers(store.as_ref(), &team, &workers, true)?;
    Ok(ToolOutput::success(json!({
        "operation": "save_team",
        "result": payload
    })))
}

pub(super) fn execute_delete_team(tool: &BackgroundAgentTool, team: String) -> Result<ToolOutput> {
    tool.write_guard()?;
    let store = tool.team_store()?;
    let payload = delete_team(store.as_ref(), &team)?;
    Ok(ToolOutput::success(json!({
        "operation": "delete_team",
        "result": payload
    })))
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn execute_create(
    tool: &BackgroundAgentTool,
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
    confirmation_token: Option<String>,
) -> Result<ToolOutput> {
    tool.write_guard()?;
    let request = BackgroundAgentCreateRequest {
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
        confirmation_token,
    };
    let result = tool
        .store
        .create_background_agent(request)
        .map_err(|e| ToolError::Tool(format!("Failed to create background agent: {e}.")))?;
    Ok(ToolOutput::success(result))
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn execute_convert_session(
    tool: &BackgroundAgentTool,
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
    confirmation_token: Option<String>,
) -> Result<ToolOutput> {
    tool.write_guard()?;
    let request = BackgroundAgentConvertSessionRequest {
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
        confirmation_token,
    };
    let result = tool
        .store
        .convert_session_to_background_agent(request)
        .map_err(|e| {
            ToolError::Tool(format!(
                "Failed to convert session into background agent: {e}."
            ))
        })?;
    Ok(ToolOutput::success(result))
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn execute_promote_to_background(
    tool: &BackgroundAgentTool,
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
    confirmation_token: Option<String>,
) -> Result<ToolOutput> {
    tool.write_guard()?;
    let session_id = session_id.ok_or_else(|| {
        ToolError::Tool(
            "promote_to_background requires session_id (runtime should auto-inject it for interactive chat sessions)"
                .to_string(),
        )
    })?;
    let request = BackgroundAgentConvertSessionRequest {
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
        confirmation_token,
    };
    let result = tool
        .store
        .convert_session_to_background_agent(request)
        .map_err(|e| {
            ToolError::Tool(format!(
                "Failed to promote session into background agent: {e}."
            ))
        })?;
    Ok(ToolOutput::success(result))
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn execute_update(
    tool: &BackgroundAgentTool,
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
    confirmation_token: Option<String>,
) -> Result<ToolOutput> {
    tool.write_guard()?;
    let request = BackgroundAgentUpdateRequest {
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
        confirmation_token,
    };
    let result = tool
        .store
        .update_background_agent(request)
        .map_err(|e| ToolError::Tool(format!("Failed to update background agent: {e}.")))?;
    Ok(ToolOutput::success(result))
}

pub(super) fn execute_delete(tool: &BackgroundAgentTool, id: String) -> Result<ToolOutput> {
    tool.write_guard()?;
    let result = tool
        .store
        .delete_background_agent(&id)
        .map_err(|e| ToolError::Tool(format!("Failed to delete background agent: {e}.")))?;
    Ok(ToolOutput::success(result))
}

pub(super) fn execute_send_message(
    tool: &BackgroundAgentTool,
    id: String,
    message: String,
    source: Option<String>,
) -> Result<ToolOutput> {
    tool.write_guard()?;
    let result = tool
        .store
        .send_background_agent_message(BackgroundAgentMessageRequest {
            id,
            message,
            source,
        })
        .map_err(|e| ToolError::Tool(format!("Failed to send message background agent: {e}.")))?;
    Ok(ToolOutput::success(result))
}
