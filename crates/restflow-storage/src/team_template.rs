//! Team template storage - byte-level API for reusable team definitions.

use anyhow::Result;
use redb::{Database, ReadableDatabase, TableDefinition};
use std::sync::Arc;

use crate::range_utils::prefix_range;

const TEAM_TEMPLATE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("team_templates");

#[derive(Clone)]
pub struct TeamTemplateStorage {
    db: Arc<Database>,
}

impl TeamTemplateStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        let write_txn = db.begin_write()?;
        write_txn.open_table(TEAM_TEMPLATE_TABLE)?;
        write_txn.commit()?;

        Ok(Self { db })
    }

    pub fn put_raw(&self, key: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TEAM_TEMPLATE_TABLE)?;
            table.insert(key, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn get_raw(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(TEAM_TEMPLATE_TABLE)?;
        Ok(table.get(key)?.map(|value| value.value().to_vec()))
    }

    pub fn delete(&self, key: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let existed = {
            let mut table = write_txn.open_table(TEAM_TEMPLATE_TABLE)?;
            table.remove(key)?.is_some()
        };
        write_txn.commit()?;
        Ok(existed)
    }

    pub fn list_raw(&self, namespace: &str) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(TEAM_TEMPLATE_TABLE)?;
        let prefix = format!("{namespace}:");
        let (start, end) = prefix_range(&prefix);
        let mut entries = Vec::new();

        for item in table.range(start.as_str()..end.as_str())? {
            let (key, value) = item?;
            entries.push((key.value().to_string(), value.value().to_vec()));
        }

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn stores_lists_and_deletes_templates() {
        let dir = tempdir().expect("temp dir should be created");
        let db_path = dir.path().join("team-template.db");
        let db = Arc::new(Database::create(db_path).expect("db should be created"));
        let storage = TeamTemplateStorage::new(db).expect("storage should be created");

        storage
            .put_raw("subagent_team:review", br#"{"team":"review"}"#)
            .expect("put should succeed");

        assert!(storage.get_raw("subagent_team:review").unwrap().is_some());
        assert_eq!(storage.list_raw("subagent_team").unwrap().len(), 1);
        assert!(storage.delete("subagent_team:review").unwrap());
        assert!(storage.get_raw("subagent_team:review").unwrap().is_none());
    }
}
