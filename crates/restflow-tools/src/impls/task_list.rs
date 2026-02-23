//! Task list tool backed by WorkspaceNoteProvider.
//!
//! Provides structured task tracking with:
//! - Status lifecycle: pending -> in_progress -> completed
//! - Task dependencies (blocks/blocked_by)
//! - Priority, owner, and tag support
//! - Dependency-aware listing (shows which tasks are blocked)
//!
//! Internally uses WorkspaceNoteProvider with a fixed `__tasks__` folder,
//! storing dependency metadata in the note content as JSON.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::Result;
use crate::{Tool, ToolOutput};
use restflow_traits::store::{
    WorkspaceNoteProvider, WorkspaceNotePatch, WorkspaceNoteQuery, WorkspaceNoteSpec,
    WorkspaceNoteStatus,
};

/// Fixed folder name for task notes
const TASK_FOLDER: &str = "__tasks__";

#[derive(Debug, Deserialize)]
struct TaskListInput {
    operation: String,
    #[serde(rename = "taskId")]
    task_id: Option<String>,
    subject: Option<String>,
    description: Option<String>,
    #[serde(rename = "activeForm")]
    active_form: Option<String>,
    priority: Option<String>,
    status: Option<String>,
    owner: Option<String>,
    tags: Option<Vec<String>>,
    metadata: Option<Value>,
    #[serde(rename = "addBlocks")]
    add_blocks: Option<Vec<String>>,
    #[serde(rename = "addBlockedBy")]
    add_blocked_by: Option<Vec<String>>,
}

/// Metadata stored in note content as JSON
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct TaskMeta {
    #[serde(default)]
    description: String,
    #[serde(default)]
    blocks: Vec<String>,
    #[serde(default)]
    blocked_by: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    active_form: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    metadata: Option<Value>,
}

/// Task list tool backed by WorkspaceNoteProvider.
pub struct TaskListTool {
    provider: Arc<dyn WorkspaceNoteProvider>,
}

impl TaskListTool {
    pub fn new(provider: Arc<dyn WorkspaceNoteProvider>) -> Self {
        Self { provider }
    }

    fn parse_meta(content: &str) -> TaskMeta {
        serde_json::from_str(content).unwrap_or_else(|_| TaskMeta {
            description: content.to_string(),
            ..Default::default()
        })
    }

    fn encode_meta(meta: &TaskMeta) -> String {
        serde_json::to_string(meta).unwrap_or_default()
    }

    /// Map WorkspaceNoteStatus to task status string
    fn status_to_str(status: &WorkspaceNoteStatus) -> &'static str {
        match status {
            WorkspaceNoteStatus::Open => "pending",
            WorkspaceNoteStatus::InProgress => "in_progress",
            WorkspaceNoteStatus::Done => "completed",
            WorkspaceNoteStatus::Archived => "completed",
        }
    }

    /// Map task status string to WorkspaceNoteStatus
    fn str_to_status(s: &str) -> Option<WorkspaceNoteStatus> {
        match s {
            "pending" => Some(WorkspaceNoteStatus::Open),
            "in_progress" => Some(WorkspaceNoteStatus::InProgress),
            "completed" => Some(WorkspaceNoteStatus::Done),
            _ => None,
        }
    }
}

#[async_trait]
impl Tool for TaskListTool {
    fn name(&self) -> &str {
        "task_list"
    }

