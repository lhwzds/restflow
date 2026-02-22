//! Shared space storage tool for reading and writing entries in shared space.

use async_trait::async_trait;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::{Result, ToolError};
use crate::{Tool, ToolOutput};
use restflow_traits::store::SharedSpaceStore;

pub struct SharedSpaceTool {
    store: Arc<dyn SharedSpaceStore>,
    accessor_id: Option<String>,
}

impl SharedSpaceTool {
    pub fn new(store: Arc<dyn SharedSpaceStore>, accessor_id: Option<String>) -> Self {
        Self { store, accessor_id }
    }
}

#[async_trait]
impl Tool for SharedSpaceTool {
    fn name(&self) -> &str {
        "shared_space"
    }

    fn description(&self) -> &str {
        "Read and write entries in the shared space storage. Use namespace:name keys."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["get", "set", "delete", "list"],
                    "description": "The action to perform"
                },
                "key": {
                    "type": "string",
                    "description": "The key (namespace:name). Required for get/set/delete."
                },
                "value": {
                    "type": "string",
                    "description": "The value to store. Required for set."
                },
                "visibility": {
                    "type": "string",
                    "enum": ["public", "shared", "private"],
                    "description": "Access level for the entry"
                },
                "content_type": {
                    "type": "string",
                    "description": "Optional content type hint"
                },
                "type_hint": {
                    "type": "string",
                    "description": "Optional type hint for categorization"
                },
                "tags": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    },
                    "description": "Optional tags for filtering"
                },
                "namespace": {
                    "type": "string",
                    "description": "For list: filter by namespace prefix"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let action = input
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::Tool("Missing action parameter".to_string()))?;

        match action {
            "get" => {
                let key = input
                    .get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::Tool("Missing key parameter".to_string()))?;
                let result = self.store.get_entry(key)?;
                Ok(ToolOutput::success(result))
            }
            "set" => {
                let key = input
                    .get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::Tool("Missing key parameter".to_string()))?;
                let value = input
                    .get("value")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::Tool("Missing value parameter".to_string()))?;
                let visibility = input.get("visibility").and_then(|v| v.as_str());
                let content_type = input.get("content_type").and_then(|v| v.as_str());
                let type_hint = input.get("type_hint").and_then(|v| v.as_str());
                let tags = input.get("tags").and_then(|v| v.as_array()).map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(|s| s.to_string()))
                        .collect::<Vec<String>>()
                });
                let result = self.store.set_entry(
                    key,
                    value,
                    visibility,
                    content_type,
                    type_hint,
                    tags,
                    self.accessor_id.as_deref(),
                )?;
                Ok(ToolOutput::success(result))
            }
            "delete" => {
                let key = input
                    .get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::Tool("Missing key parameter".to_string()))?;
                let result = self
                    .store
                    .delete_entry(key, self.accessor_id.as_deref())?;
                Ok(ToolOutput::success(result))
            }
            "list" => {
                let namespace = input.get("namespace").and_then(|v| v.as_str());
                let result = self.store.list_entries(namespace)?;
                Ok(ToolOutput::success(result))
            }
            _ => Ok(ToolOutput::error(format!("Unknown action: {}", action))),
        }
    }
}
