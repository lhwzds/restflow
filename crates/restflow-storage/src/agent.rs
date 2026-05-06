//! Agent storage - byte-level API for agent persistence.

use crate::SimpleStorage;
use crate::define_simple_storage;

define_simple_storage! {
    /// Low-level agent storage with byte-level API
    pub struct AgentStorage { store: "agent" }
}

impl AgentStorage {
    /// Delete an agent atomically - returns (existed, resolved_id).
    pub fn delete_atomically(&self, id_or_prefix: &str) -> anyhow::Result<(bool, Option<String>)> {
        let id = id_or_prefix.trim();
        if id.is_empty() {
            anyhow::bail!("Agent ID is empty");
        }

        if self.delete(id)? {
            return Ok((true, Some(id.to_string())));
        }

        let matches = self
            .list_raw()?
            .into_iter()
            .map(|(key, _)| key)
            .filter(|key| key.starts_with(id))
            .collect::<Vec<_>>();

        match matches.len() {
            0 => Ok((false, None)),
            1 => {
                let resolved = matches.into_iter().next().unwrap();
                self.delete(&resolved)?;
                Ok((true, Some(resolved)))
            }
            _ => {
                let preview = matches
                    .iter()
                    .take(5)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ");
                anyhow::bail!(
                    "Agent ID prefix '{}' is ambiguous ({} matches: {})",
                    id,
                    matches.len(),
                    preview
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::Database;
    use std::sync::Arc;
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

    #[test]
    fn test_delete_atomically_exact_id() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = AgentStorage::new(db).unwrap();

        storage.put_raw("agent-001", b"data").unwrap();

        let (existed, resolved) = storage.delete_atomically("agent-001").unwrap();
        assert!(existed);
        assert_eq!(resolved, Some("agent-001".to_string()));

        let retrieved = storage.get_raw("agent-001").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_delete_atomically_prefix() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = AgentStorage::new(db).unwrap();

        storage.put_raw("abc123xyz", b"data").unwrap();

        let (existed, resolved) = storage.delete_atomically("abc123").unwrap();
        assert!(existed);
        assert_eq!(resolved, Some("abc123xyz".to_string()));

        let retrieved = storage.get_raw("abc123xyz").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_delete_atomically_not_found() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = AgentStorage::new(db).unwrap();

        let (existed, resolved) = storage.delete_atomically("nonexistent").unwrap();
        assert!(!existed);
        assert!(resolved.is_none());
    }

    #[test]
    fn test_delete_atomically_ambiguous_prefix() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = AgentStorage::new(db).unwrap();

        storage.put_raw("agent-001", b"data1").unwrap();
        storage.put_raw("agent-002", b"data2").unwrap();

        let result = storage.delete_atomically("agent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("ambiguous"));
    }

    /// Test concurrent delete operations don't cause race conditions.
    #[test]
    fn test_concurrent_delete_atomically() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::thread;

        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = Arc::new(AgentStorage::new(db).unwrap());

        storage.put_raw("race-agent", b"data").unwrap();

        let success_count = Arc::new(AtomicUsize::new(0));
        let num_threads = 10;

        let handles: Vec<_> = (0..num_threads)
            .map(|_| {
                let s = Arc::clone(&storage);
                let count = Arc::clone(&success_count);
                thread::spawn(move || {
                    let (existed, _) = s.delete_atomically("race-agent").unwrap();
                    if existed {
                        count.fetch_add(1, Ordering::SeqCst);
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // Exactly one delete should have succeeded
        assert_eq!(success_count.load(Ordering::SeqCst), 1);

        // Agent should no longer exist
        let retrieved = storage.get_raw("race-agent").unwrap();
        assert!(retrieved.is_none());
    }
}
