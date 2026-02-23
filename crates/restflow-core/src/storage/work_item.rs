//! Typed work item storage wrapper.

use crate::models::{ItemQuery, ItemStatus, WorkItem, WorkItemPatch, WorkItemSpec};
use anyhow::{Result, anyhow};
use redb::Database;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct WorkItemStorage {
    inner: restflow_storage::WorkItemStorage,
}

impl WorkItemStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Ok(Self {
            inner: restflow_storage::WorkItemStorage::new(db)?,
        })
    }

    pub fn create_note(&self, spec: WorkItemSpec) -> Result<WorkItem> {
        let folder = normalize_non_empty("folder", spec.folder)?;
        let title = normalize_non_empty("title", spec.title)?;
        let now = restflow_storage::time_utils::now_ms();

        let note = WorkItem {
            id: format!("item-{}", Uuid::new_v4()),
            folder,
            title,
            content: spec.content,
            priority: normalize_priority(spec.priority)?,
            status: ItemStatus::Open,
            tags: spec.tags,
            assignee: None,
            created_at: now,
            updated_at: now,
        };

        let data = serde_json::to_vec(&note)?;
        self.inner
            .create(&note.id, &data, &note.folder, status_label(note.status))?;

        Ok(note)
    }

    pub fn get_note(&self, id: &str) -> Result<Option<WorkItem>> {
        let Some(data) = self.inner.get(id)? else {
            return Ok(None);
        };
        Ok(Some(serde_json::from_slice(&data)?))
    }

    pub fn update_note(&self, id: &str, patch: WorkItemPatch) -> Result<WorkItem> {
        let mut note = self
            .get_note(id)?
            .ok_or_else(|| anyhow!("Work item {} not found", id))?;

        let old_folder = note.folder.clone();
        let old_status = note.status;

        if let Some(title) = patch.title {
            note.title = normalize_non_empty("title", title)?;
        }
        if let Some(content) = patch.content {
            note.content = content;
        }
        if let Some(priority) = patch.priority {
            note.priority = normalize_priority(Some(priority))?;
        }
        if let Some(status) = patch.status {
            note.status = status;
        }
        if let Some(tags) = patch.tags {
            note.tags = tags;
        }
        if let Some(assignee) = patch.assignee {
            note.assignee = Some(assignee);
        }
        if let Some(folder) = patch.folder {
            note.folder = normalize_non_empty("folder", folder)?;
        }

        note.updated_at = restflow_storage::time_utils::now_ms();

        let data = serde_json::to_vec(&note)?;
        self.inner.update(
            id,
            &data,
            &old_folder,
            status_label(old_status),
            &note.folder,
            status_label(note.status),
        )?;

        Ok(note)
    }

    pub fn delete_note(&self, id: &str) -> Result<()> {
        let note = self
            .get_note(id)?
            .ok_or_else(|| anyhow!("Work item {} not found", id))?;

        self.inner
            .delete(id, &note.folder, status_label(note.status))?;
        Ok(())
    }

    pub fn list_notes(&self, query: ItemQuery) -> Result<Vec<WorkItem>> {
        let mut notes = if let Some(folder) = query.folder.as_deref() {
            self.read_entries(self.inner.list_by_folder(folder)?)?
        } else if let Some(status) = query.status {
            self.read_entries(self.inner.list_by_status(status_label(status))?)?
        } else {
            self.read_entries(self.inner.list()?)?
        };

        if let Some(priority) = query.priority {
            let expected = priority.trim().to_ascii_lowercase();
            notes.retain(|note| {
                note.priority
                    .as_deref()
                    .map(|p| p.eq_ignore_ascii_case(&expected))
                    .unwrap_or(false)
            });
        }

        if let Some(tag) = query.tag {
            let expected = tag.trim().to_ascii_lowercase();
            notes.retain(|note| note.tags.iter().any(|t| t.to_ascii_lowercase() == expected));
        }

        if let Some(assignee) = query.assignee {
            let expected = assignee.trim().to_ascii_lowercase();
            notes.retain(|note| {
                note.assignee
                    .as_deref()
                    .map(|a| a.to_ascii_lowercase() == expected)
                    .unwrap_or(false)
            });
        }

        if let Some(search) = query.search {
            let keyword = search.to_ascii_lowercase();
            notes.retain(|note| {
                note.title.to_ascii_lowercase().contains(&keyword)
                    || note.content.to_ascii_lowercase().contains(&keyword)
            });
        }

        notes.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        notes.dedup_by(|a, b| a.id == b.id);
        Ok(notes)
    }

    pub fn list_folders(&self) -> Result<Vec<String>> {
        let mut folders: Vec<String> = self
            .read_entries(self.inner.list()?)?
            .into_iter()
            .map(|note| note.folder)
            .collect();

        folders.sort();
        folders.dedup();
        Ok(folders)
    }

    fn read_entries(&self, rows: Vec<(String, Vec<u8>)>) -> Result<Vec<WorkItem>> {
        rows.into_iter()
            .map(|(_, data)| serde_json::from_slice(&data).map_err(Into::into))
            .collect()
    }
}

