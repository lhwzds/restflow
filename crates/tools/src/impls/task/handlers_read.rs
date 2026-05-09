use crate::{Result, ToolError, ToolOutput};
use types::store::{
    TaskArtifactListRequest, TaskMessageListRequest, TaskProgressRequest, TaskStore,
};

use super::TaskTool;

pub(super) fn execute_list(tool: &TaskTool, status: Option<String>) -> Result<ToolOutput> {
    let result = tool
        .store
        .list_tasks(status)
        .map_err(|e| ToolError::Tool(format!("Failed to list tasks: {e}.")))?;
    Ok(ToolOutput::success(result))
}

pub(super) fn execute_progress(
    tool: &TaskTool,
    id: String,
    event_limit: Option<usize>,
) -> Result<ToolOutput> {
    let result =
        TaskStore::get_task_progress(tool.store.as_ref(), TaskProgressRequest { id, event_limit })
            .map_err(|e| ToolError::Tool(format!("Failed to get task progress: {e}.")))?;
    Ok(ToolOutput::success(result))
}

pub(super) fn execute_list_messages(
    tool: &TaskTool,
    id: String,
    limit: Option<usize>,
) -> Result<ToolOutput> {
    let result =
        TaskStore::list_task_messages(tool.store.as_ref(), TaskMessageListRequest { id, limit })
            .map_err(|e| ToolError::Tool(format!("Failed to list task messages: {e}.")))?;
    Ok(ToolOutput::success(result))
}

pub(super) fn execute_list_artifacts(tool: &TaskTool, id: String) -> Result<ToolOutput> {
    let result =
        TaskStore::list_task_artifacts(tool.store.as_ref(), TaskArtifactListRequest { id })
            .map_err(|e| ToolError::Tool(format!("Failed to list task artifacts: {e}.")))?;
    Ok(ToolOutput::success(result))
}
