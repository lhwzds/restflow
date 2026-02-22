//! WorkspaceNoteProvider adapter backed by WorkspaceNoteStorage.

use crate::models::{
    NoteQuery, NoteStatus, WorkspaceNote,
    WorkspaceNotePatch as CoreWorkspaceNotePatch, WorkspaceNoteSpec as CoreWorkspaceNoteSpec,
};
use crate::storage::WorkspaceNoteStorage;
use restflow_ai::tools::{
    WorkspaceNotePatch, WorkspaceNoteProvider, WorkspaceNoteQuery,
    WorkspaceNoteRecord, WorkspaceNoteSpec, WorkspaceNoteStatus,
};

pub fn to_tool_note_status(status: NoteStatus) -> WorkspaceNoteStatus {
    match status {
        NoteStatus::Open => WorkspaceNoteStatus::Open,
        NoteStatus::InProgress => WorkspaceNoteStatus::InProgress,
        NoteStatus::Done => WorkspaceNoteStatus::Done,
        NoteStatus::Archived => WorkspaceNoteStatus::Archived,
    }
}

pub fn to_core_note_status(status: WorkspaceNoteStatus) -> NoteStatus {
    match status {
        WorkspaceNoteStatus::Open => NoteStatus::Open,
        WorkspaceNoteStatus::InProgress => NoteStatus::InProgress,
        WorkspaceNoteStatus::Done => NoteStatus::Done,
        WorkspaceNoteStatus::Archived => NoteStatus::Archived,
    }
}

pub fn to_tool_note(note: WorkspaceNote) -> WorkspaceNoteRecord {
    WorkspaceNoteRecord {
        id: note.id,
        folder: note.folder,
        title: note.title,
        content: note.content,
        priority: note.priority,
        status: to_tool_note_status(note.status),
        tags: note.tags,
        assignee: note.assignee,
        created_at: note.created_at,
        updated_at: note.updated_at,
    }
}

#[derive(Clone)]
pub struct DbWorkspaceNoteAdapter {
    storage: WorkspaceNoteStorage,
}

impl DbWorkspaceNoteAdapter {
    pub fn new(storage: WorkspaceNoteStorage) -> Self {
        Self { storage }
    }
}

impl WorkspaceNoteProvider for DbWorkspaceNoteAdapter {
    fn create(&self, spec: WorkspaceNoteSpec) -> std::result::Result<WorkspaceNoteRecord, String> {
        self.storage
            .create_note(CoreWorkspaceNoteSpec {
                folder: spec.folder,
                title: spec.title,
                content: spec.content,
                priority: spec.priority,
                tags: spec.tags,
            })
            .map(to_tool_note)
            .map_err(|e| e.to_string())
    }

    fn get(&self, id: &str) -> std::result::Result<Option<WorkspaceNoteRecord>, String> {
        self.storage
            .get_note(id)
            .map(|note| note.map(to_tool_note))
            .map_err(|e| e.to_string())
    }

    fn update(
        &self,
        id: &str,
        patch: WorkspaceNotePatch,
    ) -> std::result::Result<WorkspaceNoteRecord, String> {
        self.storage
            .update_note(
                id,
                CoreWorkspaceNotePatch {
                    title: patch.title,
                    content: patch.content,
                    priority: patch.priority,
                    status: patch.status.map(to_core_note_status),
                    tags: patch.tags,
                    assignee: patch.assignee,
                    folder: patch.folder,
                },
            )
            .map(to_tool_note)
            .map_err(|e| e.to_string())
    }

    fn delete(&self, id: &str) -> std::result::Result<bool, String> {
        match self.storage.get_note(id) {
            Ok(None) => Ok(false),
            Ok(Some(_)) => self
                .storage
                .delete_note(id)
                .map(|_| true)
                .map_err(|e| e.to_string()),
            Err(err) => Err(err.to_string()),
        }
    }

    fn list(
        &self,
        query: WorkspaceNoteQuery,
    ) -> std::result::Result<Vec<WorkspaceNoteRecord>, String> {
        self.storage
            .list_notes(NoteQuery {
                folder: query.folder,
                status: query.status.map(to_core_note_status),
                priority: query.priority,
                tag: query.tag,
                assignee: query.assignee,
                search: query.search,
            })
            .map(|notes| notes.into_iter().map(to_tool_note).collect())
            .map_err(|e| e.to_string())
    }

