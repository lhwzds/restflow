//! Team runtime storage - byte-level API for daemon-owned team state.

use anyhow::Result;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::sync::Arc;

use crate::range_utils::prefix_range;

const TEAM_STATE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("team_runtime_states");
const TEAM_MESSAGE_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("team_runtime_messages");
const TEAM_ASSIGNMENT_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("team_runtime_assignments");
const TEAM_APPROVAL_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("team_runtime_approvals");

#[derive(Clone)]
pub struct TeamRuntimeStorage {
    db: Arc<Database>,
}

impl TeamRuntimeStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        let write_txn = db.begin_write()?;
        write_txn.open_table(TEAM_STATE_TABLE)?;
        write_txn.open_table(TEAM_MESSAGE_TABLE)?;
        write_txn.open_table(TEAM_ASSIGNMENT_TABLE)?;
        write_txn.open_table(TEAM_APPROVAL_TABLE)?;
        write_txn.commit()?;
        Ok(Self { db })
    }

    pub fn put_state_raw(&self, team_run_id: &str, data: &[u8]) -> Result<()> {
        self.put_raw(TEAM_STATE_TABLE, team_run_id, data)
    }

    pub fn get_state_raw(&self, team_run_id: &str) -> Result<Option<Vec<u8>>> {
        self.get_raw(TEAM_STATE_TABLE, team_run_id)
    }

    pub fn list_states_raw(&self) -> Result<Vec<(String, Vec<u8>)>> {
        self.list_raw(TEAM_STATE_TABLE, None)
    }

    pub fn put_message_raw(&self, team_run_id: &str, message_id: &str, data: &[u8]) -> Result<()> {
        self.put_raw(
            TEAM_MESSAGE_TABLE,
            &scoped_key(team_run_id, message_id),
            data,
        )
    }

    pub fn list_messages_raw(&self, team_run_id: &str) -> Result<Vec<(String, Vec<u8>)>> {
        self.list_raw(TEAM_MESSAGE_TABLE, Some(team_run_id))
    }

    pub fn put_assignment_raw(
        &self,
        team_run_id: &str,
        assignment_id: &str,
        data: &[u8],
    ) -> Result<()> {
        self.put_raw(
            TEAM_ASSIGNMENT_TABLE,
            &scoped_key(team_run_id, assignment_id),
            data,
        )
    }

    pub fn list_assignments_raw(&self, team_run_id: &str) -> Result<Vec<(String, Vec<u8>)>> {
        self.list_raw(TEAM_ASSIGNMENT_TABLE, Some(team_run_id))
    }

    pub fn put_approval_raw(
        &self,
        team_run_id: &str,
        approval_id: &str,
        data: &[u8],
    ) -> Result<()> {
        self.put_raw(
            TEAM_APPROVAL_TABLE,
            &scoped_key(team_run_id, approval_id),
            data,
        )
    }

    pub fn get_approval_raw(
        &self,
        team_run_id: &str,
        approval_id: &str,
    ) -> Result<Option<Vec<u8>>> {
        self.get_raw(TEAM_APPROVAL_TABLE, &scoped_key(team_run_id, approval_id))
    }

    pub fn list_approvals_raw(&self, team_run_id: &str) -> Result<Vec<(String, Vec<u8>)>> {
        self.list_raw(TEAM_APPROVAL_TABLE, Some(team_run_id))
    }

    fn put_raw(&self, table: TableDefinition<&str, &[u8]>, key: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(table)?;
            table.insert(key, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn get_raw(&self, table: TableDefinition<&str, &[u8]>, key: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(table)?;
        Ok(table.get(key)?.map(|value| value.value().to_vec()))
    }

    fn list_raw(
        &self,
        table: TableDefinition<&str, &[u8]>,
        team_run_id: Option<&str>,
    ) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(table)?;
        let mut items = Vec::new();

        if let Some(team_run_id) = team_run_id {
            let prefix = format!("{team_run_id}:");
            let (start, end) = prefix_range(&prefix);
            for item in table.range(start.as_str()..end.as_str())? {
                let (key, value) = item?;
                items.push((key.value().to_string(), value.value().to_vec()));
            }
        } else {
            for item in table.iter()? {
                let (key, value) = item?;
                items.push((key.value().to_string(), value.value().to_vec()));
            }
        }

        Ok(items)
    }
}

fn scoped_key(team_run_id: &str, id: &str) -> String {
    format!("{team_run_id}:{id}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn stores_team_runtime_records_by_scope() {
        let dir = tempdir().expect("temp dir should be created");
        let db_path = dir.path().join("team-runtime.db");
        let db = Arc::new(Database::create(db_path).expect("db should be created"));
        let storage = TeamRuntimeStorage::new(db).expect("storage should be created");

        storage
            .put_state_raw("team-1", br#"{"status":"running"}"#)
            .unwrap();
        storage
            .put_message_raw("team-1", "msg-1", br#"{"content":"hello"}"#)
            .unwrap();
        storage
            .put_assignment_raw("team-1", "assign-1", br#"{"content":"task"}"#)
            .unwrap();
        storage
            .put_approval_raw("team-1", "approval-1", br#"{"approved":false}"#)
            .unwrap();

        assert!(storage.get_state_raw("team-1").unwrap().is_some());
        assert_eq!(storage.list_states_raw().unwrap().len(), 1);
        assert_eq!(storage.list_messages_raw("team-1").unwrap().len(), 1);
        assert_eq!(storage.list_assignments_raw("team-1").unwrap().len(), 1);
        assert_eq!(storage.list_approvals_raw("team-1").unwrap().len(), 1);
        assert!(
            storage
                .get_approval_raw("team-1", "approval-1")
                .unwrap()
                .is_some()
        );
    }
}
