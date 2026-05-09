//! Task storage used by the local TUI MVP.
//!
//! Task persistence is intentionally no longer backed by redb tables. Runtime
//! task state is held in a process-local cache and can optionally be mirrored to
//! a JSON file so daemon-owned tasks survive CLI/TUI process boundaries.

use crate::storage::simple_storage::namespace_for_db;
use anyhow::{Context, Result};
use fs2::FileExt;
use redb::Database;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct TaskStore {
    tasks: BTreeMap<String, Vec<u8>>,
    task_status: HashMap<String, String>,
    runs: BTreeMap<String, Vec<u8>>,
    run_task: HashMap<String, String>,
    active_run: HashMap<String, String>,
    messages: BTreeMap<String, Vec<u8>>,
    message_task: HashMap<String, String>,
    message_status: HashMap<String, String>,
    events: BTreeMap<String, Vec<u8>>,
    event_task: HashMap<String, String>,
}

fn stores() -> &'static Mutex<HashMap<usize, TaskStore>> {
    static STORES: OnceLock<Mutex<HashMap<usize, TaskStore>>> = OnceLock::new();
    STORES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut tmp = path.to_path_buf();
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!("{value}.tmp"))
        .unwrap_or_else(|| "tmp".to_string());
    tmp.set_extension(extension);
    tmp
}

fn lock_path(path: &Path) -> PathBuf {
    let mut lock = path.to_path_buf();
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!("{value}.lock"))
        .unwrap_or_else(|| "lock".to_string());
    lock.set_extension(extension);
    lock
}

fn open_lock_file(path: &Path) -> Result<File> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("Failed to create task store directory {}", parent.display())
        })?;
    }
    let lock = lock_path(path);
    OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&lock)
        .with_context(|| format!("Failed to open task store lock {}", lock.display()))
}

#[cfg(unix)]
fn sync_parent_directory(path: &Path) -> Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    File::open(parent)
        .with_context(|| format!("Failed to open task store directory {}", parent.display()))?
        .sync_all()
        .with_context(|| format!("Failed to sync task store directory {}", parent.display()))
}

#[cfg(not(unix))]
fn sync_parent_directory(_path: &Path) -> Result<()> {
    Ok(())
}

/// Low-level process-local task storage with byte-level API.
#[derive(Clone)]
pub struct TaskStorage {
    namespace: usize,
    file_path: Option<PathBuf>,
}

#[allow(dead_code)]
impl TaskStorage {
    fn parse_field(data: &[u8], field: &str) -> Result<String> {
        let value: serde_json::Value = serde_json::from_slice(data)?;
        value
            .get(field)
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .ok_or_else(|| anyhow::anyhow!("{field} is missing or not a string"))
    }

    fn parse_task_status(data: &[u8]) -> Result<String> {
        Self::parse_field(data, "status")
    }

    fn parse_run_task_id(data: &[u8]) -> Result<String> {
        Self::parse_field(data, "task_id")
    }

    fn parse_run_status(data: &[u8]) -> Result<String> {
        Self::parse_field(data, "status")
    }

    fn validate_run_payload(run_id: &str, task_id: &str, status: &str, data: &[u8]) -> Result<()> {
        let payload_task_id = Self::parse_run_task_id(data)?;
        if payload_task_id != task_id {
            anyhow::bail!(
                "task run '{}' payload task_id '{}' does not match '{}'",
                run_id,
                payload_task_id,
                task_id
            );
        }
        let payload_status = Self::parse_run_status(data)?;
        if payload_status != status {
            anyhow::bail!(
                "task run '{}' payload status '{}' does not match '{}'",
                run_id,
                payload_status,
                status
            );
        }
        Ok(())
    }

