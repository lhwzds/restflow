//! Typed deliverable storage wrapper.

use crate::models::Deliverable;
use anyhow::Result;
use redb::Database;
use std::sync::Arc;

/// Typed deliverable storage wrapper around restflow-storage::DeliverableStorage.
#[derive(Clone)]
pub struct DeliverableStorage {
    inner: restflow_storage::DeliverableStorage,
}

impl DeliverableStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Ok(Self {
            inner: restflow_storage::DeliverableStorage::new(db)?,
        })
    }

    pub fn save(&self, deliverable: &Deliverable) -> Result<()> {
        let json_bytes = serde_json::to_vec(deliverable)?;
        self.inner.put_raw_with_indexes(
            &deliverable.id,
            &deliverable.task_id,
            &deliverable.execution_id,
            &json_bytes,
        )
    }

    pub fn get(&self, id: &str) -> Result<Option<Deliverable>> {
        if let Some(bytes) = self.inner.get_raw(id)? {
            Ok(Some(serde_json::from_slice(&bytes)?))
        } else {
            Ok(None)
        }
    }

    pub fn list_by_task(&self, task_id: &str) -> Result<Vec<Deliverable>> {
        let mut items = self
            .inner
            .list_by_task_raw(task_id)?
            .into_iter()
            .map(|(_, bytes)| serde_json::from_slice::<Deliverable>(&bytes))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        items.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(items)
    }

    pub fn list_by_execution(&self, execution_id: &str) -> Result<Vec<Deliverable>> {
        let mut items = self
            .inner
            .list_by_execution_raw(execution_id)?
            .into_iter()
            .map(|(_, bytes)| serde_json::from_slice::<Deliverable>(&bytes))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        items.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(items)
    }

    pub fn delete(&self, id: &str) -> Result<bool> {
        let Some(existing) = self.get(id)? else {
            return Ok(false);
        };
        self.inner
            .delete(id, &existing.task_id, &existing.execution_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Deliverable, DeliverableType};
    use redb::Database;
    use tempfile::tempdir;

    fn setup() -> (DeliverableStorage, tempfile::TempDir) {
        let dir = tempdir().expect("temp dir should be created");
        let db_path = dir.path().join("deliverable-core.db");
        let db = Arc::new(Database::create(db_path).expect("db should be created"));
        (
            DeliverableStorage::new(db).expect("storage should be created"),
            dir,
        )
    }

    fn sample(id: &str, task_id: &str, execution_id: &str, created_at: i64) -> Deliverable {
        Deliverable {
            id: id.to_string(),
            task_id: task_id.to_string(),
            execution_id: execution_id.to_string(),
            deliverable_type: DeliverableType::Report,
            title: "Weekly Summary".to_string(),
            content: "# Summary".to_string(),
            file_path: None,
            content_type: Some("text/markdown".to_string()),
            size_bytes: 9,
            created_at,
            metadata: None,
        }
    }

    #[test]
    fn test_deliverable_storage_crud() {
        let (storage, _dir) = setup();

        let d1 = sample("d1", "t1", "e1", 1000);
        let d2 = sample("d2", "t1", "e2", 2000);
        storage.save(&d1).expect("save d1 should succeed");
        storage.save(&d2).expect("save d2 should succeed");

        let loaded = storage
            .get("d1")
            .expect("get should succeed")
            .expect("d1 should exist");
        assert_eq!(loaded.title, "Weekly Summary");

        let by_task = storage
            .list_by_task("t1")
            .expect("list by task should work");
        assert_eq!(by_task.len(), 2);
        assert_eq!(by_task[0].id, "d1");

        let by_exec = storage
            .list_by_execution("e2")
            .expect("list by execution should work");
        assert_eq!(by_exec.len(), 1);
        assert_eq!(by_exec[0].id, "d2");

        let deleted = storage.delete("d1").expect("delete should succeed");
        assert!(deleted);

        let missing = storage.get("d1").expect("get should succeed");
        assert!(missing.is_none());
    }
}
