//! Typed shared space storage wrapper.

use crate::models::{SharedEntry, Visibility};
use anyhow::{anyhow, Result};
use restflow_storage::SharedSpaceStorage as RawStorage;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub struct SharedSpaceStorage {
    inner: RawStorage,
}

impl SharedSpaceStorage {
    pub fn new(inner: RawStorage) -> Self {
        Self { inner }
    }

    fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }

    /// Create or update an entry
    pub fn set(&self, entry: &SharedEntry) -> Result<()> {
        let data = serde_json::to_vec(entry)?;
        self.inner.put_raw(&entry.key, &data)
    }

    /// Get an entry by key (with access control)
    pub fn get(&self, key: &str, accessor_id: Option<&str>) -> Result<Option<SharedEntry>> {
        let Some(data) = self.inner.get_raw(key)? else {
            return Ok(None);
        };
        let entry: SharedEntry = serde_json::from_slice(&data)?;
        if !entry.can_read(accessor_id) {
            return Err(anyhow!("Access denied: cannot read private entry"));
        }
        Ok(Some(entry))
    }

    /// Get raw entry without access control (for internal use)
    pub fn get_unchecked(&self, key: &str) -> Result<Option<SharedEntry>> {
        let Some(data) = self.inner.get_raw(key)? else {
            return Ok(None);
        };
        Ok(Some(serde_json::from_slice(&data)?))
    }

    /// Delete an entry (with access control)
    pub fn delete(&self, key: &str, accessor_id: Option<&str>) -> Result<bool> {
        if let Some(entry) = self.get_unchecked(key)? {
            if !entry.can_write(accessor_id) {
                return Err(anyhow!("Access denied: cannot delete this entry"));
            }
        }
        self.inner.delete(key)
    }

    /// List entries by namespace prefix (with access control)
    pub fn list(
        &self,
        namespace: Option<&str>,
        accessor_id: Option<&str>,
    ) -> Result<Vec<SharedEntry>> {
        let raw_entries = self.inner.list_raw(namespace)?;
        let mut entries = Vec::new();
        for (_, data) in raw_entries {
            if let Ok(entry) = serde_json::from_slice::<SharedEntry>(&data) {
                if entry.can_read(accessor_id) {
                    entries.push(entry);
                }
            }
        }
        entries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(entries)
    }

    /// List keys only (with access control)
    pub fn list_keys(
        &self,
        namespace: Option<&str>,
        accessor_id: Option<&str>,
    ) -> Result<Vec<String>> {
        let entries = self.list(namespace, accessor_id)?;
        Ok(entries.into_iter().map(|e| e.key).collect())
    }

    /// Convenience: quick set (public, no owner)
    pub fn quick_set(
        &self,
        key: &str,
        value: &str,
        modifier_id: Option<&str>,
    ) -> Result<()> {
        let now = Self::now_ms();
        let existing = self.get_unchecked(key)?;
        let entry = SharedEntry {
            key: key.to_string(),
            value: value.to_string(),
            visibility: existing
                .as_ref()
                .map(|e| e.visibility)
                .unwrap_or_default(),
            owner: existing.as_ref().and_then(|e| e.owner.clone()),
            content_type: None,
            type_hint: None,
            tags: existing
                .as_ref()
                .map(|e| e.tags.clone())
                .unwrap_or_default(),
            created_at: existing.as_ref().map(|e| e.created_at).unwrap_or(now),
            updated_at: now,
            last_modified_by: modifier_id.map(String::from),
        };
        self.set(&entry)
    }

    /// Convenience: quick get
    pub fn quick_get(&self, key: &str, accessor_id: Option<&str>) -> Result<Option<String>> {
        Ok(self.get(key, accessor_id)?.map(|e| e.value))
    }
}
