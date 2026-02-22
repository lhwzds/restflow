//! Tool for persisting typed deliverables produced by agents.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::error::Result;
use crate::tool::{Tool, ToolOutput};
use restflow_ai::tools::store_traits::DeliverableStore;

#[derive(Clone)]
pub struct SaveDeliverableTool {
    store: std::sync::Arc<dyn DeliverableStore>,
}

impl SaveDeliverableTool {
    pub fn new(store: std::sync::Arc<dyn DeliverableStore>) -> Self {
        Self { store }
    }
}

#[derive(Debug, Deserialize)]
struct SaveDeliverableInput {
    #[serde(default)]
    task_id: Option<String>,
    #[serde(default)]
    execution_id: Option<String>,
    #[serde(rename = "type")]
    deliverable_type: String,
    title: String,
    content: String,
    #[serde(default)]
    file_path: Option<String>,
    #[serde(default)]
    content_type: Option<String>,
    #[serde(default)]
    metadata: Option<Value>,
}

#[async_trait]
impl Tool for SaveDeliverableTool {
    fn name(&self) -> &str {
        "save_deliverable"
    }

    fn description(&self) -> &str {
        "Save a typed output from your execution. Types: report (markdown), data (JSON/text), file (path reference), artifact (code/config)."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "Optional background task ID. If omitted, a fallback ID is used."
                },
                "execution_id": {
                    "type": "string",
                    "description": "Optional execution ID. If omitted, a generated ID is used."
                },
                "type": {
                    "type": "string",
                    "enum": ["report", "data", "file", "artifact"],
                    "description": "Deliverable type"
                },
                "title": {
                    "type": "string",
                    "description": "Human-readable title"
                },
                "content": {
                    "type": "string",
                    "description": "Deliverable content"
                },
                "file_path": {
                    "type": "string",
                    "description": "Optional file path for file-type deliverables"
                },
                "content_type": {
                    "type": "string",
                    "description": "Optional MIME type hint"
                },
                "metadata": {
                    "type": "object",
                    "description": "Optional metadata payload"
                }
            },
            "required": ["type", "title", "content"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: SaveDeliverableInput = match serde_json::from_value(input) {
            Ok(p) => p,
            Err(e) => return Ok(ToolOutput::error(format!("Invalid input: {}", e))),
        };

        let task_id = params
            .task_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("standalone-task");

        let generated_execution_id;
        let execution_id = if let Some(value) = params
            .execution_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            value
        } else {
            generated_execution_id = Uuid::new_v4().to_string();
            &generated_execution_id
        };

        let deliverable_type = params.deliverable_type.trim().to_lowercase();
        if !matches!(
            deliverable_type.as_str(),
            "report" | "data" | "file" | "artifact"
        ) {
            return Ok(ToolOutput::error(format!(
                "Invalid type '{}'. Supported: report, data, file, artifact",
                params.deliverable_type
            )));
        }

        let result = self.store.save_deliverable(
            task_id,
            execution_id,
            &deliverable_type,
            &params.title,
            &params.content,
            params.file_path.as_deref(),
            params.content_type.as_deref(),
            params.metadata,
        )?;

        Ok(ToolOutput::success(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_ai::error::AiError;

    struct MockDeliverableStore;

    impl DeliverableStore for MockDeliverableStore {
        fn save_deliverable(
            &self,
            task_id: &str,
            execution_id: &str,
            deliverable_type: &str,
            title: &str,
            content: &str,
            _file_path: Option<&str>,
            _content_type: Option<&str>,
            _metadata: Option<Value>,
        ) -> Result<Value> {
            Ok(json!({
                "id": "d1",
                "task_id": task_id,
                "execution_id": execution_id,
                "type": deliverable_type,
                "title": title,
                "content": content,
            }))
        }
    }

    struct FailingDeliverableStore;

    impl DeliverableStore for FailingDeliverableStore {
        fn save_deliverable(
            &self,
            _task_id: &str,
            _execution_id: &str,
            _deliverable_type: &str,
            _title: &str,
            _content: &str,
            _file_path: Option<&str>,
            _content_type: Option<&str>,
            _metadata: Option<Value>,
        ) -> Result<Value> {
            Err(crate::error::ToolError::Tool("store down".to_string()))
        }
    }

    #[tokio::test]
    async fn test_save_deliverable_success() {
        let tool = SaveDeliverableTool::new(std::sync::Arc::new(MockDeliverableStore));
        let out = tool
            .execute(json!({
                "task_id": "task-1",
                "execution_id": "exec-1",
                "type": "report",
                "title": "Summary",
                "content": "# Done"
            }))
            .await
            .expect("tool call should succeed");
        assert!(out.success);
        assert_eq!(out.result["task_id"], "task-1");
    }

    #[tokio::test]
    async fn test_save_deliverable_invalid_type() {
        let tool = SaveDeliverableTool::new(std::sync::Arc::new(MockDeliverableStore));
        let out = tool
            .execute(json!({
                "type": "unknown",
                "title": "Summary",
                "content": "# Done"
            }))
            .await
            .expect("tool call should succeed");
        assert!(!out.success);
        assert!(out.error.expect("error expected").contains("Invalid type"));
    }

    #[tokio::test]
    async fn test_save_deliverable_store_failure() {
        let tool = SaveDeliverableTool::new(std::sync::Arc::new(FailingDeliverableStore));
        let err = tool
            .execute(json!({
                "type": "report",
                "title": "Summary",
                "content": "# Done"
            }))
            .await
            .expect_err("store failure should bubble up");
        assert!(err.to_string().contains("store down"));
    }
}
