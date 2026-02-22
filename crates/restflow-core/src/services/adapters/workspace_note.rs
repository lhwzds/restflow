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