    fn list_folders(&self) -> std::result::Result<Vec<String>, String> {
        self.storage.list_folders().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_ai::tools::WorkspaceNoteProvider;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn setup() -> (DbWorkspaceNoteAdapter, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(redb::Database::create(db_path).unwrap());
        let storage = WorkspaceNoteStorage::new(db).unwrap();
        (DbWorkspaceNoteAdapter::new(storage), temp_dir)
    }

    #[test]
    fn test_create_and_get_note() {
        let (adapter, _dir) = setup();
        let spec = WorkspaceNoteSpec {
            folder: "inbox".to_string(),
            title: "Test Note".to_string(),
            content: "Hello world".to_string(),
            priority: None,
            tags: vec![],
        };
        let created = adapter.create(spec).unwrap();
        assert_eq!(created.title, "Test Note");

        let fetched = adapter.get(&created.id).unwrap().unwrap();
        assert_eq!(fetched.content, "Hello world");
    }

    #[test]
    fn test_update_note() {
        let (adapter, _dir) = setup();
        let spec = WorkspaceNoteSpec {
            folder: "inbox".to_string(),
            title: "Original".to_string(),
            content: "content".to_string(),
            priority: None,
            tags: vec![],
        };
        let created = adapter.create(spec).unwrap();

        let patch = WorkspaceNotePatch {
            title: Some("Updated".to_string()),
            content: None,
            priority: None,
            status: Some(WorkspaceNoteStatus::InProgress),
            tags: None,
            assignee: None,
            folder: None,
        };
        let updated = adapter.update(&created.id, patch).unwrap();
        assert_eq!(updated.title, "Updated");
        assert_eq!(updated.status, WorkspaceNoteStatus::InProgress);
    }

    #[test]
    fn test_delete_note() {
        let (adapter, _dir) = setup();
        let spec = WorkspaceNoteSpec {
            folder: "inbox".to_string(),
            title: "Delete Me".to_string(),
            content: "bye".to_string(),
            priority: None,
            tags: vec![],
        };
        let created = adapter.create(spec).unwrap();
        assert!(adapter.delete(&created.id).unwrap());
        assert!(!adapter.delete(&created.id).unwrap());
    }

    #[test]
    fn test_list_notes_with_query() {
        let (adapter, _dir) = setup();
        for i in 0..3 {
            let spec = WorkspaceNoteSpec {
                folder: "work".to_string(),
                title: format!("Note {}", i),
                content: "body".to_string(),
                priority: None,
                tags: vec![],
            };
            adapter.create(spec).unwrap();
        }

        let query = WorkspaceNoteQuery {
            folder: Some("work".to_string()),
            status: None,
            priority: None,
            tag: None,
            assignee: None,
            search: None,
        };
        let notes = adapter.list(query).unwrap();
        assert_eq!(notes.len(), 3);
    }

    #[test]
    fn test_list_folders() {
        let (adapter, _dir) = setup();
        adapter.create(WorkspaceNoteSpec {
            folder: "inbox".to_string(),
            title: "A".to_string(),
            content: "x".to_string(),
            priority: None,
            tags: vec![],
        }).unwrap();
        adapter.create(WorkspaceNoteSpec {
            folder: "work".to_string(),
            title: "B".to_string(),
            content: "x".to_string(),
            priority: None,
            tags: vec![],
        }).unwrap();

        let folders = adapter.list_folders().unwrap();
        assert!(folders.contains(&"inbox".to_string()));
        assert!(folders.contains(&"work".to_string()));
    }

    #[test]
    fn test_status_conversion_roundtrip() {
        for status in [
            WorkspaceNoteStatus::Open,
            WorkspaceNoteStatus::InProgress,
            WorkspaceNoteStatus::Done,
            WorkspaceNoteStatus::Archived,
        ] {
            let core = to_core_note_status(status.clone());
            let back = to_tool_note_status(core);
            assert_eq!(back, status);
        }
    }
}
