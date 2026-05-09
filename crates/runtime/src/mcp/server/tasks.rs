use super::*;
use ::types::store::MANAGE_TASK_OPERATIONS_CSV;

impl RestFlowMcpServer {
    pub(crate) async fn handle_manage_tasks(
        &self,
        params: ManageTasksParams,
    ) -> Result<String, String> {
        let operation = params.operation.trim().to_lowercase();
        let params = self
            .apply_task_api_defaults(operation.as_str(), params)
            .await?;

        let value = match operation.as_str() {
            "list"
            | "create"
            | "convert_session"
            | "promote_to_background"
            | "update"
            | "delete"
            | "start"
            | "pause"
            | "resume"
            | "run"
            | "stop"
            | "control"
            | "progress"
            | "send_message"
            | "list_messages"
            | "list_artifacts"
            | "run_batch" => self.execute_task_runtime_tool(&params).await?,
            _ => {
                return Err(format!(
                    "Unknown operation: {}. Supported: {}",
                    operation, MANAGE_TASK_OPERATIONS_CSV
                ));
            }
        };

        serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
    }

    pub(crate) async fn apply_task_api_defaults(
        &self,
        operation: &str,
        mut params: ManageTasksParams,
    ) -> Result<ManageTasksParams, String> {
        if matches!(operation, "progress" | "list_messages") {
            let defaults = self.load_api_defaults().await?;
            if operation == "progress" && params.event_limit.is_none() {
                params.event_limit = Some(defaults.task_progress_event_limit);
            }
            if operation == "list_messages" && params.limit.is_none() {
                params.limit = Some(defaults.task_message_list_limit);
            }
        }
        Ok(params)
    }

    pub(crate) async fn execute_task_runtime_tool(
        &self,
        params: &ManageTasksParams,
    ) -> Result<Value, String> {
        let mut tool_input = serde_json::to_value(params)
            .map_err(|e| format!("Failed to serialize params: {}", e))?;
        strip_null_fields(&mut tool_input);
        let tool_result = self
            .backend
            .execute_runtime_tool("manage_tasks", tool_input)
            .await
            .map_err(|e| Self::wrap_backend_error("Failed to execute runtime tool", e))?;
        if !tool_result.success {
            if !tool_result.result.is_null() {
                return Ok(tool_result.result);
            }
            return Err(tool_result
                .error
                .unwrap_or_else(|| "manage_tasks tool failed".to_string()));
        }
        Ok(tool_result.result)
    }
}

fn strip_null_fields(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.retain(|_, child| !child.is_null());
            for child in map.values_mut() {
                strip_null_fields(child);
            }
        }
        Value::Array(items) => {
            for item in items {
                strip_null_fields(item);
            }
        }
        _ => {}
    }
}
