//! Run artifact storage - byte-level API for persisted run outputs.

use anyhow::Result;
use redb::{Database, ReadableDatabase, TableDefinition};
use std::sync::Arc;

use crate::range_utils::prefix_range;

const RUN_ARTIFACT_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("run_artifacts");
const RUN_ARTIFACT_RUN_INDEX_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("run_artifact_run_index");
const RUN_ARTIFACT_TASK_INDEX_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("run_artifact_task_index");
const RUN_ARTIFACT_TEAM_INDEX_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("run_artifact_team_index");

#[derive(Clone)]
pub struct RunArtifactStorage {
    db: Arc<Database>,
}

impl RunArtifactStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        let write_txn = db.begin_write()?;
        write_txn.open_table(RUN_ARTIFACT_TABLE)?;
        write_txn.open_table(RUN_ARTIFACT_RUN_INDEX_TABLE)?;
        write_txn.open_table(RUN_ARTIFACT_TASK_INDEX_TABLE)?;
        write_txn.open_table(RUN_ARTIFACT_TEAM_INDEX_TABLE)?;
        write_txn.commit()?;

        Ok(Self { db })
    }

    pub fn put_raw_with_indexes(
        &self,
        id: &str,
        run_id: &str,
        task_id: Option<&str>,
        team_run_id: Option<&str>,
        data: &[u8],
    ) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(RUN_ARTIFACT_TABLE)?;
            table.insert(id, data)?;

            let mut run_index = write_txn.open_table(RUN_ARTIFACT_RUN_INDEX_TABLE)?;
            let run_key = format!("{run_id}:{id}");
            run_index.insert(run_key.as_str(), id)?;

            if let Some(task_id) = task_id {
                let mut task_index = write_txn.open_table(RUN_ARTIFACT_TASK_INDEX_TABLE)?;
                let task_key = format!("{task_id}:{id}");
                task_index.insert(task_key.as_str(), id)?;
            }

            if let Some(team_run_id) = team_run_id {
                let mut team_index = write_txn.open_table(RUN_ARTIFACT_TEAM_INDEX_TABLE)?;
                let team_key = format!("{team_run_id}:{id}");
                team_index.insert(team_key.as_str(), id)?;
            }
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn get_raw(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(RUN_ARTIFACT_TABLE)?;
        Ok(table.get(id)?.map(|value| value.value().to_vec()))
    }

    pub fn list_by_run_raw(&self, run_id: &str) -> Result<Vec<(String, Vec<u8>)>> {
        self.list_by_index_raw(RUN_ARTIFACT_RUN_INDEX_TABLE, run_id)
    }

    pub fn list_by_task_raw(&self, task_id: &str) -> Result<Vec<(String, Vec<u8>)>> {
        self.list_by_index_raw(RUN_ARTIFACT_TASK_INDEX_TABLE, task_id)
    }

    pub fn list_by_team_raw(&self, team_run_id: &str) -> Result<Vec<(String, Vec<u8>)>> {
        self.list_by_index_raw(RUN_ARTIFACT_TEAM_INDEX_TABLE, team_run_id)
    }

    fn list_by_index_raw(
        &self,
        index_table: TableDefinition<&str, &str>,
        index_id: &str,
    ) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let index = read_txn.open_table(index_table)?;
        let artifact_table = read_txn.open_table(RUN_ARTIFACT_TABLE)?;
        let prefix = format!("{index_id}:");
        let (start, end) = prefix_range(&prefix);
        let mut artifacts = Vec::new();

        for item in index.range(start.as_str()..end.as_str())? {
            let (_, value) = item?;
            let artifact_id = value.value();
            if let Some(data) = artifact_table.get(artifact_id)? {
                artifacts.push((artifact_id.to_string(), data.value().to_vec()));
            }
        }

        Ok(artifacts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn stores_and_lists_artifacts_by_indexes() {
        let dir = tempdir().expect("temp dir should be created");
        let db_path = dir.path().join("run-artifacts.db");
        let db = Arc::new(Database::create(db_path).expect("db should be created"));
        let storage = RunArtifactStorage::new(db).expect("storage should be created");

        storage
            .put_raw_with_indexes(
                "artifact-1",
                "run-1",
                Some("task-1"),
                Some("team-1"),
                br#"{"title":"A"}"#,
            )
            .expect("put should succeed");

        assert!(storage.get_raw("artifact-1").unwrap().is_some());
        assert_eq!(storage.list_by_run_raw("run-1").unwrap().len(), 1);
        assert_eq!(storage.list_by_task_raw("task-1").unwrap().len(), 1);
        assert_eq!(storage.list_by_team_raw("team-1").unwrap().len(), 1);
    }
}