    fn description(&self) -> &str {
        "Manage structured tasks with dependencies. Operations: create, list, get, update, delete."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "description": "Operation to perform: create, list, get, update, delete",
                    "type": "string",
                    "enum": ["create", "list", "get", "update", "delete"]
                },
                "taskId": {
                    "description": "Task ID (required for get/update/delete)",
                    "type": "string"
                },
                "subject": {
                    "description": "Task title in imperative form (e.g. \"Fix auth bug\")",
                    "type": "string"
                },
                "description": {
                    "description": "Detailed description of what needs to be done",
                    "type": "string"
                },
                "activeForm": {
                    "description": "Present continuous form for spinner display (e.g. \"Fixing auth bug\")",
                    "type": "string"
                },
                "priority": {
                    "description": "Task priority: p0 (critical), p1 (high), p2 (medium), p3 (low)",
                    "type": "string",
                    "enum": ["p0", "p1", "p2", "p3"]
                },
                "status": {
                    "description": "Task status: pending, in_progress, completed, deleted",
                    "type": "string",
                    "enum": ["pending", "in_progress", "completed", "deleted"]
                },
                "owner": {
                    "description": "Task owner (agent name)",
                    "type": "string"
                },
                "tags": {
                    "description": "Tags for categorization",
                    "type": "array",
                    "items": { "type": "string" }
                },
                "metadata": {
                    "description": "Arbitrary metadata to attach",
                    "type": "object"
                },
                "addBlocks": {
                    "description": "Task IDs that this task blocks (cannot start until this completes)",
                    "type": "array",
                    "items": { "type": "string" }
                },
                "addBlockedBy": {
                    "description": "Task IDs that must complete before this task can start",
                    "type": "array",
                    "items": { "type": "string" }
                }
            },
            "required": ["operation"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: TaskListInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(err) => return Ok(ToolOutput::error(format!("Invalid input: {}", err))),
        };

        match params.operation.as_str() {
            "create" => {
                let subject = match params.subject {
                    Some(s) => s,
                    None => return Ok(ToolOutput::error("Missing 'subject' for create")),
                };

                let meta = TaskMeta {
                    description: params.description.unwrap_or_default(),
                    blocks: params.add_blocks.unwrap_or_default(),
                    blocked_by: params.add_blocked_by.unwrap_or_default(),
                    active_form: params.active_form,
                    metadata: params.metadata,
                };

                let spec = WorkspaceNoteSpec {
                    folder: TASK_FOLDER.to_string(),
                    title: subject.clone(),
                    content: Self::encode_meta(&meta),
                    priority: params.priority,
                    tags: params.tags.unwrap_or_default(),
                };

                match self.provider.create(spec) {
                    Ok(note) => Ok(ToolOutput::success(json!({
                        "id": note.id,
                        "subject": note.title,
                        "status": Self::status_to_str(&note.status),
                        "priority": note.priority,
                        "tags": note.tags,
                        "description": meta.description,
                        "activeForm": meta.active_form,
                    }))),
                    Err(err) => Ok(ToolOutput::error(err)),
                }
            }

            "list" => {
                let query = WorkspaceNoteQuery {
                    folder: Some(TASK_FOLDER.to_string()),
                    status: params
                        .status
                        .as_deref()
                        .and_then(Self::str_to_status),
                    priority: params.priority,
                    tag: params.tags.as_ref().and_then(|t| t.first().cloned()),
                    assignee: params.owner,
                    search: None,
                };

                match self.provider.list(query) {
                    Ok(notes) => {
                        // Build status map for dependency resolution
                        let status_map: HashMap<String, String> = notes
                            .iter()
                            .map(|n| (n.id.clone(), Self::status_to_str(&n.status).to_string()))
                            .collect();

                        let completed_ids: HashSet<&str> = notes
                            .iter()
                            .filter(|n| matches!(n.status, WorkspaceNoteStatus::Done | WorkspaceNoteStatus::Archived))
                            .map(|n| n.id.as_str())
                            .collect();

                        let tasks: Vec<Value> = notes
                            .iter()
                            .map(|note| {
                                let meta = Self::parse_meta(&note.content);
                                // Filter blocked_by to only show open (non-completed) blockers
                                let open_blockers: Vec<&str> = meta
                                    .blocked_by
                                    .iter()
                                    .filter(|id| !completed_ids.contains(id.as_str()))
                                    .map(|s| s.as_str())
                                    .collect();

                                json!({
                                    "id": note.id,
                                    "subject": note.title,
                                    "status": Self::status_to_str(&note.status),
                                    "owner": note.assignee,
                                    "priority": note.priority,
                                    "blockedBy": open_blockers,
                                    "blocks": meta.blocks,
                                    "activeForm": meta.active_form,
                                })
                            })
                            .collect();

                        // Also provide summary counts
                        let pending = status_map.values().filter(|s| *s == "pending").count();
                        let in_progress = status_map.values().filter(|s| *s == "in_progress").count();
                        let completed = status_map.values().filter(|s| *s == "completed").count();

                        Ok(ToolOutput::success(json!({
                            "tasks": tasks,
                            "summary": {
                                "total": tasks.len(),
                                "pending": pending,
                                "in_progress": in_progress,
                                "completed": completed,
                            }
                        })))
                    }
                    Err(err) => Ok(ToolOutput::error(err)),
                }
            }

            "get" => {
                let id = match params.task_id {
                    Some(id) => id,
                    None => return Ok(ToolOutput::error("Missing 'taskId' for get")),
                };

                match self.provider.get(&id) {
                    Ok(Some(note)) => {
                        let meta = Self::parse_meta(&note.content);
                        Ok(ToolOutput::success(json!({
                            "id": note.id,
                            "subject": note.title,
                            "description": meta.description,
                            "status": Self::status_to_str(&note.status),
                            "owner": note.assignee,
                            "priority": note.priority,
                            "tags": note.tags,
                            "blocks": meta.blocks,
                            "blockedBy": meta.blocked_by,
                            "activeForm": meta.active_form,
                            "metadata": meta.metadata,
                            "createdAt": note.created_at,
                            "updatedAt": note.updated_at,
                        })))
                    }
                    Ok(None) => Ok(ToolOutput::error(format!("Task '{}' not found", id))),
                    Err(err) => Ok(ToolOutput::error(err)),
                }
            }

            "update" => {
                let id = match params.task_id {
                    Some(id) => id,
                    None => return Ok(ToolOutput::error("Missing 'taskId' for update")),
                };

                // Handle delete status
                if params.status.as_deref() == Some("deleted") {
                    return match self.provider.delete(&id) {
                        Ok(true) => Ok(ToolOutput::success(json!({
                            "id": id,
                            "status": "deleted"
                        }))),
                        Ok(false) => Ok(ToolOutput::error(format!("Task '{}' not found", id))),
                        Err(err) => Ok(ToolOutput::error(err)),
                    };
                }

                // Get existing note to merge meta
                let existing = match self.provider.get(&id) {
                    Ok(Some(note)) => note,
                    Ok(None) => return Ok(ToolOutput::error(format!("Task '{}' not found", id))),
                    Err(err) => return Ok(ToolOutput::error(err)),
                };

                let mut meta = Self::parse_meta(&existing.content);

                // Update meta fields
                if let Some(desc) = &params.description {
                    meta.description = desc.clone();
                }
                if let Some(af) = params.active_form {
                    meta.active_form = Some(af);
                }
                if let Some(md) = params.metadata {
                    // Merge metadata
                    if let Some(existing_md) = &mut meta.metadata {
                        if let (Some(existing_obj), Some(new_obj)) =
                            (existing_md.as_object_mut(), md.as_object())
                        {
                            for (k, v) in new_obj {
                                if v.is_null() {
                                    existing_obj.remove(k);
                                } else {
                                    existing_obj.insert(k.clone(), v.clone());
                                }
                            }
                        }
                    } else {
                        meta.metadata = Some(md);
                    }
                }
                if let Some(blocks) = params.add_blocks {
                    for b in blocks {
                        if !meta.blocks.contains(&b) {
                            meta.blocks.push(b);
                        }
                    }
                }
                if let Some(blocked_by) = params.add_blocked_by {
                    for b in blocked_by {
                        if !meta.blocked_by.contains(&b) {
                            meta.blocked_by.push(b);
                        }
                    }
                }

                let patch = WorkspaceNotePatch {
                    title: params.subject,
                    content: Some(Self::encode_meta(&meta)),
                    priority: params.priority,
                    status: params
                        .status
                        .as_deref()
                        .and_then(Self::str_to_status),
                    tags: params.tags,
                    assignee: params.owner,
                    folder: None,
                };

                match self.provider.update(&id, patch) {
                    Ok(note) => Ok(ToolOutput::success(json!({
                        "id": note.id,
                        "subject": note.title,
                        "status": Self::status_to_str(&note.status),
                        "owner": note.assignee,
                        "priority": note.priority,
                        "blocks": meta.blocks,
                        "blockedBy": meta.blocked_by,
                    }))),
                    Err(err) => Ok(ToolOutput::error(err)),
                }
            }

            "delete" => {
                let id = match params.task_id {
                    Some(id) => id,
                    None => return Ok(ToolOutput::error("Missing 'taskId' for delete")),
                };

                match self.provider.delete(&id) {
                    Ok(true) => Ok(ToolOutput::success(json!({
                        "id": id,
                        "deleted": true
                    }))),
                    Ok(false) => Ok(ToolOutput::error(format!("Task '{}' not found", id))),
                    Err(err) => Ok(ToolOutput::error(err)),
                }
            }

            other => Ok(ToolOutput::error(format!(
                "Unsupported task_list operation: {}",
                other
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_traits::WorkspaceNoteRecord;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MockProvider {
        notes: Mutex<HashMap<String, WorkspaceNoteRecord>>,
        counter: Mutex<u64>,
    }

    impl WorkspaceNoteProvider for MockProvider {
        fn create(
            &self,
            spec: WorkspaceNoteSpec,
        ) -> std::result::Result<WorkspaceNoteRecord, String> {
            let mut notes = self.notes.lock().map_err(|_| "Lock poisoned".to_string())?;
            let mut counter = self.counter.lock().map_err(|_| "Lock poisoned".to_string())?;
            *counter += 1;
            let id = format!("task-{}", *counter);
            let note = WorkspaceNoteRecord {
                id: id.clone(),
                folder: spec.folder,
                title: spec.title,
                content: spec.content,
                priority: spec.priority,
                status: WorkspaceNoteStatus::Open,
                tags: spec.tags,
                assignee: None,
                created_at: 1000,
                updated_at: 1000,
            };
            notes.insert(id, note.clone());
            Ok(note)
        }

        fn get(&self, id: &str) -> std::result::Result<Option<WorkspaceNoteRecord>, String> {
            let notes = self.notes.lock().map_err(|_| "Lock poisoned".to_string())?;
            Ok(notes.get(id).cloned())
        }

        fn update(
            &self,
            id: &str,
            patch: WorkspaceNotePatch,
        ) -> std::result::Result<WorkspaceNoteRecord, String> {
            let mut notes = self.notes.lock().map_err(|_| "Lock poisoned".to_string())?;
            let note = notes.get_mut(id).ok_or_else(|| "Not found".to_string())?;

            if let Some(title) = patch.title {
                note.title = title;
            }
            if let Some(content) = patch.content {
                note.content = content;
            }
            if let Some(status) = patch.status {
                note.status = status;
            }
            if let Some(assignee) = patch.assignee {
                note.assignee = Some(assignee);
            }
            if let Some(priority) = patch.priority {
                note.priority = Some(priority);
            }
            if let Some(tags) = patch.tags {
                note.tags = tags;
            }
            note.updated_at = 2000;

            Ok(note.clone())
        }

        fn delete(&self, id: &str) -> std::result::Result<bool, String> {
            let mut notes = self.notes.lock().map_err(|_| "Lock poisoned".to_string())?;
            Ok(notes.remove(id).is_some())
        }

        fn list(
            &self,
            query: WorkspaceNoteQuery,
        ) -> std::result::Result<Vec<WorkspaceNoteRecord>, String> {
            let notes = self.notes.lock().map_err(|_| "Lock poisoned".to_string())?;
            let mut result: Vec<_> = notes
                .values()
                .filter(|n| {
                    if let Some(folder) = &query.folder {
                        if n.folder != *folder {
                            return false;
                        }
                    }
                    if let Some(status) = &query.status {
                        if n.status != *status {
                            return false;
                        }
                    }
                    true
                })
                .cloned()
                .collect();
            result.sort_by(|a, b| a.id.cmp(&b.id));
            Ok(result)
        }

        fn list_folders(&self) -> std::result::Result<Vec<String>, String> {
            Ok(vec![TASK_FOLDER.to_string()])
        }
    }

    fn make_tool() -> TaskListTool {
        TaskListTool::new(Arc::new(MockProvider::default()))
    }

    #[tokio::test]
    async fn test_task_create_and_get() {
        let tool = make_tool();

        let out = tool
            .execute(json!({
                "operation": "create",
                "subject": "Fix auth bug",
                "description": "The login flow is broken",
                "activeForm": "Fixing auth bug",
                "priority": "p1"
            }))
            .await
            .unwrap();
        assert!(out.success);
        let id = out.result["id"].as_str().unwrap().to_string();
        assert_eq!(out.result["subject"], "Fix auth bug");
        assert_eq!(out.result["status"], "pending");

        // Get the task
        let out = tool
            .execute(json!({
                "operation": "get",
                "taskId": id
            }))
            .await
            .unwrap();
        assert!(out.success);
        assert_eq!(out.result["description"], "The login flow is broken");
        assert_eq!(out.result["activeForm"], "Fixing auth bug");
        assert_eq!(out.result["priority"], "p1");
    }

    #[tokio::test]
    async fn test_task_list_filter_by_status() {
        let tool = make_tool();

        // Create two tasks
        tool.execute(json!({
            "operation": "create",
            "subject": "Task A",
            "description": "First"
        }))
        .await
        .unwrap();

        let out = tool
            .execute(json!({
                "operation": "create",
                "subject": "Task B",
                "description": "Second"
            }))
            .await
            .unwrap();
        let task_b_id = out.result["id"].as_str().unwrap().to_string();

        // Move Task B to in_progress
        tool.execute(json!({
            "operation": "update",
            "taskId": task_b_id,
            "status": "in_progress"
        }))
        .await
        .unwrap();

        // List all
        let out = tool
            .execute(json!({ "operation": "list" }))
            .await
            .unwrap();
        assert!(out.success);
        assert_eq!(out.result["summary"]["total"], 2);

        // List only pending
        let out = tool
            .execute(json!({
                "operation": "list",
                "status": "pending"
            }))
            .await
            .unwrap();
        assert!(out.success);
        assert_eq!(out.result["summary"]["total"], 1);
    }

    #[tokio::test]
    async fn test_task_update_status() {
        let tool = make_tool();

        let out = tool
            .execute(json!({
                "operation": "create",
                "subject": "Task X",
                "description": "test"
            }))
            .await
            .unwrap();
        let id = out.result["id"].as_str().unwrap().to_string();

        // Move to in_progress
        let out = tool
            .execute(json!({
                "operation": "update",
                "taskId": id,
                "status": "in_progress"
            }))
            .await
            .unwrap();
        assert!(out.success);
        assert_eq!(out.result["status"], "in_progress");

        // Move to completed
        let out = tool
            .execute(json!({
                "operation": "update",
                "taskId": id,
                "status": "completed"
            }))
            .await
            .unwrap();
        assert!(out.success);
        assert_eq!(out.result["status"], "completed");
    }

    #[tokio::test]
    async fn test_task_dependencies() {
        let tool = make_tool();

        let out = tool
            .execute(json!({
                "operation": "create",
                "subject": "Build DB",
                "description": "Set up database"
            }))
            .await
            .unwrap();
        let task_a = out.result["id"].as_str().unwrap().to_string();

        let out = tool
            .execute(json!({
                "operation": "create",
                "subject": "Build API",
                "description": "Set up API layer",
                "addBlockedBy": [task_a.clone()]
            }))
            .await
            .unwrap();
        let task_b = out.result["id"].as_str().unwrap().to_string();

        // Get task B to verify dependencies
        let out = tool
            .execute(json!({
                "operation": "get",
                "taskId": task_b.clone()
            }))
            .await
            .unwrap();
        assert!(out.success);
        let blocked_by = out.result["blockedBy"].as_array().unwrap();
        assert_eq!(blocked_by.len(), 1);
        assert_eq!(blocked_by[0], task_a);

        // Add more blocks via update
        tool.execute(json!({
            "operation": "update",
            "taskId": task_a,
            "addBlocks": [task_b.clone()]
        }))
        .await
        .unwrap();

        let out = tool
            .execute(json!({
                "operation": "get",
                "taskId": task_a
            }))
            .await
            .unwrap();
        assert!(out.result["blocks"].as_array().unwrap().contains(&json!(task_b)));
    }

    #[tokio::test]
    async fn test_task_blocked_filtering() {
        let tool = make_tool();

        // Create blocker task
        let out = tool
            .execute(json!({
                "operation": "create",
                "subject": "Blocker",
                "description": "Must finish first"
            }))
            .await
            .unwrap();
        let blocker_id = out.result["id"].as_str().unwrap().to_string();

        // Create blocked task
        tool.execute(json!({
            "operation": "create",
            "subject": "Blocked",
            "description": "Waiting on blocker",
            "addBlockedBy": [blocker_id.clone()]
        }))
        .await
        .unwrap();

        // List should show blocked task with open blockers
        let out = tool
            .execute(json!({ "operation": "list" }))
            .await
            .unwrap();
        let tasks = out.result["tasks"].as_array().unwrap();
        let blocked_task = tasks.iter().find(|t| t["subject"] == "Blocked").unwrap();
        assert!(!blocked_task["blockedBy"].as_array().unwrap().is_empty());

        // Complete the blocker
        tool.execute(json!({
            "operation": "update",
            "taskId": blocker_id,
            "status": "completed"
        }))
        .await
        .unwrap();

        // Now list should show empty blockedBy for the blocked task
        let out = tool
            .execute(json!({ "operation": "list" }))
            .await
            .unwrap();
        let tasks = out.result["tasks"].as_array().unwrap();
        let blocked_task = tasks.iter().find(|t| t["subject"] == "Blocked").unwrap();
        assert!(blocked_task["blockedBy"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_task_delete() {
        let tool = make_tool();

        let out = tool
            .execute(json!({
                "operation": "create",
                "subject": "Temporary",
                "description": "Will be deleted"
            }))
            .await
            .unwrap();
        let id = out.result["id"].as_str().unwrap().to_string();

        // Delete via delete operation
        let out = tool
            .execute(json!({
                "operation": "delete",
                "taskId": id.clone()
            }))
            .await
            .unwrap();
        assert!(out.success);
        assert_eq!(out.result["deleted"], true);

        // Confirm it's gone
        let out = tool
            .execute(json!({
                "operation": "get",
                "taskId": id
            }))
            .await
            .unwrap();
        assert!(!out.success);
    }

    #[tokio::test]
    async fn test_task_delete_via_status() {
        let tool = make_tool();

        let out = tool
            .execute(json!({
                "operation": "create",
                "subject": "Delete me",
                "description": "test"
            }))
            .await
            .unwrap();
        let id = out.result["id"].as_str().unwrap().to_string();

        // Delete via update with status=deleted
        let out = tool
            .execute(json!({
                "operation": "update",
                "taskId": id.clone(),
                "status": "deleted"
            }))
            .await
            .unwrap();
        assert!(out.success);
        assert_eq!(out.result["status"], "deleted");
    }
}
