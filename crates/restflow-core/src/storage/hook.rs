//! Typed hook storage wrapper.

use crate::models::Hook;
use anyhow::Result;
use redb::Database;
use restflow_storage::SimpleStorage;
use std::sync::Arc;

restflow_storage::define_simple_storage! {
    /// Raw hook storage table.
    pub struct RawHookStorage { table: "hooks" }
}

/// Typed hook storage wrapper around raw key-value storage.
#[derive(Debug, Clone)]
pub struct HookStorage {
    inner: RawHookStorage,
}

impl HookStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Ok(Self {
            inner: RawHookStorage::new(db)?,
        })
    }

    /// Create a new hook (fails if hook id already exists).
    pub fn create(&self, hook: &Hook) -> Result<()> {
        if self.inner.exists(&hook.id)? {
            anyhow::bail!("Hook {} already exists", hook.id);
        }
        let json = serde_json::to_vec(hook)?;
        self.inner.put_raw(&hook.id, &json)
    }

    /// Get a hook by id.
    pub fn get(&self, id: &str) -> Result<Option<Hook>> {
        let Some(bytes) = self.inner.get_raw(id)? else {
            return Ok(None);
        };

        Ok(Some(serde_json::from_slice(&bytes)?))
    }

    /// List all hooks sorted by updated time descending.
    pub fn list(&self) -> Result<Vec<Hook>> {
        let mut hooks = Vec::new();
        for (_, bytes) in self.inner.list_raw()? {
            hooks.push(serde_json::from_slice::<Hook>(&bytes)?);
        }

        hooks.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(hooks)
    }

    /// Update an existing hook.
    pub fn update(&self, id: &str, hook: &Hook) -> Result<()> {
        if !self.inner.exists(id)? {
            anyhow::bail!("Hook {} not found", id);
        }

        let json = serde_json::to_vec(hook)?;
        self.inner.put_raw(id, &json)
    }

    /// Delete a hook by id.
    pub fn delete(&self, id: &str) -> Result<bool> {
        self.inner.delete(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{HookAction, HookEvent};
    use tempfile::tempdir;

    fn setup() -> (HookStorage, tempfile::TempDir) {
        let temp_dir = tempdir().expect("create temp dir");
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).expect("create db"));
        let storage = HookStorage::new(db).expect("create storage");
        (storage, temp_dir)
    }

    fn build_hook(id: &str) -> Hook {
        let mut hook = Hook::new(
            "Hook".to_string(),
            HookEvent::TaskCompleted,
            HookAction::Webhook {
                url: "https://example.com/hook".to_string(),
                method: None,
                headers: None,
            },
        );
        hook.id = id.to_string();
        hook
    }

    #[test]
    fn test_create_get_update_delete() {
        let (storage, _temp_dir) = setup();
        let mut hook = build_hook("hook-1");

        storage.create(&hook).expect("create hook");

        let stored = storage.get("hook-1").expect("get hook").expect("exists");
        assert_eq!(stored.id, "hook-1");

        hook.name = "Updated Hook".to_string();
        hook.touch();
        storage.update("hook-1", &hook).expect("update hook");

        let updated = storage.get("hook-1").expect("get hook").expect("exists");
        assert_eq!(updated.name, "Updated Hook");

        assert!(storage.delete("hook-1").expect("delete hook"));
        assert!(storage.get("hook-1").expect("get hook").is_none());
    }

    #[test]
    fn test_list_hooks() {
        let (storage, _temp_dir) = setup();

        let hook_a = build_hook("hook-a");
        let mut hook_b = build_hook("hook-b");
        hook_b.updated_at += 10;

        storage.create(&hook_a).expect("create hook a");
        storage.create(&hook_b).expect("create hook b");

        let hooks = storage.list().expect("list hooks");
        assert_eq!(hooks.len(), 2);
        assert_eq!(hooks[0].id, "hook-b");
    }
}
