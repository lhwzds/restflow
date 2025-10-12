use crate::models::Workflow;
use anyhow::Result;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::sync::Arc;

pub const WORKFLOW_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("workflow");

pub struct WorkflowStorage {
    db: Arc<Database>,
}

impl WorkflowStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        // Create table if not exists
        let write_txn = db.begin_write()?;
        write_txn.open_table(WORKFLOW_TABLE)?;
        write_txn.commit()?;

        Ok(Self { db })
    }

    pub fn create_workflow(&self, workflow: &Workflow) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(WORKFLOW_TABLE)?;
            let json_bytes = serde_json::to_vec(workflow)?;
            table.insert(workflow.id.as_str(), json_bytes.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn get_workflow(&self, id: &str) -> Result<Workflow> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(WORKFLOW_TABLE)?;

        if let Some(value) = table.get(id)? {
            let workflow: Workflow = serde_json::from_slice(value.value())?;
            Ok(workflow)
        } else {
            Err(anyhow::anyhow!("Workflow {} not found", id))
        }
    }

    pub fn list_workflows(&self) -> Result<Vec<Workflow>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(WORKFLOW_TABLE)?;

        let mut workflows = Vec::new();
        for item in table.iter()? {
            let (_, value) = item?;
            let workflow: Workflow = serde_json::from_slice(value.value())?;
            workflows.push(workflow);
        }

        Ok(workflows)
    }

    pub fn update_workflow(&self, id: &str, workflow: &Workflow) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(WORKFLOW_TABLE)?;

            if table.get(id)?.is_none() {
                return Err(anyhow::anyhow!("Workflow not found"));
            }

            let json_bytes = serde_json::to_vec(workflow)?;
            table.insert(id, json_bytes.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn delete_workflow(&self, id: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(WORKFLOW_TABLE)?;

            if table.get(id)?.is_none() {
                return Err(anyhow::anyhow!("Workflow not found"));
            }

            table.remove(id)?;
        }
        write_txn.commit()?;
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
                        "model": "gpt-4",
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

        // Create workflow
        storage.create_workflow(&workflow).unwrap();

        // Get workflow
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

        // Create multiple workflows
        for i in 1..=3 {
            let workflow = create_test_workflow(&format!("wf-{:03}", i));
            storage.create_workflow(&workflow).unwrap();
        }

        // List workflows
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

        // Create initial workflow
        let mut workflow = create_test_workflow("wf-001");
        storage.create_workflow(&workflow).unwrap();

        // Update workflow
        workflow.name = "Updated Workflow".to_string();
        workflow.nodes.push(Node {
            id: "node3".to_string(),
            node_type: NodeType::DataTransform,
            config: serde_json::json!({"transform": "x + 10"}),
            position: None,
        });

        storage.update_workflow("wf-001", &workflow).unwrap();

        // Verify update
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

        // Create workflow
        let workflow = create_test_workflow("wf-001");
        storage.create_workflow(&workflow).unwrap();

        // Delete workflow
        storage.delete_workflow("wf-001").unwrap();

        // Verify deletion
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
