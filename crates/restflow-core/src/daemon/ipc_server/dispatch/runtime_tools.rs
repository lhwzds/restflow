use super::super::runtime::{build_agent_system_prompt, get_runtime_tool_registry};
use super::super::*;

impl IpcServer {
    pub(super) async fn handle_get_available_tools(
        core: &Arc<AppCore>,
        runtime_tool_registry: &OnceLock<restflow_ai::tools::ToolRegistry>,
    ) -> IpcResponse {
        match get_runtime_tool_registry(core, runtime_tool_registry) {
            Ok(registry) => {
                let tools: Vec<String> = registry
                    .list()
                    .iter()
                    .map(|name| name.to_string())
                    .collect();
                IpcResponse::success(tools)
            }
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_get_available_tool_definitions(
        core: &Arc<AppCore>,
        runtime_tool_registry: &OnceLock<restflow_ai::tools::ToolRegistry>,
    ) -> IpcResponse {
        match get_runtime_tool_registry(core, runtime_tool_registry) {
            Ok(registry) => {
                let tools: Vec<ToolDefinition> = registry
                    .schemas()
                    .into_iter()
                    .map(|schema| ToolDefinition {
                        name: schema.name,
                        description: schema.description,
                        parameters: schema.parameters,
                    })
                    .collect();
                IpcResponse::success(tools)
            }
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }

    pub(super) async fn handle_execute_tool(
        core: &Arc<AppCore>,
        runtime_tool_registry: &OnceLock<restflow_ai::tools::ToolRegistry>,
        name: String,
        input: serde_json::Value,
    ) -> IpcResponse {
        match get_runtime_tool_registry(core, runtime_tool_registry) {
            Ok(registry) => match registry.execute_safe(&name, input).await {
                Ok(output) => IpcResponse::success(ToolExecutionResult {
                    success: output.success,
                    result: output.result,
                    error: output.error,
                    error_category: output.error_category,
                    retryable: output.retryable,
                    retry_after_ms: output.retry_after_ms,
                }),
                Err(err) => ipc_error_with_optional_json_details(500, err.to_string()),
            },
            Err(err) => ipc_error_with_optional_json_details(500, err.to_string()),
        }
    }

    pub(super) async fn handle_build_agent_system_prompt(
        core: &Arc<AppCore>,
        agent_node: crate::models::AgentNode,
    ) -> IpcResponse {
        match build_agent_system_prompt(core, agent_node) {
            Ok(prompt) => IpcResponse::success(serde_json::json!({ "prompt": prompt })),
            Err(err) => IpcResponse::error(500, err.to_string()),
        }
    }
}
