use restflow_contracts::ToolExecutionResult;
use restflow_traits::ToolOutput;

pub fn to_tool_execution_result(output: ToolOutput) -> ToolExecutionResult {
    ToolExecutionResult {
        success: output.success,
        result: output.result,
        error: output.error,
        error_category: output.error_category,
        retryable: output.retryable,
        retry_after_ms: output.retry_after_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_traits::ToolErrorCategory;
    use serde_json::json;

    #[test]
    fn maps_tool_output_to_contract_result() {
        let output = ToolOutput {
            success: false,
            result: json!({"details":"x"}),
            error: Some("boom".to_string()),
            error_category: Some(ToolErrorCategory::Execution),
            retryable: Some(false),
            retry_after_ms: Some(100),
        };

        let mapped = to_tool_execution_result(output);
        assert!(!mapped.success);
        assert_eq!(mapped.result["details"], "x");
        assert_eq!(mapped.error.as_deref(), Some("boom"));
        assert_eq!(mapped.error_category, Some(ToolErrorCategory::Execution));
        assert_eq!(mapped.retryable, Some(false));
        assert_eq!(mapped.retry_after_ms, Some(100));
    }
}
