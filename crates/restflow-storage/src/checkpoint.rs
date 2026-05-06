//! Process-local checkpoint storage for the local MVP.

use crate::simple_storage::namespace_for_db;
use anyhow::Result;
use redb::Database;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone)]
struct CheckpointRecord {
    execution_id: String,
    task_id: Option<String>,
    savepoint_id: Option<u64>,
    data: Vec<u8>,
}

#[derive(Default)]
struct CheckpointStore {
    next_savepoint_id: u64,
    records: HashMap<String, CheckpointRecord>,
}

fn stores() -> &'static Mutex<HashMap<usize, CheckpointStore>> {
    static STORES: OnceLock<Mutex<HashMap<usize, CheckpointStore>>> = OnceLock::new();
    STORES.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Clone)]
pub struct CheckpointStorage {
    namespace: usize,
}

impl CheckpointStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Ok(Self {
            namespace: namespace_for_db(&db),
        })
    }

    pub fn save(
        &self,
        id: &str,
        execution_id: &str,
        task_id: Option<&str>,
        data: &[u8],
    ) -> Result<()> {
        let mut stores = stores().lock().expect("checkpoint store lock poisoned");
        stores.entry(self.namespace).or_default().records.insert(
            id.to_string(),
            CheckpointRecord {
                execution_id: execution_id.to_string(),
                task_id: task_id.map(str::to_string),
                savepoint_id: None,
                data: data.to_vec(),
            },
        );
        Ok(())
    }

    pub fn save_with_savepoint(
        &self,
        id: &str,
        execution_id: &str,
        task_id: Option<&str>,
        data: &[u8],
    ) -> Result<u64> {
        let mut stores = stores().lock().expect("checkpoint store lock poisoned");
        let store = stores.entry(self.namespace).or_default();
        store.next_savepoint_id += 1;
        let savepoint_id = store.next_savepoint_id;
        store.records.insert(
            id.to_string(),
            CheckpointRecord {
                execution_id: execution_id.to_string(),
                task_id: task_id.map(str::to_string),
                savepoint_id: Some(savepoint_id),
                data: data.to_vec(),
            },
        );
        Ok(savepoint_id)
    }

    pub fn delete_savepoint(&self, savepoint_id: u64) -> Result<bool> {
        let mut stores = stores().lock().expect("checkpoint store lock poisoned");
        let Some(store) = stores.get_mut(&self.namespace) else {
            return Ok(false);
        };
        if let Some((id, _)) = store
            .records
            .iter()
            .find(|(_, record)| record.savepoint_id == Some(savepoint_id))
            .map(|(id, record)| (id.clone(), record.clone()))
        {
            store.records.remove(&id);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn load(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let stores = stores().lock().expect("checkpoint store lock poisoned");
        Ok(stores
            .get(&self.namespace)
            .and_then(|store| store.records.get(id).map(|record| record.data.clone())))
    }

    pub fn load_by_execution_id(&self, execution_id: &str) -> Result<Option<Vec<u8>>> {
        let stores = stores().lock().expect("checkpoint store lock poisoned");
        Ok(stores.get(&self.namespace).and_then(|store| {
            store
                .records
                .values()
                .filter(|record| record.execution_id == execution_id)
                .last()
                .map(|record| record.data.clone())
        }))
    }

    pub fn load_by_task_id(&self, task_id: &str) -> Result<Option<Vec<u8>>> {
        let stores = stores().lock().expect("checkpoint store lock poisoned");
        Ok(stores.get(&self.namespace).and_then(|store| {
            store
                .records
                .values()
                .filter(|record| record.task_id.as_deref() == Some(task_id))
                .last()
                .map(|record| record.data.clone())
        }))
    }

    pub fn delete(&self, id: &str, _execution_id: &str, _task_id: Option<&str>) -> Result<()> {
        let mut stores = stores().lock().expect("checkpoint store lock poisoned");
        if let Some(store) = stores.get_mut(&self.namespace) {
            store.records.remove(id);
        }
        Ok(())
    }

    pub fn cleanup_expired(&self, now_ms: i64) -> Result<usize> {
        let mut stores = stores().lock().expect("checkpoint store lock poisoned");
        let Some(store) = stores.get_mut(&self.namespace) else {
            return Ok(0);
        };
        let expired = store
            .records
            .iter()
            .filter_map(|(id, record)| {
                let expires_at = serde_json::from_slice::<serde_json::Value>(&record.data)
                    .ok()
                    .and_then(|value| value.get("expires_at_ms").and_then(|v| v.as_i64()));
                expires_at
                    .is_some_and(|value| value <= now_ms)
                    .then_some(id.clone())
            })
            .collect::<Vec<_>>();
        for id in &expired {
            store.records.remove(id);
        }
        Ok(expired.len())
    }
}
