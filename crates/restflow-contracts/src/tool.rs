use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ToolErrorCategory {
    Network,
    Auth,
    Config,
    Execution,
    RateLimit,
    NotFound,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolExecutionResult {
    pub success: bool,
    pub result: Value,
    pub error: Option<String>,
    #[serde(default)]
    pub error_category: Option<ToolErrorCategory>,
    #[serde(default)]
    pub retryable: Option<bool>,
    #[serde(default)]
    pub retry_after_ms: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_definition_round_trips() {
        let definition = ToolDefinition {
            name: "search".to_string(),
            description: "Search documents".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                }
            }),
        };

        let json = serde_json::to_string(&definition).unwrap();
        let decoded: ToolDefinition = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, definition);
    }

    #[test]
    fn tool_execution_result_round_trips() {
        let result = ToolExecutionResult {
            success: false,
            result: serde_json::json!({ "partial": "output" }),
            error: Some("Timed out".to_string()),
            error_category: Some(ToolErrorCategory::RateLimit),
            retryable: Some(true),
            retry_after_ms: Some(1_000),
        };

        let json = serde_json::to_string(&result).unwrap();
        let decoded: ToolExecutionResult = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, result);
    }
}