    fn reconcile_active_run_slot(
        store: &mut TaskStore,
        task_id: &str,
        run_id: &str,
        status: &str,
    ) -> Result<()> {
        let wants_active = status == "running";
        if let Some(existing_run_id) = store.active_run.get(task_id).cloned() {
            if existing_run_id != run_id {
                if let Some(existing_raw) = store.runs.get(&existing_run_id) {
                    let existing_task_id = Self::parse_run_task_id(existing_raw)?;
                    let existing_status = Self::parse_run_status(existing_raw)?;
                    if existing_task_id == task_id && existing_status == "running" {
                        if wants_active {
                            anyhow::bail!(
                                "task '{}' already has active run '{}'",
                                task_id,
                                existing_run_id
                            );
                        }
                        return Ok(());
                    }
                }
                store.active_run.remove(task_id);
            } else if !wants_active {
                store.active_run.remove(task_id);
                return Ok(());
            }
        } else if !wants_active {
            return Ok(());
        }

        if wants_active {
            store
                .active_run
                .insert(task_id.to_string(), run_id.to_string());
        }
        Ok(())
    }

    /// Create a new TaskStorage instance.
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Ok(Self {
            namespace: namespace_for_db(&db),
            file_path: None,
        })
    }

    pub fn new_namespace(namespace: usize) -> Result<Self> {
        Ok(Self {
            namespace,
            file_path: None,
        })
    }

    /// Create a new TaskStorage instance backed by a JSON snapshot file.
    pub fn new_file_backed(db: Arc<Database>, file_path: impl Into<PathBuf>) -> Result<Self> {
        let storage = Self {
            namespace: namespace_for_db(&db),
            file_path: Some(file_path.into()),
        };
        storage.refresh_from_file()?;
        Ok(storage)
    }

    pub fn new_file_backed_namespace(
        namespace: usize,
        file_path: impl Into<PathBuf>,
    ) -> Result<Self> {
        let storage = Self {
            namespace,
            file_path: Some(file_path.into()),
        };
        storage.refresh_from_file()?;
        Ok(storage)
    }

    fn load_store_from_file(path: &Path) -> Result<Option<TaskStore>> {
        if !path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(path)
            .with_context(|| format!("Failed to read task store {}", path.display()))?;
        if bytes.is_empty() {
            return Ok(None);
        }
        let store: TaskStore = serde_json::from_slice(&bytes)
            .with_context(|| format!("Failed to parse task store {}", path.display()))?;
        Ok(Some(store))
    }

    fn refresh_from_file(&self) -> Result<()> {
        let Some(path) = self.file_path.as_ref() else {
            return Ok(());
        };
        self.with_file_lock(false, || self.refresh_from_file_unlocked(path))
    }

    fn refresh_from_file_unlocked(&self, path: &Path) -> Result<()> {
        let Some(store) = Self::load_store_from_file(path)? else {
            return Ok(());
        };
        stores()
            .lock()
            .expect("task store lock poisoned")
            .insert(self.namespace, store);
        Ok(())
    }

    fn with_file_lock<T>(
        &self,
        exclusive: bool,
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        // Keep this lock outside operations that touch the process-local store:
        // every file-backed path must acquire the file lock before the store mutex.
        let Some(path) = self.file_path.as_ref() else {
            return operation();
        };
        let lock = open_lock_file(path)?;
        if exclusive {
            lock.lock_exclusive()
        } else {
            lock.lock_shared()
        }
        .with_context(|| format!("Failed to lock task store {}", lock_path(path).display()))?;
        let result = operation();
        let unlock_result = lock
            .unlock()
            .with_context(|| format!("Failed to unlock task store {}", lock_path(path).display()));
        match (result, unlock_result) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
        }
    }

    fn persist_snapshot_to_file(path: &Path, store: &TaskStore) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create task store directory {}", parent.display())
            })?;
        }
        let bytes = serde_json::to_vec_pretty(store)
            .with_context(|| format!("Failed to encode task store {}", path.display()))?;
        let tmp_path = temporary_path(path);
        let mut tmp_file = File::create(&tmp_path)
            .with_context(|| format!("Failed to create task store {}", tmp_path.display()))?;
        tmp_file
            .write_all(&bytes)
            .with_context(|| format!("Failed to write task store {}", tmp_path.display()))?;
        tmp_file
            .sync_all()
            .with_context(|| format!("Failed to sync task store {}", tmp_path.display()))?;
        drop(tmp_file);
        std::fs::rename(&tmp_path, path)
            .with_context(|| format!("Failed to replace task store {}", path.display()))?;
        sync_parent_directory(path)?;
        Ok(())
    }

    fn persist_to_file(&self) -> Result<()> {
        let Some(path) = self.file_path.as_ref() else {
            return Ok(());
        };
        self.with_file_lock(true, || {
            let store = stores()
                .lock()
                .expect("task store lock poisoned")
                .get(&self.namespace)
                .cloned()
                .unwrap_or_default();
            Self::persist_snapshot_to_file(path, &store)
        })
    }

    fn read_store<T>(&self, f: impl FnOnce(Option<&TaskStore>) -> Result<T>) -> Result<T> {
        self.with_file_lock(false, || {
            if let Some(path) = self.file_path.as_ref() {
                self.refresh_from_file_unlocked(path)?;
            }
            let stores = stores().lock().expect("task store lock poisoned");
            f(stores.get(&self.namespace))
        })
    }

    fn mutate_store<T>(&self, f: impl FnOnce(&mut TaskStore) -> Result<T>) -> Result<T> {
        self.with_file_lock(true, || {
            let (result, snapshot) = {
                let mut stores = stores().lock().expect("task store lock poisoned");
                if let Some(path) = self.file_path.as_ref()
                    && let Some(store) = Self::load_store_from_file(path)?
                {
                    stores.insert(self.namespace, store);
                }
                let result = {
                    let store = stores.entry(self.namespace).or_default();
                    f(store)?
                };
                let snapshot = stores.get(&self.namespace).cloned().unwrap_or_default();
                (result, snapshot)
            };
            if let Some(path) = self.file_path.as_ref() {
                Self::persist_snapshot_to_file(path, &snapshot)?;
            }
            Ok(result)
        })
    }

    pub fn put_task_raw(&self, id: &str, data: &[u8]) -> Result<()> {
        self.mutate_store(|store| {
            store.tasks.insert(id.to_string(), data.to_vec());
            Ok(())
        })
    }

    pub fn put_task_raw_with_status(&self, id: &str, status: &str, data: &[u8]) -> Result<()> {
        self.mutate_store(|store| {
            store.tasks.insert(id.to_string(), data.to_vec());
            store.task_status.insert(id.to_string(), status.to_string());
            Ok(())
        })
    }

    pub fn update_task_raw_with_status(
        &self,
        id: &str,
        _old_status: &str,
        new_status: &str,
        data: &[u8],
    ) -> Result<()> {
        self.put_task_raw_with_status(id, new_status, data)
    }

    pub fn update_task_raw_if_status_matches(
        &self,
        id: &str,
        expected_status: &str,
        new_status: &str,
        data: &[u8],
    ) -> Result<bool> {
        self.mutate_store(|store| {
            let Some(existing) = store.tasks.get(id) else {
                return Ok(false);
            };
            let current_status = Self::parse_task_status(existing)?;
            if current_status != expected_status {
                return Ok(false);
            }
            store.tasks.insert(id.to_string(), data.to_vec());
            store
                .task_status
                .insert(id.to_string(), new_status.to_string());
            Ok(true)
        })
    }

    pub fn get_task_raw(&self, id: &str) -> Result<Option<Vec<u8>>> {
        self.read_store(|store| Ok(store.and_then(|store| store.tasks.get(id).cloned())))
    }

    pub fn list_tasks_raw(&self) -> Result<Vec<(String, Vec<u8>)>> {
        self.read_store(|store| {
            Ok(store
                .map(|store| {
                    store
                        .tasks
                        .iter()
                        .map(|(id, data)| (id.clone(), data.clone()))
                        .collect()
                })
                .unwrap_or_default())
        })
    }

    pub fn list_tasks_by_status_indexed(&self, status: &str) -> Result<Vec<(String, Vec<u8>)>> {
        self.read_store(|store| {
            let Some(store) = store else {
                return Ok(Vec::new());
            };
            let mut rows = Vec::new();
            for (id, data) in &store.tasks {
                let current = store
                    .task_status
                    .get(id)
                    .cloned()
                    .or_else(|| Self::parse_task_status(data).ok());
                if current.as_deref() == Some(status) {
                    rows.push((id.clone(), data.clone()));
                }
            }
            Ok(rows)
        })
    }

    pub fn delete_task(&self, id: &str) -> Result<bool> {
        self.mutate_store(|store| {
            store.task_status.remove(id);
            store.active_run.remove(id);
            Ok(store.tasks.remove(id).is_some())
        })
    }

    pub fn delete_task_with_status(&self, id: &str, _status: &str) -> Result<bool> {
        self.delete_task(id)
    }

    pub fn delete_task_cascade(&self, id: &str) -> Result<bool> {
        self.mutate_store(|store| {
            let existed = store.tasks.remove(id).is_some();
            store.task_status.remove(id);
            store.active_run.remove(id);
            let run_ids = store
                .run_task
                .iter()
                .filter_map(|(run_id, task_id)| (task_id == id).then_some(run_id.clone()))
                .collect::<Vec<_>>();
            for run_id in run_ids {
                store.runs.remove(&run_id);
                store.run_task.remove(&run_id);
            }
            let message_ids = store
                .message_task
                .iter()
                .filter_map(|(message_id, task_id)| (task_id == id).then_some(message_id.clone()))
                .collect::<Vec<_>>();
            for message_id in message_ids {
                store.messages.remove(&message_id);
                store.message_task.remove(&message_id);
                store.message_status.remove(&message_id);
            }
            let event_ids = store
                .event_task
                .iter()
                .filter_map(|(event_id, task_id)| (task_id == id).then_some(event_id.clone()))
                .collect::<Vec<_>>();
            for event_id in event_ids {
                store.events.remove(&event_id);
                store.event_task.remove(&event_id);
            }
            Ok(existed)
        })
    }

    pub fn put_run_raw(&self, run_id: &str, task_id: &str, data: &[u8]) -> Result<()> {
        let status = Self::parse_run_status(data)?;
        self.mutate_store(|store| {
            if !store.tasks.contains_key(task_id) {
                anyhow::bail!(
                    "task run '{}' references missing task '{}'",
                    run_id,
                    task_id
                );
            }
            store.runs.insert(run_id.to_string(), data.to_vec());
            store
                .run_task
                .insert(run_id.to_string(), task_id.to_string());
            if status == "running" {
                store
                    .active_run
                    .insert(task_id.to_string(), run_id.to_string());
            }
            Ok(())
        })
    }

    pub fn put_run_raw_with_status(
        &self,
        run_id: &str,
        task_id: &str,
        status: &str,
        data: &[u8],
    ) -> Result<()> {
        Self::validate_run_payload(run_id, task_id, status, data)?;
        self.mutate_store(|store| {
            if !store.tasks.contains_key(task_id) {
                anyhow::bail!(
                    "task run '{}' references missing task '{}'",
                    run_id,
                    task_id
                );
            }
            Self::reconcile_active_run_slot(store, task_id, run_id, status)?;
            store.runs.insert(run_id.to_string(), data.to_vec());
            store
                .run_task
                .insert(run_id.to_string(), task_id.to_string());
            Ok(())
        })
    }

    pub fn update_run_raw(&self, run_id: &str, task_id: &str, data: &[u8]) -> Result<()> {
        let status = Self::parse_run_status(data)?;
        self.update_run_raw_with_status(run_id, task_id, &status, &status, data)
    }

    pub fn update_run_raw_with_status(
        &self,
        run_id: &str,
        task_id: &str,
        _old_status: &str,
        new_status: &str,
        data: &[u8],
    ) -> Result<()> {
        Self::validate_run_payload(run_id, task_id, new_status, data)?;
        self.mutate_store(|store| {
            if !store.tasks.contains_key(task_id) {
                anyhow::bail!(
                    "task run '{}' references missing task '{}'",
                    run_id,
                    task_id
                );
            }
            Self::reconcile_active_run_slot(store, task_id, run_id, new_status)?;
            store.runs.insert(run_id.to_string(), data.to_vec());
            store
                .run_task
                .insert(run_id.to_string(), task_id.to_string());
            Ok(())
        })
    }

    pub fn get_run_raw(&self, run_id: &str) -> Result<Option<Vec<u8>>> {
        self.read_store(|store| Ok(store.and_then(|store| store.runs.get(run_id).cloned())))
    }

    pub fn list_runs_raw(&self) -> Result<Vec<(String, Vec<u8>)>> {
        self.read_store(|store| {
            Ok(store
                .map(|store| {
                    store
                        .runs
                        .iter()
                        .map(|(id, data)| (id.clone(), data.clone()))
                        .collect()
                })
                .unwrap_or_default())
        })
    }

    pub fn list_runs_by_task_raw(&self, task_id: &str) -> Result<Vec<(String, Vec<u8>)>> {
        self.read_store(|store| {
            let Some(store) = store else {
                return Ok(Vec::new());
            };
            Ok(store
                .runs
                .iter()
                .filter(|(run_id, _)| store.run_task.get(*run_id).is_some_and(|id| id == task_id))
                .map(|(id, data)| (id.clone(), data.clone()))
                .collect())
        })
    }

    pub fn get_active_run_raw(&self, task_id: &str) -> Result<Option<(String, Vec<u8>)>> {
        self.read_store(|store| {
            let Some(store) = store else {
                return Ok(None);
            };
            let Some(run_id) = store.active_run.get(task_id) else {
                return Ok(None);
            };
            Ok(store
                .runs
                .get(run_id)
                .map(|data| (run_id.clone(), data.clone())))
        })
    }

    pub fn clear_active_run_raw(&self, task_id: &str) -> Result<()> {
        self.mutate_store(|store| {
            store.active_run.remove(task_id);
            Ok(())
        })
    }

    pub fn list_active_runs_raw(&self) -> Result<Vec<(String, Vec<u8>)>> {
        self.read_store(|store| {
            let Some(store) = store else {
                return Ok(Vec::new());
            };
            Ok(store
                .active_run
                .values()
                .filter_map(|run_id| {
                    store
                        .runs
                        .get(run_id)
                        .map(|data| (run_id.clone(), data.clone()))
                })
                .collect())
        })
    }

    pub fn put_task_message_raw_with_status(
        &self,
        message_id: &str,
        task_id: &str,
        status: &str,
        data: &[u8],
    ) -> Result<()> {
        self.mutate_store(|store| {
            store.messages.insert(message_id.to_string(), data.to_vec());
            store
                .message_task
                .insert(message_id.to_string(), task_id.to_string());
            store
                .message_status
                .insert(message_id.to_string(), status.to_string());
            Ok(())
        })
    }

    pub fn update_task_message_raw_with_status(
        &self,
        message_id: &str,
        _task_id: &str,
        _old_status: &str,
        status: &str,
        data: &[u8],
    ) -> Result<()> {
        self.mutate_store(|store| {
            store.messages.insert(message_id.to_string(), data.to_vec());
            store
                .message_status
                .insert(message_id.to_string(), status.to_string());
            Ok(())
        })
    }

    pub fn get_task_message_raw(&self, message_id: &str) -> Result<Option<Vec<u8>>> {
        self.read_store(|store| Ok(store.and_then(|store| store.messages.get(message_id).cloned())))
    }

    pub fn list_task_messages_for_task_raw(&self, task_id: &str) -> Result<Vec<(String, Vec<u8>)>> {
        self.read_store(|store| {
            let Some(store) = store else {
                return Ok(Vec::new());
            };
            Ok(store
                .messages
                .iter()
                .filter(|(message_id, _)| {
                    store
                        .message_task
                        .get(*message_id)
                        .is_some_and(|id| id == task_id)
                })
                .map(|(id, data)| (id.clone(), data.clone()))
                .collect())
        })
    }

    pub fn list_task_messages_by_status_for_task_raw(
        &self,
        task_id: &str,
        status: &str,
    ) -> Result<Vec<(String, Vec<u8>)>> {
        self.read_store(|store| {
            let Some(store) = store else {
                return Ok(Vec::new());
            };
            Ok(store
                .messages
                .iter()
                .filter(|(message_id, _)| {
                    store
                        .message_task
                        .get(*message_id)
                        .is_some_and(|id| id == task_id)
                        && store
                            .message_status
                            .get(*message_id)
                            .is_some_and(|current| current == status)
                })
                .map(|(id, data)| (id.clone(), data.clone()))
                .collect())
        })
    }

    pub fn delete_task_message(
        &self,
        message_id: &str,
        _task_id: &str,
        _status: &str,
    ) -> Result<bool> {
        self.mutate_store(|store| {
            store.message_task.remove(message_id);
            store.message_status.remove(message_id);
            Ok(store.messages.remove(message_id).is_some())
        })
    }

    pub fn delete_task_messages_for_task(&self, task_id: &str) -> Result<u32> {
        self.mutate_store(|store| {
            let ids = store
                .message_task
                .iter()
                .filter_map(|(message_id, current_task_id)| {
                    (current_task_id == task_id).then_some(message_id.clone())
                })
                .collect::<Vec<_>>();
            for id in &ids {
                store.messages.remove(id);
                store.message_task.remove(id);
                store.message_status.remove(id);
            }
            Ok(ids.len() as u32)
        })
    }

    pub fn put_event_raw(&self, event_id: &str, task_id: &str, data: &[u8]) -> Result<()> {
        self.mutate_store(|store| {
            store.events.insert(event_id.to_string(), data.to_vec());
            store
                .event_task
                .insert(event_id.to_string(), task_id.to_string());
            Ok(())
        })
    }

    pub fn get_event_raw(&self, event_id: &str) -> Result<Option<Vec<u8>>> {
        self.read_store(|store| Ok(store.and_then(|store| store.events.get(event_id).cloned())))
    }

    pub fn list_events_for_task_raw(&self, task_id: &str) -> Result<Vec<(String, Vec<u8>)>> {
        self.read_store(|store| {
            let Some(store) = store else {
                return Ok(Vec::new());
            };
            Ok(store
                .events
                .iter()
                .filter(|(event_id, _)| {
                    store
                        .event_task
                        .get(*event_id)
                        .is_some_and(|id| id == task_id)
                })
                .map(|(id, data)| (id.clone(), data.clone()))
                .collect())
        })
    }

    pub fn delete_event(&self, event_id: &str, _task_id: &str) -> Result<bool> {
        self.mutate_store(|store| {
            store.event_task.remove(event_id);
            Ok(store.events.remove(event_id).is_some())
        })
    }

    pub fn delete_events_for_task(&self, task_id: &str) -> Result<u32> {
        self.mutate_store(|store| {
            let ids = store
                .event_task
                .iter()
                .filter_map(|(event_id, current_task_id)| {
                    (current_task_id == task_id).then_some(event_id.clone())
                })
                .collect::<Vec<_>>();
            for id in &ids {
                store.events.remove(id);
                store.event_task.remove(id);
            }
            Ok(ids.len() as u32)
        })
    }
}
