//! Typed run artifact storage wrapper.

use crate::models::RunArtifact;
use anyhow::Result;
use redb::Database;
use std::sync::Arc;

#[derive(Clone)]
pub struct RunArtifactStorage {
    inner: restflow_storage::RunArtifactStorage,
}

impl RunArtifactStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Ok(Self {
            inner: restflow_storage::RunArtifactStorage::new(db)?,
        })
    }

    pub fn save(&self, artifact: &RunArtifact) -> Result<()> {
        let json_bytes = serde_json::to_vec(artifact)?;
        self.inner.put_raw_with_indexes(
            &artifact.id,
            &artifact.run_id,
            artifact.task_id.as_deref(),
            artifact.team_run_id.as_deref(),
            &json_bytes,
        )
    }

    pub fn get(&self, id: &str) -> Result<Option<RunArtifact>> {
        let Some(bytes) = self.inner.get_raw(id)? else {
            return Ok(None);
        };
        Ok(Some(serde_json::from_slice(&bytes)?))
    }

    pub fn list_by_run(&self, run_id: &str) -> Result<Vec<RunArtifact>> {
        self.decode_sorted(self.inner.list_by_run_raw(run_id)?)
    }

    pub fn list_by_task(&self, task_id: &str) -> Result<Vec<RunArtifact>> {
        self.decode_sorted(self.inner.list_by_task_raw(task_id)?)
    }

    pub fn list_by_team(&self, team_run_id: &str) -> Result<Vec<RunArtifact>> {
        self.decode_sorted(self.inner.list_by_team_raw(team_run_id)?)
    }

    fn decode_sorted(&self, raw: Vec<(String, Vec<u8>)>) -> Result<Vec<RunArtifact>> {
        let mut artifacts = raw
            .into_iter()
            .map(|(_, bytes)| serde_json::from_slice::<RunArtifact>(&bytes))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        artifacts.sort_by_key(|artifact| artifact.created_at);
        Ok(artifacts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::RunArtifactKind;
    use tempfile::tempdir;

    fn artifact(id: &str, run_id: &str, task_id: Option<&str>, created_at: i64) -> RunArtifact {
        RunArtifact {
            id: id.to_string(),
            run_id: run_id.to_string(),
            task_id: task_id.map(ToOwned::to_owned),
            team_run_id: None,
            kind: RunArtifactKind::FinalOutput,
            title: "Final output".to_string(),
            content: Some("done".to_string()),
            content_ref: None,
            content_type: Some("text/plain".to_string()),
            size_bytes: 4,
            created_at,
            metadata: None,
        }
    }

    #[test]
    fn stores_and_lists_typed_artifacts() {
        let dir = tempdir().expect("temp dir should be created");
        let db_path = dir.path().join("run-artifacts.db");
        let db = Arc::new(Database::create(db_path).expect("db should be created"));
        let storage = RunArtifactStorage::new(db).expect("storage should be created");

        storage
            .save(&artifact("a1", "run-1", Some("task-1"), 2))
            .expect("save should succeed");
        storage
            .save(&artifact("a2", "run-1", Some("task-1"), 1))
            .expect("save should succeed");

        let by_run = storage.list_by_run("run-1").expect("list by run");
        assert_eq!(
            by_run
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["a2", "a1"]
        );
        assert!(storage.get("a1").unwrap().unwrap().has_payload());
        assert_eq!(storage.list_by_task("task-1").unwrap().len(), 2);
    }
}
