//! Work items tool for listing, claiming, and managing work items.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::Result;
use crate::{Tool, ToolOutput};
use restflow_traits::store::{WorkItemProvider, WorkItemStatus, WorkItemSpec, WorkItemPatch, WorkItemQuery};

#[derive(Debug, Deserialize)]
struct WorkItemInput {
    operation: String,
    id: Option<String>,
    folder: Option<String>,
    title: Option<String>,
    content: Option<String>,
    priority: Option<String>,
    status: Option<WorkItemStatus>,
    tags: Option<Vec<String>>,
    assignee: Option<String>,
    search: Option<String>,
    tag: Option<String>,
}

pub struct WorkItemTool {
    provider: Arc<dyn WorkItemProvider>,
    allow_write: bool,
}

impl WorkItemTool {
    pub fn new(provider: Arc<dyn WorkItemProvider>) -> Self {
        Self {
            provider,
            allow_write: false,
        }
    }

    pub fn with_write(mut self, allow_write: bool) -> Self {
        self.allow_write = allow_write;
        self
    }

    fn write_guard(&self) -> std::result::Result<(), String> {
        if self.allow_write {
            Ok(())
        } else {
            Err("Write access to work items is disabled for this tool".to_string())
        }
    }
}

#[async_trait]
impl Tool for WorkItemTool {
    fn name(&self) -> &str {
        "work_items"
    }

    fn description(&self) -> &str {
        "Manage work items. Operations: create (new item), update (modify existing item), delete (remove item), list (browse items), get (read an item), and claim (assign to yourself)."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["list", "list_folders", "get", "create", "update", "delete", "claim"]
                },
                "id": { "type": "string" },
                "folder": { "type": "string" },
                "title": { "type": "string" },
                "content": { "type": "string" },
                "priority": {
                    "type": "string",
                    "enum": ["p0", "p1", "p2", "p3"]
                },
                "status": {
                    "type": "string",
                    "enum": ["open", "in_progress", "done", "archived"]
                },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "assignee": { "type": "string" },
                "tag": { "type": "string" },
                "search": { "type": "string" }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: WorkItemInput = match serde_json::from_value(input) {
            Ok(value) => value,
            Err(err) => return Ok(ToolOutput::error(format!("Invalid input: {}", err))),
        };

