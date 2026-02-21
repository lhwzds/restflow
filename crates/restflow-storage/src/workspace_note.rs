//! Workspace note storage with folder/status indexes.

use anyhow::{Result, anyhow};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::sync::Arc;

use crate::range_utils::prefix_range;

const NOTES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("workspace_notes");
const FOLDER_INDEX_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("workspace_note_folder_index");
const STATUS_INDEX_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("workspace_note_status_index");

#[derive(Debug, Clone)]
pub struct WorkspaceNoteStorage {
    db: Arc<Database>,
}

impl WorkspaceNoteStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        let write_txn = db.begin_write()?;
        write_txn.open_table(NOTES_TABLE)?;
        write_txn.open_table(FOLDER_INDEX_TABLE)?;
        write_txn.open_table(STATUS_INDEX_TABLE)?;
        write_txn.commit()?;

        Ok(Self { db })
    }

    pub fn create(&self, id: &str, data: &[u8], folder: &str, status: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;

        {
            let mut notes = write_txn.open_table(NOTES_TABLE)?;
            if notes.get(id)?.is_some() {
                return Err(anyhow!("Workspace note {} already exists", id));
            }
            notes.insert(id, data)?;
        }

        {
            let mut folder_index = write_txn.open_table(FOLDER_INDEX_TABLE)?;
            let folder_key = Self::folder_key(folder, id);
            folder_index.insert(folder_key.as_str(), id)?;
        }

        {
            let mut status_index = write_txn.open_table(STATUS_INDEX_TABLE)?;
            let status_key = Self::status_key(status, id);
            status_index.insert(status_key.as_str(), id)?;
        }

        write_txn.commit()?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let notes = read_txn.open_table(NOTES_TABLE)?;
        Ok(notes.get(id)?.map(|value| value.value().to_vec()))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &self,
        id: &str,
        data: &[u8],
        old_folder: &str,
        old_status: &str,
        new_folder: &str,
        new_status: &str,
    ) -> Result<()> {
        let write_txn = self.db.begin_write()?;

        {
            let mut notes = write_txn.open_table(NOTES_TABLE)?;
            if notes.get(id)?.is_none() {
                return Err(anyhow!("Workspace note {} not found", id));
            }
            notes.insert(id, data)?;
        }

        {
            let mut folder_index = write_txn.open_table(FOLDER_INDEX_TABLE)?;
            let old_key = Self::folder_key(old_folder, id);
            let new_key = Self::folder_key(new_folder, id);
            folder_index.remove(old_key.as_str())?;
            folder_index.insert(new_key.as_str(), id)?;
        }

        {
            let mut status_index = write_txn.open_table(STATUS_INDEX_TABLE)?;
            let old_key = Self::status_key(old_status, id);
            let new_key = Self::status_key(new_status, id);
            status_index.remove(old_key.as_str())?;
            status_index.insert(new_key.as_str(), id)?;
        }

        write_txn.commit()?;
        Ok(())
    }

    pub fn delete(&self, id: &str, folder: &str, status: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;

        let removed = {
            let mut notes = write_txn.open_table(NOTES_TABLE)?;
            notes.remove(id)?.is_some()
        };

        if removed {
            let mut folder_index = write_txn.open_table(FOLDER_INDEX_TABLE)?;
            let folder_key = Self::folder_key(folder, id);
            folder_index.remove(folder_key.as_str())?;

            let mut status_index = write_txn.open_table(STATUS_INDEX_TABLE)?;
            let status_key = Self::status_key(status, id);
            status_index.remove(status_key.as_str())?;
        }

        write_txn.commit()?;
        Ok(removed)
    }

    pub fn list(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let notes = read_txn.open_table(NOTES_TABLE)?;

        let mut items = Vec::new();
        for row in notes.iter()? {
            let (key, value) = row?;
            items.push((key.value().to_string(), value.value().to_vec()));
        }
        Ok(items)
    }

    pub fn list_by_folder(&self, folder: &str) -> Result<Vec<(String, Vec<u8>)>> {
        self.list_by_index(folder, FOLDER_INDEX_TABLE, Self::folder_prefix)
    }

    pub fn list_by_status(&self, status: &str) -> Result<Vec<(String, Vec<u8>)>> {
        self.list_by_index(status, STATUS_INDEX_TABLE, Self::status_prefix)
    }

    fn list_by_index(
        &self,
        key: &str,
        index_table: TableDefinition<&str, &str>,
        prefix_fn: fn(&str) -> String,
    ) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let index = read_txn.open_table(index_table)?;
        let notes = read_txn.open_table(NOTES_TABLE)?;
        let prefix = prefix_fn(key);
        let (start, end) = prefix_range(&prefix);

        let mut items = Vec::new();
        for row in index.range(start.as_str()..end.as_str())? {
            let (_index_key, note_id) = row?;
            if let Some(value) = notes.get(note_id.value())? {
                items.push((note_id.value().to_string(), value.value().to_vec()));
            }
        }
        Ok(items)
    }

    fn folder_key(folder: &str, id: &str) -> String {
        format!("{}:{}", folder, id)
    }

    fn status_key(status: &str, id: &str) -> String {
        format!("{}:{}", status, id)
    }

    fn folder_prefix(folder: &str) -> String {
        format!("{}:", folder)
    }

    fn status_prefix(status: &str) -> String {
        format!("{}:", status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn setup() -> WorkspaceNoteStorage {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("workspace-note.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        WorkspaceNoteStorage::new(db).unwrap()
    }

    #[test]
    fn create_get_update_delete_and_index_queries_work() {
        let storage = setup();
        let id = "note-1";

        storage
            .create(id, br#"{"title":"a"}"#, "feature", "open")
            .unwrap();

        let created = storage.get(id).unwrap().unwrap();
        assert_eq!(created, br#"{"title":"a"}"#);

        let folder_items = storage.list_by_folder("feature").unwrap();
        assert_eq!(folder_items.len(), 1);
        let status_items = storage.list_by_status("open").unwrap();
        assert_eq!(status_items.len(), 1);

        storage
            .update(
                id,
                br#"{"title":"b"}"#,
                "feature",
                "open",
                "issue",
                "in_progress",
            )
            .unwrap();

        assert!(storage.list_by_folder("feature").unwrap().is_empty());
        assert!(storage.list_by_status("open").unwrap().is_empty());
        assert_eq!(storage.list_by_folder("issue").unwrap().len(), 1);
        assert_eq!(storage.list_by_status("in_progress").unwrap().len(), 1);

        let deleted = storage.delete(id, "issue", "in_progress").unwrap();
        assert!(deleted);
        assert!(storage.get(id).unwrap().is_none());
    }

    #[test]
    fn list_returns_all_items() {
        let storage = setup();
        storage
            .create("note-1", br#"{"title":"a"}"#, "feature", "open")
            .unwrap();
        storage
            .create("note-2", br#"{"title":"b"}"#, "issue", "done")
            .unwrap();

        let items = storage.list().unwrap();
        assert_eq!(items.len(), 2);
    }
}
