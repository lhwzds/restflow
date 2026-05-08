use super::*;

impl RestFlowMcpServer {
    pub(crate) async fn handle_runtime_tool(
        &self,
        name: &str,
        input: Value,
    ) -> Result<String, String> {
        let output = self.backend.execute_runtime_tool(name, input).await?;
        if output.success {
            serde_json::to_string_pretty(&output.result).map_err(|e| e.to_string())
        } else {
            let message = output
                .error
                .unwrap_or_else(|| format!("Tool '{}' execution failed", name));
            let payload = serde_json::json!({
                "tool": name,
                "error": message,
                "error_category": output.error_category,
                "retryable": output.retryable,
                "retry_after_ms": output.retry_after_ms,
                "details": output.result,
            });
            Err(serde_json::to_string_pretty(&payload)
                .unwrap_or_else(|_| format!("Tool '{}' execution failed", name)))
        }
    }

    pub(crate) async fn handle_switch_model_for_mcp(&self, input: Value) -> Result<String, String> {
        let output = match RuntimeTool::execute(&self.switch_model_tool, input).await {
            Ok(output) => output,
            Err(error) => {
                let payload = serde_json::json!({
                    "tool": "switch_model",
                    "error": error.to_string(),
                    "error_category": serde_json::Value::Null,
                    "retryable": serde_json::Value::Null,
                    "retry_after_ms": serde_json::Value::Null,
                    "details": serde_json::Value::Null,
                });
                return Err(serde_json::to_string_pretty(&payload)
                    .unwrap_or_else(|_| "switch_model execution failed".to_string()));
            }
        };
        if output.success {
            serde_json::to_string_pretty(&output.result).map_err(|e| e.to_string())
        } else {
            let payload = serde_json::json!({
                "tool": "switch_model",
                "error": output
                    .error
                    .unwrap_or_else(|| "switch_model execution failed".to_string()),
                "error_category": output.error_category,
                "retryable": output.retryable,
                "retry_after_ms": output.retry_after_ms,
                "details": output.result,
            });
            Err(serde_json::to_string_pretty(&payload)
                .unwrap_or_else(|_| "switch_model execution failed".to_string()))
        }
    }
}
