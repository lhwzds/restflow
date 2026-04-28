//! Memory management tool for long-term memory.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::Result;
use crate::{Tool, ToolOutput};
use restflow_traits::store::{MemoryExportRequest, MemoryManager};

#[derive(Clone)]
pub struct MemoryManagementTool {
    manager: Arc<dyn MemoryManager>,
}

impl MemoryManagementTool {
    pub fn new(manager: Arc<dyn MemoryManager>) -> Self {
        Self { manager }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum MemoryAction {
    Stats {
        agent_id: String,
    },
    Export {
        agent_id: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        options: Option<Value>,
    },
}

#[async_trait]
impl Tool for MemoryManagementTool {
    fn name(&self) -> &str {
        "manage_memory"
    }

    fn description(&self) -> &str {
        "Inspect long-term memory storage with stats and export operations."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["stats", "export"],
                    "description": "Memory operation to perform"
                },
                "agent_id": {
                    "type": "string",
                    "description": "Agent ID for memory operations"
                },
                "session_id": {
                    "type": "string",
                    "description": "Optional session ID (for export/clear)"
                },
                "options": {
                    "type": "object",
                    "description": "Export options override (for export)"
                },
            },
            "required": ["operation", "agent_id"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let action: MemoryAction = serde_json::from_value(input)?;

        let output = match action {
            MemoryAction::Stats { agent_id } => ToolOutput::success(self.manager.stats(&agent_id)?),
            MemoryAction::Export {
                agent_id,
                session_id,
                options,
            } => {
                let request = MemoryExportRequest {
                    agent_id,
                    session_id,
                    options,
                };
                ToolOutput::success(self.manager.export(request)?)
            }
        };

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_traits::store::{MemoryClearRequest, MemoryCompactRequest};

    struct MockManager;

    impl MemoryManager for MockManager {
        fn stats(&self, _agent_id: &str) -> Result<Value> {
            Ok(json!({"chunk_count": 1}))
        }

        fn export(&self, _request: MemoryExportRequest) -> Result<Value> {
            Ok(json!({"markdown": "# Export"}))
        }

        fn clear(&self, _request: MemoryClearRequest) -> Result<Value> {
            Ok(json!({"deleted": 1}))
        }

        fn compact(&self, _request: MemoryCompactRequest) -> Result<Value> {
            Ok(json!({"deleted": 1}))
        }
    }

    #[tokio::test]
    async fn test_stats() {
        let tool = MemoryManagementTool::new(Arc::new(MockManager));
        let output = tool
            .execute(json!({"operation": "stats", "agent_id": "agent"}))
            .await
            .unwrap();
        assert!(output.success);
    }

    #[tokio::test]
    async fn test_clear_is_not_supported() {
        let tool = MemoryManagementTool::new(Arc::new(MockManager));
        let result = tool
            .execute(json!({"operation": "clear", "agent_id": "agent"}))
            .await;
        let err = result.expect_err("expected unsupported operation error");
        assert!(err.to_string().contains("unknown variant"));
    }
}
