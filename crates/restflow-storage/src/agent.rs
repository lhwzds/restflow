//! Agent storage - byte-level API for agent persistence.

use anyhow::Result;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::sync::Arc;

const AGENT_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("agents");

/// Low-level agent storage with byte-level API
pub struct AgentStorage {
    db: Arc<Database>,
}

impl AgentStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        let write_txn = db.begin_write()?;
        write_txn.open_table(AGENT_TABLE)?;
        write_txn.commit()?;

        Ok(Self { db })
    }

    /// Store raw agent data
    pub fn put_raw(&self, id: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(AGENT_TABLE)?;
            table.insert(id, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get raw agent data by ID
    pub fn get_raw(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(AGENT_TABLE)?;

        if let Some(value) = table.get(id)? {
            Ok(Some(value.value().to_vec()))
        } else {
            Ok(None)
        }
    }

    /// List all raw agent data
    pub fn list_raw(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(AGENT_TABLE)?;

        let mut agents = Vec::new();
        for item in table.iter()? {
            let (key, value) = item?;
            agents.push((key.value().to_string(), value.value().to_vec()));
        }

        Ok(agents)
    }

    /// Delete agent by ID
    pub fn delete(&self, id: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let existed = {
            let mut table = write_txn.open_table(AGENT_TABLE)?;
            table.remove(id)?.is_some()
        };
        write_txn.commit()?;
        Ok(existed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_put_and_get_raw() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = AgentStorage::new(db).unwrap();

        let data = b"test agent data";
        storage.put_raw("agent-001", data).unwrap();

        let retrieved = storage.get_raw("agent-001").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), data);
    }

    #[test]
    fn test_list_raw() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = AgentStorage::new(db).unwrap();

        storage.put_raw("agent-001", b"data1").unwrap();
        storage.put_raw("agent-002", b"data2").unwrap();

        let agents = storage.list_raw().unwrap();
        assert_eq!(agents.len(), 2);
    }

    #[test]
    fn test_delete() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = AgentStorage::new(db).unwrap();

        storage.put_raw("agent-001", b"data").unwrap();

        let deleted = storage.delete("agent-001").unwrap();
        assert!(deleted);

        let retrieved = storage.get_raw("agent-001").unwrap();
        assert!(retrieved.is_none());
    }
}
