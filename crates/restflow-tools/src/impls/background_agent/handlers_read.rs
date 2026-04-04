use serde_json::json;

use crate::{Result, ToolError, ToolOutput};
use restflow_traits::store::{
    TaskDeliverableListRequest, TaskMessageListRequest, TaskProgressRequest, TaskStore,
    TaskTraceListRequest, TaskTraceReadRequest,
};

use super::TaskTool;
use super::team::{get_team, list_teams};

pub(super) fn execute_list(tool: &TaskTool, status: Option<String>) -> Result<ToolOutput> {
    let result = TaskStore::list_tasks(tool.store.as_ref(), status)
        .map_err(|e| ToolError::Tool(format!("Failed to list background agent: {e}.")))?;
    Ok(ToolOutput::success(result))
}

pub(super) fn execute_progress(
    tool: &TaskTool,
    id: String,
    event_limit: Option<usize>,
) -> Result<ToolOutput> {
    let result =
        TaskStore::get_task_progress(tool.store.as_ref(), TaskProgressRequest { id, event_limit })
            .map_err(|e| ToolError::Tool(format!("Failed to get background agent: {e}.")))?;
    Ok(ToolOutput::success(result))
}

pub(super) fn execute_list_messages(
    tool: &TaskTool,
    id: String,
    limit: Option<usize>,
) -> Result<ToolOutput> {
    let result =
        TaskStore::list_task_messages(tool.store.as_ref(), TaskMessageListRequest { id, limit })
            .map_err(|e| {
                ToolError::Tool(format!("Failed to list messages background agent: {e}."))
            })?;
    Ok(ToolOutput::success(result))
}

pub(super) fn execute_list_deliverables(tool: &TaskTool, id: String) -> Result<ToolOutput> {
    let result =
        TaskStore::list_task_deliverables(tool.store.as_ref(), TaskDeliverableListRequest { id })
            .map_err(|e| {
            ToolError::Tool(format!(
                "Failed to list deliverables background agent: {e}."
            ))
        })?;
    Ok(ToolOutput::success(result))
}

pub(super) fn execute_list_traces(
    tool: &TaskTool,
    id: Option<String>,
    limit: Option<usize>,
) -> Result<ToolOutput> {
    let result =
        TaskStore::list_task_traces(tool.store.as_ref(), TaskTraceListRequest { id, limit })
            .map_err(|e| {
                ToolError::Tool(format!("Failed to list traces for background agent: {e}."))
            })?;
    Ok(ToolOutput::success(result))
}

pub(super) fn execute_read_trace(
    tool: &TaskTool,
    trace_id: String,
    line_limit: Option<usize>,
) -> Result<ToolOutput> {
    let result = TaskStore::read_task_trace(
        tool.store.as_ref(),
        TaskTraceReadRequest {
            trace_id,
            line_limit,
        },
    )
    .map_err(|e| ToolError::Tool(format!("Failed to read trace for background agent: {e}.")))?;
    Ok(ToolOutput::success(result))
}

pub(super) fn execute_list_teams(tool: &TaskTool) -> Result<ToolOutput> {
    let store = tool.team_store()?;
    let payload = list_teams(store.as_ref())?;
    Ok(ToolOutput::success(json!({
        "operation": "list_teams",
        "teams": payload
    })))
}

pub(super) fn execute_get_team(tool: &TaskTool, team: String) -> Result<ToolOutput> {
    let store = tool.team_store()?;
    let payload = get_team(store.as_ref(), &team)?;
    Ok(ToolOutput::success(json!({
        "operation": "get_team",
        "team": payload["team"].clone(),
        "version": payload["version"].clone(),
        "created_at": payload["created_at"].clone(),
        "updated_at": payload["updated_at"].clone(),
        "member_groups": payload["member_groups"].clone(),
        "total_instances": payload["total_instances"].clone(),
        "members": payload["members"].clone()
    })))
}