        match params.operation.as_str() {
            "list" => {
                let query = WorkItemQuery {
                    folder: params.folder,
                    status: params.status,
                    priority: params.priority,
                    tag: params.tag,
                    assignee: params.assignee,
                    search: params.search,
                };
                match self.provider.list(query) {
                    Ok(items) => Ok(ToolOutput::success(json!({ "items": items }))),
                    Err(err) => Ok(ToolOutput::error(err)),
                }
            }
            "list_folders" => match self.provider.list_folders() {
                Ok(folders) => Ok(ToolOutput::success(json!({ "folders": folders }))),
                Err(err) => Ok(ToolOutput::error(err)),
            },
            "get" => {
                let Some(id) = params.id else {
                    return Ok(ToolOutput::error("Missing id for get operation"));
                };
                match self.provider.get(&id) {
                    Ok(Some(item)) => Ok(ToolOutput::success(json!(item))),
                    Ok(None) => Ok(ToolOutput::error(format!(
                        "Work item '{}' not found",
                        id
                    ))),
                    Err(err) => Ok(ToolOutput::error(err)),
                }
            }
            "create" => {
                if let Err(err) = self.write_guard() {
                    return Ok(ToolOutput::error(err));
                }

                let spec = WorkItemSpec {
                    folder: match params.folder {
                        Some(value) => value,
                        None => {
                            return Ok(ToolOutput::error("Missing folder for create operation"));
                        }
                    },
                    title: match params.title {
                        Some(value) => value,
                        None => return Ok(ToolOutput::error("Missing title for create operation")),
                    },
                    content: params.content.unwrap_or_default(),
                    priority: params.priority,
                    tags: params.tags.unwrap_or_default(),
                };

                match self.provider.create(spec) {
                    Ok(item) => Ok(ToolOutput::success(json!(item))),
                    Err(err) => Ok(ToolOutput::error(err)),
                }
            }
            "update" => {
                if let Err(err) = self.write_guard() {
                    return Ok(ToolOutput::error(err));
                }

                let Some(id) = params.id else {
                    return Ok(ToolOutput::error("Missing id for update operation"));
                };

                let patch = WorkItemPatch {
                    title: params.title,
                    content: params.content,
                    priority: params.priority,
                    status: params.status,
                    tags: params.tags,
                    assignee: params.assignee,
                    folder: params.folder,
                };

                match self.provider.update(&id, patch) {
                    Ok(item) => Ok(ToolOutput::success(json!(item))),
                    Err(err) => Ok(ToolOutput::error(err)),
                }
            }
            "delete" => {
                if let Err(err) = self.write_guard() {
                    return Ok(ToolOutput::error(err));
                }

                let Some(id) = params.id else {
                    return Ok(ToolOutput::error("Missing id for delete operation"));
                };

                match self.provider.delete(&id) {
                    Ok(deleted) => Ok(ToolOutput::success(json!({ "id": id, "deleted": deleted }))),
                    Err(err) => Ok(ToolOutput::error(err)),
                }
            }
            "claim" => {
                if let Err(err) = self.write_guard() {
                    return Ok(ToolOutput::error(err));
                }

                let Some(id) = params.id else {
                    return Ok(ToolOutput::error("Missing id for claim operation"));
                };

                let patch = WorkItemPatch {
                    status: Some(WorkItemStatus::InProgress),
                    assignee: params.assignee,
                    ..WorkItemPatch::default()
                };

                match self.provider.update(&id, patch) {
                    Ok(item) => Ok(ToolOutput::success(json!(item))),
                    Err(err) => Ok(ToolOutput::error(err)),
                }
            }
            other => Ok(ToolOutput::error(format!(
                "Unsupported work item operation: {}",
                other
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_traits::WorkItemRecord;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MockWorkItemProvider {
        items: Mutex<HashMap<String, WorkItemRecord>>,
    }

    impl MockWorkItemProvider {
        fn seed() -> Arc<Self> {
            let provider = Arc::new(Self::default());
            provider.items.lock().unwrap().insert(
                "item-1".to_string(),
                WorkItemRecord {
                    id: "item-1".to_string(),
                    folder: "feature".to_string(),
                    title: "Plan".to_string(),
                    content: "Todo".to_string(),
                    priority: Some("p1".to_string()),
                    status: WorkItemStatus::Open,
                    tags: vec!["agent".to_string()],
                    assignee: None,
                    created_at: 1,
                    updated_at: 1,
                },
            );
            provider
        }
    }

    impl WorkItemProvider for MockWorkItemProvider {
        fn create(
            &self,
            spec: WorkItemSpec,
        ) -> std::result::Result<WorkItemRecord, String> {
            let mut items = self.items.lock().map_err(|_| "Lock poisoned".to_string())?;
            let id = format!("item-{}", items.len() + 1);
            let item = WorkItemRecord {
                id: id.clone(),
                folder: spec.folder,
                title: spec.title,
                content: spec.content,
                priority: spec.priority,
                status: WorkItemStatus::Open,
                tags: spec.tags,
                assignee: None,
                created_at: 1,
                updated_at: 1,
            };
            items.insert(id, item.clone());
            Ok(item)
        }

        fn get(&self, id: &str) -> std::result::Result<Option<WorkItemRecord>, String> {
            let items = self.items.lock().map_err(|_| "Lock poisoned".to_string())?;
            Ok(items.get(id).cloned())
        }

        fn update(
            &self,
            id: &str,
            patch: WorkItemPatch,
        ) -> std::result::Result<WorkItemRecord, String> {
            let mut items = self.items.lock().map_err(|_| "Lock poisoned".to_string())?;
            let item = items.get_mut(id).ok_or_else(|| "Not found".to_string())?;

            if let Some(status) = patch.status {
                item.status = status;
            }
            if let Some(assignee) = patch.assignee {
                item.assignee = Some(assignee);
            }
            if let Some(title) = patch.title {
                item.title = title;
            }

            Ok(item.clone())
        }

        fn delete(&self, id: &str) -> std::result::Result<bool, String> {
            let mut items = self.items.lock().map_err(|_| "Lock poisoned".to_string())?;
            Ok(items.remove(id).is_some())
        }

        fn list(
            &self,
            _query: WorkItemQuery,
        ) -> std::result::Result<Vec<WorkItemRecord>, String> {
            let items = self.items.lock().map_err(|_| "Lock poisoned".to_string())?;
            Ok(items.values().cloned().collect())
        }

        fn list_folders(&self) -> std::result::Result<Vec<String>, String> {
            Ok(vec!["feature".to_string(), "issue".to_string()])
        }
    }

    #[tokio::test]
    async fn list_operation_returns_items() {
        let provider = MockWorkItemProvider::seed();
        let tool = WorkItemTool::new(provider).with_write(true);

        let out = tool.execute(json!({ "operation": "list" })).await.unwrap();
        assert!(out.success);
        assert!(!out.result["items"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn claim_operation_updates_status() {
        let provider = MockWorkItemProvider::seed();
        let tool = WorkItemTool::new(provider).with_write(true);

        let out = tool
            .execute(json!({
                "operation": "claim",
                "id": "item-1",
                "assignee": "agent-b"
            }))
            .await
            .unwrap();

        assert!(out.success);
        assert_eq!(out.result["status"], json!("in_progress"));
        assert_eq!(out.result["assignee"], json!("agent-b"));
    }

    #[tokio::test]
    async fn write_operations_are_blocked_when_disabled() {
        let provider = MockWorkItemProvider::seed();
        let tool = WorkItemTool::new(provider);

        let out = tool
            .execute(json!({
                "operation": "create",
                "folder": "feature",
                "title": "x",
                "content": "y"
            }))
            .await
            .unwrap();

        assert!(!out.success);
        assert!(out.error.unwrap().contains("disabled"));
    }
}
