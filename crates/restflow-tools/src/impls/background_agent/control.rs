use crate::impls::operation_assessment::guarded_confirmation_required_output;
use crate::{Result, ToolError, ToolOutput};
use restflow_traits::store::BackgroundAgentControlRequest;

use super::BackgroundAgentTool;

async fn execute_named_control(
    tool: &BackgroundAgentTool,
    id: String,
    action: &str,
    verb: &str,
    preview: bool,
    approval_id: Option<String>,
) -> Result<ToolOutput> {
    tool.write_guard()?;
    let request = BackgroundAgentControlRequest {
        id,
        action: action.to_string(),
        preview,
        confirmation_token: approval_id,
    };
    let result = tool
        .store
        .control_background_agent(request)
        .map_err(|e| ToolError::Tool(format!("Failed to {verb} background agent: {e}.")))?;
    if let Some(output) = guarded_confirmation_required_output(&result) {
        return Ok(output);
    }
    Ok(ToolOutput::success(result))
}

pub(super) async fn execute_pause(tool: &BackgroundAgentTool, id: String) -> Result<ToolOutput> {
    execute_named_control(tool, id, "pause", "pause", false, None).await
}

pub(super) async fn execute_start(tool: &BackgroundAgentTool, id: String) -> Result<ToolOutput> {
    execute_named_control(tool, id, "start", "start", false, None).await
}

pub(super) async fn execute_resume(tool: &BackgroundAgentTool, id: String) -> Result<ToolOutput> {
    execute_named_control(tool, id, "resume", "resume", false, None).await
}

pub(super) async fn execute_stop(tool: &BackgroundAgentTool, id: String) -> Result<ToolOutput> {
    execute_named_control(tool, id, "stop", "stop", false, None).await
}

pub(super) async fn execute_run(
    tool: &BackgroundAgentTool,
    id: String,
    preview: bool,
    approval_id: Option<String>,
) -> Result<ToolOutput> {
    execute_named_control(tool, id, "run_now", "run", preview, approval_id).await
}

pub(super) async fn execute_control(
    tool: &BackgroundAgentTool,
    id: String,
    action: String,
    preview: bool,
    approval_id: Option<String>,
) -> Result<ToolOutput> {
    execute_named_control(tool, id, &action, "control", preview, approval_id).await
}
