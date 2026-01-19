//! Typed workflow storage wrapper.

use crate::models::Workflow;
use anyhow::Result;
use redb::Database;
use std::sync::Arc;

/// Typed workflow storage wrapper around restflow-storage::WorkflowStorage.
pub struct WorkflowStorage {
    inner: restflow_storage::WorkflowStorage,
}

impl WorkflowStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Ok(Self {
            inner: restflow_storage::WorkflowStorage::new(db)?,
        })
    }

    /// Create a new workflow
    pub fn create_workflow(&self, workflow: &Workflow) -> Result<()> {
        let json_bytes = serde_json::to_vec(workflow)?;
        self.inner.put_raw(&workflow.id, &json_bytes)
    }

    /// Get a workflow by ID
    pub fn get_workflow(&self, id: &str) -> Result<Workflow> {
        let bytes = self
            .inner
            .get_raw(id)?
            .ok_or_else(|| anyhow::anyhow!("Workflow {} not found", id))?;
        let workflow: Workflow = serde_json::from_slice(&bytes)?;
        Ok(workflow)
    }

    /// List all workflows
    pub fn list_workflows(&self) -> Result<Vec<Workflow>> {
        let raw_workflows = self.inner.list_raw()?;
        let mut workflows = Vec::new();
        for (_, bytes) in raw_workflows {
            let workflow: Workflow = serde_json::from_slice(&bytes)?;
            workflows.push(workflow);
        }
        Ok(workflows)
    }

    /// Update an existing workflow
    pub fn update_workflow(&self, id: &str, workflow: &Workflow) -> Result<()> {
        if !self.inner.exists(id)? {
            return Err(anyhow::anyhow!("Workflow not found"));
        }
        let json_bytes = serde_json::to_vec(workflow)?;
        self.inner.put_raw(id, &json_bytes)
    }

    /// Delete a workflow by ID
    pub fn delete_workflow(&self, id: &str) -> Result<()> {
        if !self.inner.delete(id)? {
            return Err(anyhow::anyhow!("Workflow not found"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Edge, Node, NodeType, Workflow};
    use tempfile::tempdir;

    fn create_test_workflow(id: &str) -> Workflow {
        Workflow {
            id: id.to_string(),
            name: format!("Test Workflow {}", id),
            nodes: vec![
                Node {
                    id: "node1".to_string(),
                    node_type: NodeType::Agent,
                    config: serde_json::json!({
                        "model": "gpt-5",
                        "prompt": "Test prompt"
                    }),
                    position: None,
                },
                Node {
                    id: "node2".to_string(),
                    node_type: NodeType::HttpRequest,
                    config: serde_json::json!({
                        "url": "https://api.example.com",
                        "method": "GET"
                    }),
                    position: None,
                },
            ],
            edges: vec![Edge {
                from: "node1".to_string(),
                to: "node2".to_string(),
            }],
        }
    }

    #[test]
    fn test_create_and_get_workflow() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = WorkflowStorage::new(db).unwrap();

        let workflow = create_test_workflow("wf-001");

        storage.create_workflow(&workflow).unwrap();

        let retrieved = storage.get_workflow("wf-001").unwrap();
        assert_eq!(retrieved.id, "wf-001");
        assert_eq!(retrieved.name, "Test Workflow wf-001");
        assert_eq!(retrieved.nodes.len(), 2);
        assert_eq!(retrieved.edges.len(), 1);
    }

    #[test]
    fn test_list_workflows() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = WorkflowStorage::new(db).unwrap();

        for i in 1..=3 {
            let workflow = create_test_workflow(&format!("wf-{:03}", i));
            storage.create_workflow(&workflow).unwrap();
        }

        let workflows = storage.list_workflows().unwrap();
        assert_eq!(workflows.len(), 3);

        let ids: Vec<String> = workflows.iter().map(|w| w.id.clone()).collect();
        assert!(ids.contains(&"wf-001".to_string()));
        assert!(ids.contains(&"wf-002".to_string()));
        assert!(ids.contains(&"wf-003".to_string()));
    }

    #[test]
    fn test_update_workflow() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = WorkflowStorage::new(db).unwrap();

        let mut workflow = create_test_workflow("wf-001");
        storage.create_workflow(&workflow).unwrap();

        workflow.name = "Updated Workflow".to_string();
        workflow.nodes.push(Node {
            id: "node3".to_string(),
            node_type: NodeType::DataTransform,
            config: serde_json::json!({"transform": "x + 10"}),
            position: None,
        });

        storage.update_workflow("wf-001", &workflow).unwrap();

        let retrieved = storage.get_workflow("wf-001").unwrap();
        assert_eq!(retrieved.name, "Updated Workflow");
        assert_eq!(retrieved.nodes.len(), 3);
    }

    #[test]
    fn test_update_nonexistent_workflow() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = WorkflowStorage::new(db).unwrap();

        let workflow = create_test_workflow("nonexistent");
        let result = storage.update_workflow("nonexistent", &workflow);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_delete_workflow() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = WorkflowStorage::new(db).unwrap();

        let workflow = create_test_workflow("wf-001");
        storage.create_workflow(&workflow).unwrap();

        storage.delete_workflow("wf-001").unwrap();

        let result = storage.get_workflow("wf-001");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_delete_nonexistent_workflow() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = WorkflowStorage::new(db).unwrap();

        let result = storage.delete_workflow("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_get_nonexistent_workflow() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = WorkflowStorage::new(db).unwrap();

        let result = storage.get_workflow("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
}
