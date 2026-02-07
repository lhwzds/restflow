//! Memory management tool for long-term memory.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use super::traits::{Tool, ToolOutput};
use crate::error::{AiError, Result};

#[derive(Clone, Debug, Deserialize)]
pub struct MemoryExportRequest {
    pub agent_id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub options: Option<Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MemoryClearRequest {
    pub agent_id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub delete_sessions: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MemoryCompactRequest {
    pub agent_id: String,
    #[serde(default)]
    pub keep_recent: Option<u32>,
    #[serde(default)]
    pub before_ms: Option<i64>,
}

pub trait MemoryManager: Send + Sync {
    fn stats(&self, agent_id: &str) -> Result<Value>;
    fn export(&self, request: MemoryExportRequest) -> Result<Value>;
    fn clear(&self, request: MemoryClearRequest) -> Result<Value>;
    fn compact(&self, request: MemoryCompactRequest) -> Result<Value>;
}

#[derive(Clone)]
pub struct MemoryManagementTool {
    manager: Arc<dyn MemoryManager>,
    allow_write: bool,
}

impl MemoryManagementTool {
    pub fn new(manager: Arc<dyn MemoryManager>) -> Self {
        Self {
            manager,
            allow_write: false,
        }
    }

    pub fn with_write(mut self, allow_write: bool) -> Self {
        self.allow_write = allow_write;
        self
    }

    fn write_guard(&self) -> Result<()> {
        if self.allow_write {
            Ok(())
        } else {
            Err(AiError::Tool(
                "Write access to memory is disabled for this tool".to_string(),
            ))
        }
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
    Clear {
        agent_id: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        delete_sessions: Option<bool>,
    },
    Compact {
        agent_id: String,
        #[serde(default)]
        keep_recent: Option<u32>,
        #[serde(default)]
        before_ms: Option<i64>,
    },
}

#[async_trait]
impl Tool for MemoryManagementTool {
    fn name(&self) -> &str {
        "manage_memory"
    }

    fn description(&self) -> &str {
        "Manage long-term memory. Supports stats, export, clear, and compact."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["stats", "export", "clear", "compact"],
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
                "delete_sessions": {
                    "type": "boolean",
                    "description": "Whether to delete memory sessions when clearing",
                    "default": true
                },
                "keep_recent": {
                    "type": "integer",
                    "description": "Number of most recent chunks to keep when compacting",
                    "minimum": 0
                },
                "before_ms": {
                    "type": "integer",
                    "description": "Delete chunks older than this timestamp (ms since epoch)"
                }
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
            MemoryAction::Clear {
                agent_id,
                session_id,
                delete_sessions,
            } => {
                self.write_guard()?;
                let request = MemoryClearRequest {
                    agent_id,
                    session_id,
                    delete_sessions,
                };
                ToolOutput::success(self.manager.clear(request)?)
            }
            MemoryAction::Compact {
                agent_id,
                keep_recent,
                before_ms,
            } => {
                self.write_guard()?;
                let request = MemoryCompactRequest {
                    agent_id,
                    keep_recent,
                    before_ms,
                };
                ToolOutput::success(self.manager.compact(request)?)
            }
        };

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    async fn test_clear_requires_write() {
        let tool = MemoryManagementTool::new(Arc::new(MockManager));
        let result = tool
            .execute(json!({"operation": "clear", "agent_id": "agent"}))
            .await;
        assert!(result.is_err());
    }
}
