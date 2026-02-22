//! SharedSpaceStore adapter backed by SharedSpaceStorage.

use crate::models::{SharedEntry, Visibility};
use crate::storage::SharedSpaceStorage;
use chrono::Utc;
use restflow_ai::tools::SharedSpaceStore;
use restflow_tools::ToolError;
use serde_json::{Value, json};

pub struct SharedSpaceStoreAdapter {
    storage: SharedSpaceStorage,
    accessor_id: Option<String>,
}

impl SharedSpaceStoreAdapter {
    pub fn new(storage: SharedSpaceStorage, accessor_id: Option<String>) -> Self {
        Self {
            storage,
            accessor_id,
        }
    }
}

impl SharedSpaceStore for SharedSpaceStoreAdapter {
    fn get_entry(&self, key: &str) -> restflow_tools::Result<Value> {
        let entry = self
            .storage
            .get(key, self.accessor_id.as_deref())
            .map_err(|e| ToolError::Tool(e.to_string()))?;
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
        let existing = self
            .storage
            .get_unchecked(key)
            .map_err(|e| ToolError::Tool(e.to_string()))?;

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

        self.storage
            .set(&entry)
            .map_err(|e| ToolError::Tool(e.to_string()))?;

        Ok(json!({
            "success": true,
            "key": key,
            "created": existing.is_none()
        }))
    }

    fn delete_entry(&self, key: &str, accessor_id: Option<&str>) -> restflow_tools::Result<Value> {
        let deleted = self
            .storage
            .delete(key, accessor_id)
            .map_err(|e| ToolError::Tool(e.to_string()))?;
        Ok(json!({
            "deleted": deleted,
            "key": key
        }))
    }

    fn list_entries(&self, namespace: Option<&str>) -> restflow_tools::Result<Value> {
        let prefix = namespace.map(|ns| format!("{}:", ns));
        let entries = self
            .storage
            .list(prefix.as_deref(), self.accessor_id.as_deref())
            .map_err(|e| ToolError::Tool(e.to_string()))?;
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
