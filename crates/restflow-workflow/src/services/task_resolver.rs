//! Task resolver service for resolving Task dependencies from storage.
//!
//! This service breaks the circular dependency between Task and Storage
//! by extracting storage-dependent methods from Task into a separate service.

use crate::models::{Node, Task, Workflow};
use crate::storage::Storage;
use anyhow::Result;
use std::sync::Arc;

/// Resolves Task dependencies (workflow, node) from storage.
///
/// This service provides methods that were previously on Task that required Storage.
/// By moving these methods here, we break the circular dependency between
/// Task (in models) and Storage.
pub struct TaskResolver<'a> {
    storage: &'a Storage,
}

impl<'a> TaskResolver<'a> {
    /// Create a new TaskResolver with the given storage reference.
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }

    /// Get the node for a task by loading the workflow and finding the node.
    ///
    /// This method will use the cached workflow if available, otherwise
    /// it will load the workflow from storage.
    pub fn get_node(&self, task: &Task) -> Result<Node> {
        let workflow = self.get_workflow(task)?;
        workflow
            .nodes
            .iter()
            .find(|n| n.id == task.node_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Node {} not found in workflow", task.node_id))
    }

    /// Get the workflow for a task.
    ///
    /// Returns the cached workflow if available (set via Task::set_workflow),
    /// otherwise loads from storage.
    pub fn get_workflow(&self, task: &Task) -> Result<Arc<Workflow>> {
        // Check if workflow is already cached in task
        if let Some(cached) = task.cached_workflow() {
            return Ok(cached);
        }

        // Load from storage
        Ok(Arc::new(
            self.storage.workflows.get_workflow(&task.workflow_id)?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::context::ExecutionContext;
    use crate::models::{ManualTriggerInput, NodeInput, NodeType};
    use tempfile::tempdir;

    fn create_test_input() -> NodeInput {
        NodeInput::ManualTrigger(ManualTriggerInput { payload: None })
    }

    #[test]
    fn test_get_workflow_from_storage() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let storage = Storage::new(db_path.to_str().unwrap()).unwrap();

        // Create a workflow
        let workflow = Workflow {
            id: "wf-1".to_string(),
            name: "Test Workflow".to_string(),
            nodes: vec![Node {
                id: "node-1".to_string(),
                node_type: NodeType::Print,
                config: serde_json::json!({"type": "Print", "data": {"message": "test"}}),
                position: None,
            }],
            edges: vec![],
        };
        storage.workflows.create_workflow(&workflow).unwrap();

        // Create a task referencing the workflow
        let task = Task::new(
            "exec-1".to_string(),
            "wf-1".to_string(),
            "node-1".to_string(),
            create_test_input(),
            ExecutionContext::new("exec-1".to_string()),
        );

        // Resolve workflow via TaskResolver
        let resolver = TaskResolver::new(&storage);
        let resolved_workflow = resolver.get_workflow(&task).unwrap();

        assert_eq!(resolved_workflow.id, "wf-1");
        assert_eq!(resolved_workflow.name, "Test Workflow");
    }

    #[test]
    fn test_get_workflow_from_cache() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let storage = Storage::new(db_path.to_str().unwrap()).unwrap();

        // Create a workflow
        let workflow = Workflow {
            id: "wf-1".to_string(),
            name: "Test Workflow".to_string(),
            nodes: vec![Node {
                id: "node-1".to_string(),
                node_type: NodeType::Print,
                config: serde_json::json!({"type": "Print", "data": {"message": "test"}}),
                position: None,
            }],
            edges: vec![],
        };

        // Create a task and pre-cache the workflow
        let task = Task::new(
            "exec-1".to_string(),
            "wf-1".to_string(),
            "node-1".to_string(),
            create_test_input(),
            ExecutionContext::new("exec-1".to_string()),
        );

        // Pre-cache the workflow (without storing in DB)
        let _ = task.set_workflow(Arc::new(workflow));

        // Resolve workflow - should use cache (no DB lookup)
        let resolver = TaskResolver::new(&storage);
        let resolved_workflow = resolver.get_workflow(&task).unwrap();

        assert_eq!(resolved_workflow.id, "wf-1");
    }

    #[test]
    fn test_get_node() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let storage = Storage::new(db_path.to_str().unwrap()).unwrap();

        // Create a workflow with multiple nodes
        let workflow = Workflow {
            id: "wf-1".to_string(),
            name: "Test Workflow".to_string(),
            nodes: vec![
                Node {
                    id: "node-1".to_string(),
                    node_type: NodeType::Print,
                    config: serde_json::json!({"type": "Print", "data": {"message": "test1"}}),
                    position: None,
                },
                Node {
                    id: "node-2".to_string(),
                    node_type: NodeType::HttpRequest,
                    config: serde_json::json!({"type": "HttpRequest", "data": {"url": "http://example.com"}}),
                    position: None,
                },
            ],
            edges: vec![],
        };
        storage.workflows.create_workflow(&workflow).unwrap();

        // Create a task for node-2
        let task = Task::new(
            "exec-1".to_string(),
            "wf-1".to_string(),
            "node-2".to_string(),
            create_test_input(),
            ExecutionContext::new("exec-1".to_string()),
        );

        // Resolve node via TaskResolver
        let resolver = TaskResolver::new(&storage);
        let resolved_node = resolver.get_node(&task).unwrap();

        assert_eq!(resolved_node.id, "node-2");
        assert_eq!(resolved_node.node_type, NodeType::HttpRequest);
    }

    #[test]
    fn test_get_node_not_found() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let storage = Storage::new(db_path.to_str().unwrap()).unwrap();

        // Create a workflow with one node
        let workflow = Workflow {
            id: "wf-1".to_string(),
            name: "Test Workflow".to_string(),
            nodes: vec![Node {
                id: "node-1".to_string(),
                node_type: NodeType::Print,
                config: serde_json::json!({"type": "Print", "data": {"message": "test"}}),
                position: None,
            }],
            edges: vec![],
        };
        storage.workflows.create_workflow(&workflow).unwrap();

        // Create a task referencing a non-existent node
        let task = Task::new(
            "exec-1".to_string(),
            "wf-1".to_string(),
            "nonexistent-node".to_string(),
            create_test_input(),
            ExecutionContext::new("exec-1".to_string()),
        );

        // Resolve node - should fail
        let resolver = TaskResolver::new(&storage);
        let result = resolver.get_node(&task);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
}
