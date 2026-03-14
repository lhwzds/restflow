use crate::{Result, ToolError, ToolOutput};
use restflow_traits::store::BackgroundAgentControlRequest;

use super::BackgroundAgentTool;

fn execute_named_control(
    tool: &BackgroundAgentTool,
    id: String,
    action: &str,
    verb: &str,
) -> Result<ToolOutput> {
    tool.write_guard()?;
    let result = tool
        .store
        .control_background_agent(BackgroundAgentControlRequest {
            id,
            action: action.to_string(),
        })
        .map_err(|e| ToolError::Tool(format!("Failed to {verb} background agent: {e}.")))?;
    Ok(ToolOutput::success(result))
}

pub(super) fn execute_pause(tool: &BackgroundAgentTool, id: String) -> Result<ToolOutput> {
    execute_named_control(tool, id, "pause", "pause")
}

pub(super) fn execute_start(tool: &BackgroundAgentTool, id: String) -> Result<ToolOutput> {
    execute_named_control(tool, id, "start", "start")
}

pub(super) fn execute_resume(tool: &BackgroundAgentTool, id: String) -> Result<ToolOutput> {
    execute_named_control(tool, id, "resume", "resume")
}

pub(super) fn execute_stop(tool: &BackgroundAgentTool, id: String) -> Result<ToolOutput> {
    execute_named_control(tool, id, "stop", "stop")
}

pub(super) fn execute_run(tool: &BackgroundAgentTool, id: String) -> Result<ToolOutput> {
    execute_named_control(tool, id, "run_now", "run")
}

pub(super) fn execute_control(
    tool: &BackgroundAgentTool,
    id: String,
    action: String,
) -> Result<ToolOutput> {
    tool.write_guard()?;
    let result = tool
        .store
        .control_background_agent(BackgroundAgentControlRequest { id, action })
        .map_err(|e| ToolError::Tool(format!("Failed to control background agent: {e}.")))?;
    Ok(ToolOutput::success(result))
}
