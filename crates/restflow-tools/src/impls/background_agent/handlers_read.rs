use serde_json::json;

use crate::{Result, ToolError, ToolOutput};
use restflow_traits::store::{
    BackgroundAgentDeliverableListRequest, BackgroundAgentMessageListRequest,
    BackgroundAgentProgressRequest, BackgroundAgentTraceListRequest,
    BackgroundAgentTraceReadRequest,
};

use super::BackgroundAgentTool;
use super::team::{get_team, list_teams};

pub(super) fn execute_list(
    tool: &BackgroundAgentTool,
    status: Option<String>,
) -> Result<ToolOutput> {
    let result = tool
        .store
        .list_background_agents(status)
        .map_err(|e| ToolError::Tool(format!("Failed to list background agent: {e}.")))?;
    Ok(ToolOutput::success(result))
}

pub(super) fn execute_progress(
    tool: &BackgroundAgentTool,
    id: String,
    event_limit: Option<usize>,
) -> Result<ToolOutput> {
    let result = tool
        .store
        .get_background_agent_progress(BackgroundAgentProgressRequest { id, event_limit })
        .map_err(|e| ToolError::Tool(format!("Failed to get background agent: {e}.")))?;
    Ok(ToolOutput::success(result))
}

pub(super) fn execute_list_messages(
    tool: &BackgroundAgentTool,
    id: String,
    limit: Option<usize>,
) -> Result<ToolOutput> {
    let result = tool
        .store
        .list_background_agent_messages(BackgroundAgentMessageListRequest { id, limit })
        .map_err(|e| ToolError::Tool(format!("Failed to list messages background agent: {e}.")))?;
    Ok(ToolOutput::success(result))
}

pub(super) fn execute_list_deliverables(
    tool: &BackgroundAgentTool,
    id: String,
) -> Result<ToolOutput> {
    let result = tool
        .store
        .list_background_agent_deliverables(BackgroundAgentDeliverableListRequest { id })
        .map_err(|e| {
            ToolError::Tool(format!(
                "Failed to list deliverables background agent: {e}."
            ))
        })?;
    Ok(ToolOutput::success(result))
}

pub(super) fn execute_list_traces(
    tool: &BackgroundAgentTool,
    id: Option<String>,
    limit: Option<usize>,
) -> Result<ToolOutput> {
    let result = tool
        .store
        .list_background_agent_traces(BackgroundAgentTraceListRequest { id, limit })
        .map_err(|e| {
            ToolError::Tool(format!("Failed to list traces for background agent: {e}."))
        })?;
    Ok(ToolOutput::success(result))
}

pub(super) fn execute_read_trace(
    tool: &BackgroundAgentTool,
    trace_id: String,
    line_limit: Option<usize>,
) -> Result<ToolOutput> {
    let result = tool
        .store
        .read_background_agent_trace(BackgroundAgentTraceReadRequest {
            trace_id,
            line_limit,
        })
        .map_err(|e| ToolError::Tool(format!("Failed to read trace for background agent: {e}.")))?;
    Ok(ToolOutput::success(result))
}

pub(super) fn execute_list_teams(tool: &BackgroundAgentTool) -> Result<ToolOutput> {
    let store = tool.team_store()?;
    let payload = list_teams(store.as_ref())?;
    Ok(ToolOutput::success(json!({
        "operation": "list_teams",
        "teams": payload
    })))
}

pub(super) fn execute_get_team(tool: &BackgroundAgentTool, team: String) -> Result<ToolOutput> {
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