fn normalize_non_empty(field: &str, value: String) -> Result<String> {
    let normalized = value.trim().to_string();
    if normalized.is_empty() {
        return Err(anyhow!("{} cannot be empty", field));
    }
    Ok(normalized)
}

fn normalize_priority(value: Option<String>) -> Result<Option<String>> {
    let Some(priority) = value else {
        return Ok(None);
    };
    let normalized = priority.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Ok(None);
    }
    match normalized.as_str() {
        "p0" | "p1" | "p2" | "p3" => Ok(Some(normalized)),
        _ => Err(anyhow!("Invalid priority: {}", priority)),
    }
}

fn status_label(status: ItemStatus) -> &'static str {
    match status {
        ItemStatus::Open => "open",
        ItemStatus::InProgress => "in_progress",
        ItemStatus::Done => "done",
        ItemStatus::Archived => "archived",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn setup() -> WorkItemStorage {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("workspace-note-wrapper.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        WorkItemStorage::new(db).unwrap()
    }

    #[test]
    fn create_and_get_note() {
        let storage = setup();
        let note = storage
            .create_note(WorkItemSpec {
                folder: "feature".to_string(),
                title: "Plan".to_string(),
                content: "- item".to_string(),
                priority: Some("p1".to_string()),
                tags: vec!["agent-b".to_string()],
            })
            .unwrap();

        let loaded = storage.get_note(&note.id).unwrap().unwrap();
        assert_eq!(loaded.title, "Plan");
        assert_eq!(loaded.priority.as_deref(), Some("p1"));
    }

    #[test]
    fn update_and_query_note() {
        let storage = setup();
        let note = storage
            .create_note(WorkItemSpec {
                folder: "feature".to_string(),
                title: "Plan".to_string(),
                content: "todo".to_string(),
                priority: Some("p2".to_string()),
                tags: vec!["x".to_string()],
            })
            .unwrap();

        let updated = storage
            .update_note(
                &note.id,
                WorkItemPatch {
                    status: Some(ItemStatus::InProgress),
                    assignee: Some("agent-b".to_string()),
                    priority: Some("p1".to_string()),
                    ..WorkItemPatch::default()
                },
            )
            .unwrap();

        assert_eq!(updated.status, ItemStatus::InProgress);

        let query = ItemQuery {
            status: Some(ItemStatus::InProgress),
            assignee: Some("agent-b".to_string()),
            ..ItemQuery::default()
        };

        let items = storage.list_notes(query).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, note.id);
    }

    #[test]
    fn list_folders_and_delete_note() {
        let storage = setup();
        let note = storage
            .create_note(WorkItemSpec {
                folder: "issue".to_string(),
                title: "Bug".to_string(),
                content: "desc".to_string(),
                priority: None,
                tags: Vec::new(),
            })
            .unwrap();

        let folders = storage.list_folders().unwrap();
        assert_eq!(folders, vec!["issue".to_string()]);

        storage.delete_note(&note.id).unwrap();
        assert!(storage.get_note(&note.id).unwrap().is_none());
    }
}
