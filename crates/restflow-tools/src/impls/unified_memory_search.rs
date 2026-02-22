//! Unified memory search tool for searching long-term memory and chat session history.

use async_trait::async_trait;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::{Result, ToolError};
use crate::{Tool, ToolOutput};
use restflow_traits::store::UnifiedMemorySearch;

pub struct UnifiedMemorySearchTool {
    search: Arc<dyn UnifiedMemorySearch>,
}

impl UnifiedMemorySearchTool {
    pub fn new(search: Arc<dyn UnifiedMemorySearch>) -> Self {
        Self { search }
    }
}

#[async_trait]
impl Tool for UnifiedMemorySearchTool {
    fn name(&self) -> &str {
        "memory_search"
    }

    fn description(&self) -> &str {
        "Search through long-term memory and chat session history"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Keywords or phrase to search for"
                },
                "agent_id": {
                    "type": "string",
                    "description": "Agent ID to search within"
                },
                "include_sessions": {
                    "type": "boolean",
                    "description": "Whether to search chat sessions",
                    "default": true
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results to return",
                    "default": 5,
                    "minimum": 1
                },
                "offset": {
                    "type": "integer",
                    "description": "Offset for pagination",
                    "default": 0,
                    "minimum": 0
                }
            },
            "required": ["query", "agent_id"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let query = input
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::Tool("Missing query parameter".to_string()))?;
        let agent_id = input
            .get("agent_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::Tool("Missing agent_id parameter".to_string()))?;
        let include_sessions = input
            .get("include_sessions")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let limit = input
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(5)
            .min(u32::MAX as u64) as u32;
        let offset = input
            .get("offset")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            .min(u32::MAX as u64) as u32;

        let results =
            self.search
                .search(agent_id, query, include_sessions, limit, offset)?;
        Ok(ToolOutput::success(results))
    }
}
