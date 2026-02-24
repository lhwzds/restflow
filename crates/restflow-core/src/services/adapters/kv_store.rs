//! KvStore adapter backed by KvStoreStorage.

use crate::models::{SharedEntry, Visibility};
use crate::storage::KvStoreStorage;
use chrono::Utc;
use restflow_tools::ToolError;
use restflow_traits::store::KvStore;
use serde_json::{Value, json};

pub struct KvStoreAdapter {
    storage: KvStoreStorage,
    accessor_id: Option<String>,
}

impl KvStoreAdapter {
    pub fn new(storage: KvStoreStorage, accessor_id: Option<String>) -> Self {
        Self {
            storage,
            accessor_id,
        }
    }
}

impl KvStore for KvStoreAdapter {
    fn get_entry(&self, key: &str) -> restflow_tools::Result<Value> {
        let entry = self.storage.get(key, self.accessor_id.as_deref())?;
        let payload = match entry {
            Some(entry) => json!({
                "found": true,
                "key": entry.key,
                "value": entry.value,
                "content_type": entry.content_type,
                "visibility": entry.visibility,
                "tags": entry.tags,
                "updated_at": entry.updated_at
            }),
            None => json!({
                "found": false,
                "key": key
            }),
        };
        Ok(payload)
    }

    fn set_entry(
        &self,
        key: &str,
        content: &str,
        visibility: Option<&str>,
        content_type: Option<&str>,
        type_hint: Option<&str>,
        tags: Option<Vec<String>>,
        _accessor_id: Option<&str>,
    ) -> restflow_tools::Result<Value> {
        let existing = self.storage.get_unchecked(key)?;

        if let Some(ref entry) = existing
            && !entry.can_write(self.accessor_id.as_deref())
        {
            return Err(ToolError::Tool(
                "Access denied: cannot write to this entry".to_string(),
            ));
        }

        fn parse_visibility(value: &str) -> Visibility {
            match value {
                "private" => Visibility::Private,
                "shared" => Visibility::Shared,
                _ => Visibility::Public,
            }
        }

        let vis = visibility
            .map(parse_visibility)
            .or(existing.as_ref().map(|e| e.visibility))
            .unwrap_or_default();

        let entry = SharedEntry {
            key: key.to_string(),
            value: content.to_string(),
            visibility: vis,
            owner: existing
                .as_ref()
                .and_then(|e| e.owner.clone())
                .or_else(|| self.accessor_id.clone()),
            content_type: content_type
                .map(|s| s.to_string())
                .or_else(|| existing.as_ref().and_then(|e| e.content_type.clone())),
            type_hint: type_hint
                .map(|s| s.to_string())
                .or_else(|| existing.as_ref().and_then(|e| e.type_hint.clone())),
            tags: tags
                .or_else(|| existing.as_ref().map(|e| e.tags.clone()))
                .unwrap_or_default(),
            created_at: existing
                .as_ref()
                .map(|e| e.created_at)
                .unwrap_or_else(|| Utc::now().timestamp_millis()),
            updated_at: Utc::now().timestamp_millis(),
            last_modified_by: self.accessor_id.clone(),
        };

        self.storage.set(&entry)?;

        Ok(json!({
            "success": true,
            "key": key,
            "created": existing.is_none()
        }))
    }

    fn delete_entry(&self, key: &str, accessor_id: Option<&str>) -> restflow_tools::Result<Value> {
        let deleted = self.storage.delete(key, accessor_id)?;
        Ok(json!({
            "deleted": deleted,
            "key": key
        }))
    }

    fn list_entries(&self, namespace: Option<&str>) -> restflow_tools::Result<Value> {
        let prefix = namespace.map(|ns| format!("{}:", ns));
        let entries = self
            .storage
            .list(prefix.as_deref(), self.accessor_id.as_deref())?;
        let items: Vec<_> = entries
            .iter()
            .map(|entry| {
                let preview = if entry.value.len() > 100 {
                    format!("{}...", &entry.value[..100])
                } else {
                    entry.value.clone()
                };
                json!({
                    "key": entry.key,
                    "content_type": entry.content_type,
                    "visibility": entry.visibility,
                    "tags": entry.tags,
                    "updated_at": entry.updated_at,
                    "preview": preview
                })
            })
            .collect();
        Ok(json!({
            "count": items.len(),
            "entries": items
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_traits::store::KvStore;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn setup() -> (KvStoreAdapter, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(redb::Database::create(db_path).unwrap());
        let inner = restflow_storage::KvStoreStorage::new(db).unwrap();
        let storage = KvStoreStorage::new(inner);
        (
            KvStoreAdapter::new(storage, Some("test-agent".to_string())),
            temp_dir,
        )
    }

    #[test]
    fn test_set_and_get_entry() {
        let (adapter, _dir) = setup();
        adapter
            .set_entry("key1", "value1", None, None, None, None, None)
            .unwrap();

        let result = adapter.get_entry("key1").unwrap();
        assert_eq!(result["found"], true);
        assert_eq!(result["value"], "value1");
    }

    #[test]
    fn test_get_nonexistent_entry() {
        let (adapter, _dir) = setup();
        let result = adapter.get_entry("missing").unwrap();
        assert_eq!(result["found"], false);
    }

    #[test]
    fn test_delete_entry() {
        let (adapter, _dir) = setup();
        adapter
            .set_entry("del-key", "val", None, None, None, None, None)
            .unwrap();
        let result = adapter.delete_entry("del-key", None).unwrap();
        assert_eq!(result["deleted"], true);

        let after = adapter.get_entry("del-key").unwrap();
        assert_eq!(after["found"], false);
    }

    #[test]
    fn test_list_entries() {
        let (adapter, _dir) = setup();
        adapter
            .set_entry("a", "1", None, None, None, None, None)
            .unwrap();
        adapter
            .set_entry("b", "2", None, None, None, None, None)
            .unwrap();

        let result = adapter.list_entries(None).unwrap();
        assert_eq!(result["count"], 2);
    }

    #[test]
    fn test_update_existing_entry() {
        let (adapter, _dir) = setup();
        adapter
            .set_entry("upd", "old", None, None, None, None, None)
            .unwrap();
        adapter
            .set_entry("upd", "new", None, None, None, None, None)
            .unwrap();

        let result = adapter.get_entry("upd").unwrap();
        assert_eq!(result["value"], "new");
    }
}
