//! Tools API - Expose available AI agent tools

use crate::api::ApiResponse;
use axum::Json;
use restflow_ai::tools::{default_registry, ToolSchema};
use serde::{Deserialize, Serialize};

/// Tool information exposed to frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

impl From<ToolSchema> for ToolInfo {
    fn from(schema: ToolSchema) -> Self {
        Self {
            name: schema.name,
            description: schema.description,
            parameters: schema.parameters,
        }
    }
}

/// GET /api/tools - List all available tools
pub async fn list_tools() -> Json<ApiResponse<Vec<ToolInfo>>> {
    let registry = default_registry();
    let tools: Vec<ToolInfo> = registry.schemas().into_iter().map(Into::into).collect();
    Json(ApiResponse::ok(tools))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_list_tools() {
        let response = list_tools().await;
        let body = response.0;

        assert!(body.success);
        let tools = body.data.unwrap();

        // Should have at least the 3 default tools
        assert!(tools.len() >= 3);

        // Check tool names
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"http_request"));
        assert!(names.contains(&"run_python"));
        assert!(names.contains(&"send_email"));
    }
}
