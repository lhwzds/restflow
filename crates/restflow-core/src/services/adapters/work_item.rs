//! WorkItemProvider adapter backed by WorkItemStorage.

use crate::models::{
    ItemQuery, ItemStatus, WorkItem,
    WorkItemPatch as CoreWorkItemPatch, WorkItemSpec as CoreWorkItemSpec,
};
use crate::storage::WorkItemStorage;
use restflow_traits::store::{
    WorkItemPatch, WorkItemProvider, WorkItemQuery,
    WorkItemRecord, WorkItemSpec, WorkItemStatus,
};

pub fn to_tool_item_status(status: ItemStatus) -> WorkItemStatus {
    match status {
        ItemStatus::Open => WorkItemStatus::Open,
        ItemStatus::InProgress => WorkItemStatus::InProgress,
        ItemStatus::Done => WorkItemStatus::Done,
        ItemStatus::Archived => WorkItemStatus::Archived,
    }
}

pub fn to_core_item_status(status: WorkItemStatus) -> ItemStatus {
    match status {
        WorkItemStatus::Open => ItemStatus::Open,
        WorkItemStatus::InProgress => ItemStatus::InProgress,
        WorkItemStatus::Done => ItemStatus::Done,
        WorkItemStatus::Archived => ItemStatus::Archived,
    }
}

pub fn to_tool_item(item: WorkItem) -> WorkItemRecord {
    WorkItemRecord {
        id: item.id,
        folder: item.folder,
        title: item.title,
        content: item.content,
        priority: item.priority,
        status: to_tool_item_status(item.status),
        tags: item.tags,
        assignee: item.assignee,
        created_at: item.created_at,
        updated_at: item.updated_at,
    }
}

#[derive(Clone)]
pub struct DbWorkItemAdapter {
    storage: WorkItemStorage,
}

impl DbWorkItemAdapter {
    pub fn new(storage: WorkItemStorage) -> Self {
        Self { storage }
    }
}

impl WorkItemProvider for DbWorkItemAdapter {
    fn create(&self, spec: WorkItemSpec) -> std::result::Result<WorkItemRecord, String> {
        self.storage
            .create_note(CoreWorkItemSpec {
                folder: spec.folder,
                title: spec.title,
                content: spec.content,
                priority: spec.priority,
                tags: spec.tags,
            })
            .map(to_tool_item)
            .map_err(|e| e.to_string())
    }

    fn get(&self, id: &str) -> std::result::Result<Option<WorkItemRecord>, String> {
        self.storage
            .get_note(id)
            .map(|item| item.map(to_tool_item))
            .map_err(|e| e.to_string())
    }

    fn update(
        &self,
        id: &str,
        patch: WorkItemPatch,
    ) -> std::result::Result<WorkItemRecord, String> {
        self.storage
            .update_note(
                id,
                CoreWorkItemPatch {
                    title: patch.title,
                    content: patch.content,
                    priority: patch.priority,
                    status: patch.status.map(to_core_item_status),
                    tags: patch.tags,
                    assignee: patch.assignee,
                    folder: patch.folder,
                },
            )
            .map(to_tool_item)
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
        query: WorkItemQuery,
    ) -> std::result::Result<Vec<WorkItemRecord>, String> {
        self.storage
            .list_notes(ItemQuery {
                folder: query.folder,
                status: query.status.map(to_core_item_status),
                priority: query.priority,
                tag: query.tag,
                assignee: query.assignee,
                search: query.search,
            })
            .map(|items| items.into_iter().map(to_tool_item).collect())
            .map_err(|e| e.to_string())
    }

    fn list_folders(&self) -> std::result::Result<Vec<String>, String> {
        self.storage.list_folders().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_traits::store::WorkItemProvider;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn setup() -> (DbWorkItemAdapter, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(redb::Database::create(db_path).unwrap());
        let storage = WorkItemStorage::new(db).unwrap();
        (DbWorkItemAdapter::new(storage), temp_dir)
    }

    #[test]
    fn test_create_and_get_item() {
        let (adapter, _dir) = setup();
        let spec = WorkItemSpec {
            folder: "inbox".to_string(),
            title: "Test Item".to_string(),
            content: "Hello world".to_string(),
            priority: None,
            tags: vec![],
        };
        let created = adapter.create(spec).unwrap();
        assert_eq!(created.title, "Test Item");

        let fetched = adapter.get(&created.id).unwrap().unwrap();
        assert_eq!(fetched.content, "Hello world");
    }

    #[test]
    fn test_update_item() {
        let (adapter, _dir) = setup();
        let spec = WorkItemSpec {
            folder: "inbox".to_string(),
            title: "Original".to_string(),
            content: "content".to_string(),
            priority: None,
            tags: vec![],
        };
        let created = adapter.create(spec).unwrap();

        let patch = WorkItemPatch {
            title: Some("Updated".to_string()),
            content: None,
            priority: None,
            status: Some(WorkItemStatus::InProgress),
            tags: None,
            assignee: None,
            folder: None,
        };
        let updated = adapter.update(&created.id, patch).unwrap();
        assert_eq!(updated.title, "Updated");
        assert_eq!(updated.status, WorkItemStatus::InProgress);
    }

    #[test]
    fn test_delete_item() {
        let (adapter, _dir) = setup();
        let spec = WorkItemSpec {
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
    fn test_list_items_with_query() {
        let (adapter, _dir) = setup();
        for i in 0..3 {
            let spec = WorkItemSpec {
                folder: "work".to_string(),
                title: format!("Item {}", i),
                content: "body".to_string(),
                priority: None,
                tags: vec![],
            };
            adapter.create(spec).unwrap();
        }

        let query = WorkItemQuery {
            folder: Some("work".to_string()),
            status: None,
            priority: None,
            tag: None,
            assignee: None,
            search: None,
        };
        let items = adapter.list(query).unwrap();
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn test_list_folders() {
        let (adapter, _dir) = setup();
        adapter.create(WorkItemSpec {
            folder: "inbox".to_string(),
            title: "A".to_string(),
            content: "x".to_string(),
            priority: None,
            tags: vec![],
        }).unwrap();
        adapter.create(WorkItemSpec {
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
            WorkItemStatus::Open,
            WorkItemStatus::InProgress,
            WorkItemStatus::Done,
            WorkItemStatus::Archived,
        ] {
            let core = to_core_item_status(status.clone());
            let back = to_tool_item_status(core);
            assert_eq!(back, status);
        }
    }
}
