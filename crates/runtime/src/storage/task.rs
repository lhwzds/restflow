//! Typed agent task storage wrapper.
//!
//! Provides type-safe access to agent task storage by wrapping the byte-level
//! process-local byte APIs with Rust types from our models.

#[cfg(any(test, feature = "test-utils"))]
use crate::models::TaskSchedule;
use crate::models::{
    Task, TaskControlAction, TaskEvent, TaskEventType, TaskMessage, TaskMessageSource,
    TaskMessageStatus, TaskPatch, TaskProgress, TaskSpec, TaskStatus,
};
use anyhow::Result;
use redb::Database;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::warn;
use uuid::Uuid;

use raw::TaskStorage as RawTaskStorage;

/// Typed agent task storage wrapper around process-local task bytes.
#[derive(Clone)]
pub struct TaskStorage {
    inner: RawTaskStorage,
}

#[derive(Debug, Clone)]
pub struct TaskSessionBinding {
    pub session_id: String,
    pub owns_session: bool,
}

impl TaskStorage {
    const MIN_TASK_TIMEOUT_SECS: u64 = 10;

    fn validate_task_session_binding(session_binding: &TaskSessionBinding) -> Result<()> {
        if session_binding.session_id.trim().is_empty() {
            anyhow::bail!("task must be bound to a chat session");
        }
        Ok(())
    }

    fn validate_task_has_session(task: &Task) -> Result<()> {
        if task.chat_session_id.trim().is_empty() {
            anyhow::bail!("task '{}' must be bound to a chat session", task.id);
        }
        Ok(())
    }

    fn has_non_empty_text(value: Option<&str>) -> bool {
        value.is_some_and(|text| !text.trim().is_empty())
    }

    fn normalize_optional_id(value: Option<String>) -> Option<String> {
        value.and_then(|id| {
            let trimmed = id.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
    }

    fn validate_timeout_secs(timeout_secs: Option<u64>) -> Result<()> {
        if let Some(timeout) = timeout_secs
            && timeout < Self::MIN_TASK_TIMEOUT_SECS
        {
            return Err(anyhow::anyhow!(
                "timeout_secs must be at least {} seconds",
                Self::MIN_TASK_TIMEOUT_SECS
            ));
        }
        Ok(())
    }

    fn validate_task_input(input: Option<&str>, input_template: Option<&str>) -> Result<()> {
        if Self::resolve_effective_input_for_validation(input, input_template).is_some() {
            return Ok(());
        }
        Err(anyhow::anyhow!(
            "task requires non-empty input or input_template"
        ))
    }

    fn resolve_effective_input_for_validation(
        input: Option<&str>,
        input_template: Option<&str>,
    ) -> Option<String> {
        let fallback_input = input
            .filter(|value| Self::has_non_empty_text(Some(value)))
            .map(str::to_string);

        if let Some(template) = input_template {
            let rendered = Self::render_input_template_for_validation(template, input);
            if !rendered.trim().is_empty() {
                return Some(rendered);
            }
            return fallback_input;
        }

        fallback_input
    }

    fn render_input_template_for_validation(template: &str, input: Option<&str>) -> String {
        let input_value = input.unwrap_or_default();
        let replacements = std::collections::HashMap::from([("{{task.input}}", input_value)]);
        crate::template::render_template_single_pass(template, &replacements)
    }

    /// Create a new TaskStorage instance
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Ok(Self {
            inner: RawTaskStorage::new(db.clone())?,
        })
    }

    /// Create a new TaskStorage instance backed by a JSON snapshot file.
    pub fn new_file_backed(db: Arc<Database>, file_path: impl Into<PathBuf>) -> Result<Self> {
        Ok(Self {
            inner: RawTaskStorage::new_file_backed(db.clone(), file_path)?,
        })
    }

    pub fn new_file_backed_path(file_path: impl Into<PathBuf>) -> Result<Self> {
        Ok(Self {
            inner: RawTaskStorage::new_file_backed_path(file_path)?,
        })
    }

    fn event_stage_label(event_type: &TaskEventType) -> String {
        match event_type {
            TaskEventType::Created => "created",
            TaskEventType::Started => "running",
            TaskEventType::Completed => "completed",
            TaskEventType::Failed => "failed",
            TaskEventType::Paused => "paused",
            TaskEventType::Resumed => "active",
            TaskEventType::Compaction => "compaction",
            TaskEventType::Interrupted => "interrupted",
        }
        .to_string()
    }
}

pub(crate) mod raw {
    //! Task storage used by the local TUI MVP.
    //!
    //! Task persistence is intentionally no longer backed by redb tables. Runtime
    //! task state is held in a process-local cache and can optionally be mirrored to
    //! a JSON file so daemon-owned tasks survive CLI/TUI process boundaries.

    use anyhow::{Context, Result};
    use fs2::FileExt;
    use redb::Database;
    use serde::{Deserialize, Serialize};
    use std::collections::{BTreeMap, HashMap};
    use std::fs::{File, OpenOptions};
    use std::io::ErrorKind;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::time::SystemTime;

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

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct TaskStoreFingerprint {
        len: u64,
        modified: Option<SystemTime>,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct TaskStoreCacheMarker {
        fingerprint: Option<TaskStoreFingerprint>,
        has_store: bool,
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
        store: Arc<Mutex<Option<TaskStore>>>,
        cache_marker: Arc<Mutex<Option<TaskStoreCacheMarker>>>,
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

        fn validate_run_payload(
            run_id: &str,
            task_id: &str,
            status: &str,
            data: &[u8],
        ) -> Result<()> {
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
            let _ = db;
            Ok(Self {
                store: Arc::new(Mutex::new(None)),
                cache_marker: Arc::new(Mutex::new(None)),
                file_path: None,
            })
        }

        /// Create a new TaskStorage instance backed by a JSON snapshot file.
        pub fn new_file_backed(db: Arc<Database>, file_path: impl Into<PathBuf>) -> Result<Self> {
            let _ = db;
            let storage = Self {
                store: Arc::new(Mutex::new(None)),
                cache_marker: Arc::new(Mutex::new(None)),
                file_path: Some(file_path.into()),
            };
            storage.refresh_from_file()?;
            Ok(storage)
        }

        pub fn new_file_backed_path(file_path: impl Into<PathBuf>) -> Result<Self> {
            let storage = Self {
                store: Arc::new(Mutex::new(None)),
                cache_marker: Arc::new(Mutex::new(None)),
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

        fn file_fingerprint(path: &Path) -> Result<Option<TaskStoreFingerprint>> {
            match std::fs::metadata(path) {
                Ok(metadata) => Ok(Some(TaskStoreFingerprint {
                    len: metadata.len(),
                    modified: metadata.modified().ok(),
                })),
                Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
                Err(error) => Err(error)
                    .with_context(|| format!("Failed to stat task store {}", path.display())),
            }
        }

        fn is_file_cache_current(&self, fingerprint: Option<TaskStoreFingerprint>) -> bool {
            let store_present = self
                .store
                .lock()
                .expect("task store lock poisoned")
                .is_some();
            let expected = TaskStoreCacheMarker {
                fingerprint,
                has_store: store_present,
            };
            *self
                .cache_marker
                .lock()
                .expect("task store cache marker lock poisoned")
                == Some(expected)
        }

        fn record_file_cache_marker(
            &self,
            fingerprint: Option<TaskStoreFingerprint>,
            has_store: bool,
        ) {
            *self
                .cache_marker
                .lock()
                .expect("task store cache marker lock poisoned") = Some(TaskStoreCacheMarker {
                fingerprint,
                has_store,
            });
        }

        fn record_current_file_cache_marker(&self, path: &Path, has_store: bool) -> Result<()> {
            let fingerprint = Self::file_fingerprint(path)?;
            self.record_file_cache_marker(fingerprint, has_store);
            Ok(())
        }

        fn refresh_from_file(&self) -> Result<()> {
            let Some(path) = self.file_path.as_ref() else {
                return Ok(());
            };
            self.with_file_lock(false, || self.refresh_from_file_unlocked(path))
        }

        fn refresh_from_file_unlocked(&self, path: &Path) -> Result<()> {
            let fingerprint = Self::file_fingerprint(path)?;
            if self.is_file_cache_current(fingerprint) {
                return Ok(());
            }
            let Some(store) = Self::load_store_from_file(path)? else {
                *self.store.lock().expect("task store lock poisoned") = None;
                self.record_file_cache_marker(fingerprint, false);
                return Ok(());
            };
            *self.store.lock().expect("task store lock poisoned") = Some(store);
            self.record_file_cache_marker(fingerprint, true);
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
            let unlock_result = lock.unlock().with_context(|| {
                format!("Failed to unlock task store {}", lock_path(path).display())
            });
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
                let store = self
                    .store
                    .lock()
                    .expect("task store lock poisoned")
                    .clone()
                    .unwrap_or_default();
                Self::persist_snapshot_to_file(path, &store)
                    .and_then(|()| self.record_current_file_cache_marker(path, true))
            })
        }

        fn read_store<T>(&self, f: impl FnOnce(Option<&TaskStore>) -> Result<T>) -> Result<T> {
            self.with_file_lock(false, || {
                if let Some(path) = self.file_path.as_ref() {
                    self.refresh_from_file_unlocked(path)?;
                }
                let store = self.store.lock().expect("task store lock poisoned");
                f(store.as_ref())
            })
        }

        fn mutate_store<T>(&self, f: impl FnOnce(&mut TaskStore) -> Result<T>) -> Result<T> {
            self.with_file_lock(true, || {
                let (result, snapshot) = {
                    if let Some(path) = self.file_path.as_ref() {
                        self.refresh_from_file_unlocked(path)?;
                    }
                    let mut slot = self.store.lock().expect("task store lock poisoned");
                    let store = slot.get_or_insert_with(TaskStore::default);
                    let result = { f(store)? };
                    let snapshot = store.clone();
                    (result, snapshot)
                };
                if let Some(path) = self.file_path.as_ref() {
                    Self::persist_snapshot_to_file(path, &snapshot)?;
                    self.record_current_file_cache_marker(path, true)?;
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
                    .filter_map(|(message_id, task_id)| {
                        (task_id == id).then_some(message_id.clone())
                    })
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
                    .filter(|(run_id, _)| {
                        store.run_task.get(*run_id).is_some_and(|id| id == task_id)
                    })
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

        pub fn set_active_run_raw(&self, task_id: &str, run_id: &str) -> Result<()> {
            self.mutate_store(|store| {
                if !store.tasks.contains_key(task_id) {
                    anyhow::bail!("active run references missing task '{}'", task_id);
                }
                let Some(raw) = store.runs.get(run_id) else {
                    anyhow::bail!("active run references missing run '{}'", run_id);
                };
                let run_task_id = Self::parse_run_task_id(raw)?;
                let run_status = Self::parse_run_status(raw)?;
                if run_task_id != task_id {
                    anyhow::bail!(
                        "active run '{}' references task '{}' instead of '{}'",
                        run_id,
                        run_task_id,
                        task_id
                    );
                }
                if run_status != "running" {
                    anyhow::bail!(
                        "active run '{}' is '{}' instead of running",
                        run_id,
                        run_status
                    );
                }
                store
                    .active_run
                    .insert(task_id.to_string(), run_id.to_string());
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
            self.read_store(|store| {
                Ok(store.and_then(|store| store.messages.get(message_id).cloned()))
            })
        }

        pub fn list_task_messages_for_task_raw(
            &self,
            task_id: &str,
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
}

mod cleanup {
    use super::*;

    impl TaskStorage {
        /// Delete old terminal tasks and their related messages/events.
        ///
        /// Returns the number of deleted tasks.
        pub fn cleanup_old_tasks(&self, older_than_ms: i64) -> Result<usize> {
            let tasks = self.list_tasks()?;
            let mut deleted = 0usize;

            for task in tasks {
                // Re-fetch current state before deleting to avoid race condition.
                // Between the initial list_tasks() snapshot and delete_task(),
                // another thread could have changed task status or timestamp.
                if let Some(current) = self.get_task(&task.id)? {
                    // Verify status is still terminal for cleanup.
                    if !matches!(
                        current.status,
                        TaskStatus::Completed
                            | TaskStatus::Failed
                                if current.next_run_at.is_none()
                    ) {
                        continue;
                    }

                    // Verify timestamp is still old enough for deletion
                    if current.updated_at >= older_than_ms {
                        continue;
                    }
                } else {
                    // Task was already deleted, skip
                    continue;
                }

                if self.delete_task(&task.id)? {
                    deleted += 1;
                }
            }

            Ok(deleted)
        }
    }
}

mod event_log {
    use super::*;

    impl TaskStorage {
        // ============== Task Event Operations ==============

        /// Add a new event for a task
        pub fn add_event(&self, event: &TaskEvent) -> Result<()> {
            let json_bytes = serde_json::to_vec(event)?;
            self.inner
                .put_event_raw(&event.id, &event.task_id, &json_bytes)?;
            Ok(())
        }

        /// Get an event by ID
        pub fn get_event(&self, event_id: &str) -> Result<Option<TaskEvent>> {
            if let Some(bytes) = self.inner.get_event_raw(event_id)? {
                let event: TaskEvent = serde_json::from_slice(&bytes)?;
                Ok(Some(event))
            } else {
                Ok(None)
            }
        }

        /// List all events for a task
        pub fn list_events_for_task(&self, task_id: &str) -> Result<Vec<TaskEvent>> {
            let events = self.inner.list_events_for_task_raw(task_id)?;
            let mut result = Vec::new();
            for (_, bytes) in events {
                let event: TaskEvent = serde_json::from_slice(&bytes)?;
                result.push(event);
            }

            // Sort by timestamp descending (most recent first).
            result.sort_by_key(|event| std::cmp::Reverse(event.timestamp));
            Ok(result)
        }

        /// List recent events for a task (with limit)
        pub fn list_recent_events_for_task(
            &self,
            task_id: &str,
            limit: usize,
        ) -> Result<Vec<TaskEvent>> {
            let events = self.list_events_for_task(task_id)?;
            Ok(events.into_iter().take(limit).collect())
        }
    }
}

mod message_queue {
    use super::*;

    impl TaskStorage {
        // ============== Task Message Operations ==============

        /// Queue a message for a task.
        pub fn send_task_message(
            &self,
            task_id: &str,
            message: String,
            source: TaskMessageSource,
        ) -> Result<TaskMessage> {
            if self.get_task(task_id)?.is_none() {
                return Err(anyhow::anyhow!("Task {} not found", task_id));
            }

            let bg_message = TaskMessage::new(task_id.to_string(), source, message);
            self.persist_task_message(&bg_message, None)?;
            Ok(bg_message)
        }

        /// Persist an agent-originated reply message for a task.
        ///
        /// The message is stored directly as consumed to avoid re-injection into
        /// the pending message pump (which only processes queued entries).
        pub fn log_task_reply(&self, task_id: &str, message: String) -> Result<TaskMessage> {
            if self.get_task(task_id)?.is_none() {
                return Err(anyhow::anyhow!("Task {} not found", task_id));
            }

            let mut bg_message =
                TaskMessage::new(task_id.to_string(), TaskMessageSource::Agent, message);
            bg_message.mark_delivered();
            bg_message.mark_consumed();
            self.persist_task_message(&bg_message, None)?;
            Ok(bg_message)
        }

        /// Get a task message by ID.
        pub fn get_task_message(&self, message_id: &str) -> Result<Option<TaskMessage>> {
            if let Some(bytes) = self.inner.get_task_message_raw(message_id)? {
                let message: TaskMessage = serde_json::from_slice(&bytes)?;
                Ok(Some(message))
            } else {
                Ok(None)
            }
        }

        /// List all task messages for a task, sorted by timestamp descending.
        pub fn list_task_messages(&self, task_id: &str, limit: usize) -> Result<Vec<TaskMessage>> {
            let raw = self.inner.list_task_messages_for_task_raw(task_id)?;
            let mut result = Vec::new();
            for (_, bytes) in raw {
                let message: TaskMessage = serde_json::from_slice(&bytes)?;
                result.push(message);
            }
            result.sort_by_key(|message| std::cmp::Reverse(message.created_at));
            Ok(result.into_iter().take(limit).collect())
        }

        /// List queued messages waiting for delivery.
        pub fn list_pending_task_messages(
            &self,
            task_id: &str,
            limit: usize,
        ) -> Result<Vec<TaskMessage>> {
            let raw = self.inner.list_task_messages_by_status_for_task_raw(
                task_id,
                TaskMessageStatus::Queued.as_str(),
            )?;
            let mut result = Vec::new();
            for (_, bytes) in raw {
                let message: TaskMessage = serde_json::from_slice(&bytes)?;
                result.push(message);
            }
            result.sort_by_key(|message| message.created_at);
            Ok(result.into_iter().take(limit).collect())
        }

        /// Mark a queued message as delivered.
        pub fn mark_task_message_delivered(&self, message_id: &str) -> Result<Option<TaskMessage>> {
            let mut message = match self.get_task_message(message_id)? {
                Some(message) => message,
                None => return Ok(None),
            };
            let previous_status = message.status.clone();
            message.mark_delivered();
            self.persist_task_message(&message, Some(previous_status))?;
            Ok(Some(message))
        }

        /// Mark a delivered message as consumed.
        pub fn mark_task_message_consumed(&self, message_id: &str) -> Result<Option<TaskMessage>> {
            let mut message = match self.get_task_message(message_id)? {
                Some(message) => message,
                None => return Ok(None),
            };
            let previous_status = message.status.clone();
            message.mark_consumed();
            self.persist_task_message(&message, Some(previous_status))?;
            Ok(Some(message))
        }

        /// Mark a message as failed with an error.
        pub fn mark_task_message_failed(
            &self,
            message_id: &str,
            error: String,
        ) -> Result<Option<TaskMessage>> {
            let mut message = match self.get_task_message(message_id)? {
                Some(message) => message,
                None => return Ok(None),
            };
            let previous_status = message.status.clone();
            message.mark_failed(error);
            self.persist_task_message(&message, Some(previous_status))?;
            Ok(Some(message))
        }

        fn persist_task_message(
            &self,
            message: &TaskMessage,
            previous_status: Option<TaskMessageStatus>,
        ) -> Result<()> {
            let json_bytes = serde_json::to_vec(message)?;
            if let Some(previous_status) = previous_status {
                self.inner.update_task_message_raw_with_status(
                    &message.id,
                    &message.task_id,
                    previous_status.as_str(),
                    message.status.as_str(),
                    &json_bytes,
                )?;
            } else {
                self.inner.put_task_message_raw_with_status(
                    &message.id,
                    &message.task_id,
                    message.status.as_str(),
                    &json_bytes,
                )?;
            }
            Ok(())
        }
    }
}

mod run_records {
    use super::*;
    use crate::models::{TaskRun, TaskRunMetrics, TaskRunStatus};
    use std::collections::BTreeMap;

    impl TaskStorage {
        pub fn create_task_run(&self, run: TaskRun) -> Result<TaskRun> {
            let json_bytes = serde_json::to_vec(&run)?;
            self.inner.put_run_raw_with_status(
                &run.run_id,
                &run.task_id,
                run.status.as_str(),
                &json_bytes,
            )?;
            Ok(run)
        }

        pub fn get_task_run(&self, run_id: &str) -> Result<Option<TaskRun>> {
            self.inner
                .get_run_raw(run_id)?
                .map(|bytes| serde_json::from_slice(&bytes).map_err(Into::into))
                .transpose()
        }

        pub fn list_task_runs(&self, task_id: &str) -> Result<Vec<TaskRun>> {
            let mut runs = self
                .inner
                .list_runs_by_task_raw(task_id)?
                .into_iter()
                .map(|(_, bytes)| {
                    serde_json::from_slice::<TaskRun>(&bytes).map_err(anyhow::Error::from)
                })
                .collect::<Result<Vec<_>>>()?;
            runs.sort_by_key(|run| (run.started_at, run.run_id.clone()));
            Ok(runs)
        }

        pub fn list_active_task_runs(&self) -> Result<Vec<TaskRun>> {
            let runs = self
                .inner
                .list_runs_raw()?
                .into_iter()
                .map(|(_, bytes)| {
                    serde_json::from_slice::<TaskRun>(&bytes).map_err(anyhow::Error::from)
                })
                .collect::<Result<Vec<_>>>()?;

            let mut by_task = BTreeMap::<String, Vec<TaskRun>>::new();
            for run in runs {
                by_task.entry(run.task_id.clone()).or_default().push(run);
            }

            let mut active_runs = Vec::new();
            for (task_id, task_runs) in by_task {
                if let Some(active) = self.reconcile_active_task_runs(&task_id, task_runs)? {
                    active_runs.push(active);
                }
            }

            active_runs.sort_by_key(|run| (run.started_at, run.run_id.clone()));
            Ok(active_runs)
        }

        pub fn get_active_task_run(&self, task_id: &str) -> Result<Option<TaskRun>> {
            self.reconcile_active_task_runs(task_id, self.list_task_runs(task_id)?)
        }

        pub fn start_task_run(
            &self,
            task_id: &str,
            run_id: impl Into<String>,
            execution_id: impl Into<String>,
            started_at: i64,
        ) -> Result<TaskRun> {
            let run_id = run_id.into();
            let execution_id = execution_id.into();

            let run = TaskRun::new(run_id, task_id.to_string(), execution_id, started_at);
            self.create_task_run(run)
        }

        pub fn mark_task_run_terminal(
            &self,
            run_id: &str,
            status: TaskRunStatus,
            ended_at: i64,
            error: Option<String>,
            metrics: TaskRunMetrics,
        ) -> Result<Option<TaskRun>> {
            let Some(mut run) = self.get_task_run(run_id)? else {
                return Ok(None);
            };

            let previous_status = run.status.clone();
            run.mark_terminal(status, ended_at, error, metrics);
            self.update_task_run(&run, previous_status)?;
            Ok(Some(run))
        }

        pub fn interrupt_task_run(
            &self,
            run_id: &str,
            ended_at: i64,
            reason: impl Into<String>,
        ) -> Result<Option<TaskRun>> {
            let Some(mut run) = self.get_task_run(run_id)? else {
                return Ok(None);
            };

            let previous_status = run.status.clone();
            run.mark_interrupted(ended_at, reason);
            self.update_task_run(&run, previous_status)?;
            Ok(Some(run))
        }

        fn update_task_run(&self, run: &TaskRun, previous_status: TaskRunStatus) -> Result<()> {
            let json_bytes = serde_json::to_vec(run)?;
            self.inner.update_run_raw_with_status(
                &run.run_id,
                &run.task_id,
                previous_status.as_str(),
                run.status.as_str(),
                &json_bytes,
            )
        }

        fn refresh_active_task_run_index(&self, run: &TaskRun) -> Result<()> {
            self.inner.set_active_run_raw(&run.task_id, &run.run_id)
        }

        fn reconcile_active_task_runs(
            &self,
            task_id: &str,
            runs: Vec<TaskRun>,
        ) -> Result<Option<TaskRun>> {
            let mut active_runs = runs
                .into_iter()
                .filter(|run| run.status.is_active())
                .collect::<Vec<_>>();

            if active_runs.is_empty() {
                if self.inner.get_active_run_raw(task_id)?.is_some() {
                    self.inner.clear_active_run_raw(task_id)?;
                }
                return Ok(None);
            }

            active_runs.sort_by_key(|run| (run.started_at, run.run_id.clone()));
            let winner = active_runs
                .pop()
                .expect("active run collection must be non-empty");

            if !active_runs.is_empty() {
                let recovered_at = chrono::Utc::now().timestamp_millis();
                let reason = format!("Recovered duplicate active run; kept '{}'", winner.run_id);
                for loser in active_runs {
                    self.interrupt_task_run(&loser.run_id, recovered_at, reason.clone())?;
                }
            }

            let active_index_is_current = self
                .inner
                .get_active_run_raw(task_id)?
                .and_then(|(run_id, raw)| {
                    if run_id != winner.run_id {
                        return None;
                    }
                    serde_json::from_slice::<TaskRun>(&raw).ok()
                })
                .is_some_and(|run| run.status.is_active() && run.task_id == task_id);

            if !active_index_is_current {
                self.refresh_active_task_run_index(&winner)?;
            }
            Ok(Some(winner))
        }
    }
}

mod session_binding {
    use super::*;

    impl TaskStorage {
        // ============== Task Operations ==============

        /// Validate a task creation spec without creating records or sessions.
        pub fn validate_create_spec(spec: &TaskSpec) -> Result<()> {
            Self::validate_timeout_secs(spec.timeout_secs)?;
            Self::validate_task_input(spec.input.as_deref(), spec.input_template.as_deref())
        }

        /// Validate a task update patch against the current task without mutating storage.
        pub fn validate_update_patch_for_task(task: &Task, patch: &TaskPatch) -> Result<()> {
            Self::validate_timeout_secs(patch.timeout_secs)?;
            let input = patch.input.as_deref().or(task.input.as_deref());
            let input_template = patch
                .input_template
                .as_deref()
                .or(task.input_template.as_deref());
            Self::validate_task_input(input, input_template)
        }

        /// Create a task from a rich spec.
        pub fn create_task_from_spec(&self, spec: TaskSpec) -> Result<Task> {
            let session_binding = TaskSessionBinding {
                session_id: Self::normalize_optional_id(spec.chat_session_id.clone())
                    .ok_or_else(|| anyhow::anyhow!("task must be bound to a chat session"))?,
                owns_session: false,
            };
            self.create_task_from_spec_with_binding(spec, session_binding)
        }

        /// Create a task after the caller has resolved its chat-session binding.
        pub fn create_task_from_spec_with_binding(
            &self,
            spec: TaskSpec,
            session_binding: TaskSessionBinding,
        ) -> Result<Task> {
            Self::validate_create_spec(&spec)?;
            Self::validate_task_session_binding(&session_binding)?;
            let TaskSpec {
                name,
                agent_id,
                chat_session_id: _,
                description,
                input,
                input_template,
                schedule,
                execution_mode,
                timeout_secs,
                resource_limits,
                prerequisites,
                continuation,
            } = spec;

            let mut task = Task::new(Uuid::new_v4().to_string(), name, agent_id, schedule);

            task.chat_session_id = session_binding.session_id;
            task.owns_chat_session = session_binding.owns_session;
            task.description = description;
            task.input = input;
            task.input_template = input_template;
            if let Some(execution_mode) = execution_mode {
                task.execution_mode = execution_mode;
            }
            task.timeout_secs = timeout_secs;
            if let Some(resource_limits) = resource_limits {
                task.resource_limits = Some(resource_limits);
            }
            task.prerequisites = prerequisites;
            if let Some(continuation) = continuation {
                task.continuation = continuation;
            }
            task.updated_at = chrono::Utc::now().timestamp_millis();

            self.save_task(&task)?;
            let event = TaskEvent::new(task.id.clone(), TaskEventType::Created)
                .with_message("Task created");
            self.add_event(&event)?;
            Ok(task)
        }

        /// Update a task with a partial patch.
        pub fn update_task_from_patch(&self, id: &str, patch: TaskPatch) -> Result<Task> {
            let task = self
                .get_task(id)?
                .ok_or_else(|| anyhow::anyhow!("Task {} not found", id))?;
            Self::validate_update_patch_for_task(&task, &patch)?;
            let session_binding = if let Some(session_id) =
                Self::normalize_optional_id(patch.chat_session_id.clone())
            {
                TaskSessionBinding {
                    session_id,
                    owns_session: false,
                }
            } else {
                TaskSessionBinding {
                    session_id: task.chat_session_id.clone(),
                    owns_session: task.owns_chat_session,
                }
            };
            self.update_task_from_patch_with_binding(id, patch, session_binding)
        }

        /// Update a task after the caller has resolved its chat-session binding.
        pub fn update_task_from_patch_with_binding(
            &self,
            id: &str,
            patch: TaskPatch,
            session_binding: TaskSessionBinding,
        ) -> Result<Task> {
            Self::validate_task_session_binding(&session_binding)?;
            let mut task = self
                .get_task(id)?
                .ok_or_else(|| anyhow::anyhow!("Task {} not found", id))?;
            Self::validate_update_patch_for_task(&task, &patch)?;
            let TaskPatch {
                name,
                description,
                agent_id,
                chat_session_id: _,
                input,
                input_template,
                schedule,
                execution_mode,
                timeout_secs,
                resource_limits,
                prerequisites,
                continuation,
            } = patch;

            if let Some(name) = name {
                task.name = name;
            }
            if let Some(description) = description {
                task.description = Some(description);
            }
            if let Some(agent_id) = agent_id {
                task.agent_id = agent_id;
            }
            task.chat_session_id = session_binding.session_id;
            task.owns_chat_session = session_binding.owns_session;
            if let Some(input) = input {
                task.input = Some(input);
            }
            if let Some(input_template) = input_template {
                task.input_template = Some(input_template);
            }
            if let Some(schedule) = schedule {
                task.schedule = schedule;
                task.update_next_run();
            }
            if let Some(execution_mode) = execution_mode {
                task.execution_mode = execution_mode;
            }
            if let Some(timeout_secs) = timeout_secs {
                task.timeout_secs = Some(timeout_secs);
            }
            if let Some(resource_limits) = resource_limits {
                task.resource_limits = Some(resource_limits);
            }
            if let Some(prerequisites) = prerequisites {
                task.prerequisites = prerequisites;
            }
            if let Some(continuation) = continuation {
                task.continuation = continuation;
                task.continuation_total_iterations = 0;
                task.continuation_segments_completed = 0;
            }
            task.updated_at = chrono::Utc::now().timestamp_millis();
            self.update_task(&task)?;
            Ok(task)
        }

        /// Apply a control action to a task.
        pub fn control_task(&self, id: &str, action: TaskControlAction) -> Result<Task> {
            let mut task = self
                .get_task(id)?
                .ok_or_else(|| anyhow::anyhow!("Task {} not found", id))?;

            let now = chrono::Utc::now().timestamp_millis();
            let event = match action {
                TaskControlAction::Start => {
                    task.status = TaskStatus::Active;
                    task.next_run_at = Some(now);
                    task.updated_at = now;
                    TaskEvent::new(task.id.clone(), TaskEventType::Resumed)
                        .with_message("Background agent started")
                }
                TaskControlAction::Pause => {
                    task.pause();
                    TaskEvent::new(task.id.clone(), TaskEventType::Paused)
                        .with_message("Background agent paused")
                }
                TaskControlAction::Resume => {
                    task.resume();
                    TaskEvent::new(task.id.clone(), TaskEventType::Resumed)
                        .with_message("Background agent resumed")
                }
                TaskControlAction::Stop => {
                    task.set_interrupted();
                    TaskEvent::new(task.id.clone(), TaskEventType::Interrupted)
                        .with_message("Background agent stopped")
                }
                TaskControlAction::RunNow => {
                    task.status = TaskStatus::Active;
                    task.next_run_at = Some(now);
                    task.updated_at = now;
                    TaskEvent::new(task.id.clone(), TaskEventType::Resumed)
                        .with_message("Background agent scheduled for immediate run")
                }
            };

            self.update_task(&task)?;
            self.add_event(&event)?;
            Ok(task)
        }

        /// Get aggregated progress for a task.
        pub fn get_task_progress(&self, id: &str, event_limit: usize) -> Result<TaskProgress> {
            let task = self
                .get_task(id)?
                .ok_or_else(|| anyhow::anyhow!("Task {} not found", id))?;
            let recent_events = self.list_recent_events_for_task(id, event_limit.max(1))?;
            let recent_event = recent_events.first().cloned();
            let stage = recent_event
                .as_ref()
                .map(|event| Self::event_stage_label(&event.event_type));
            let pending_message_count =
                self.list_pending_task_messages(id, usize::MAX)?.len() as u32;

            Ok(TaskProgress {
                task_id: task.id.clone(),
                status: task.status,
                stage,
                recent_event,
                recent_events,
                last_run_at: task.last_run_at,
                next_run_at: task.next_run_at,
                total_tokens_used: task.total_tokens_used,
                total_cost_usd: task.total_cost_usd,
                success_count: task.success_count,
                failure_count: task.failure_count,
                pending_message_count,
                transcript: None,
            })
        }
    }
}

mod task_lifecycle {
    use super::*;
    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum ResolveTaskIdError {
        #[error("Task not found: {0}")]
        NotFound(String),
        #[error("Task ID prefix '{prefix}' is ambiguous. Candidates: {preview}")]
        Ambiguous { prefix: String, preview: String },
        #[error(transparent)]
        Internal(#[from] anyhow::Error),
    }

    impl TaskStorage {
        // ============== Agent Task Operations ==============

        /// Create a new agent task for storage-level tests.
        #[cfg(any(test, feature = "test-utils"))]
        pub fn create_task(
            &self,
            name: String,
            agent_id: String,
            schedule: TaskSchedule,
        ) -> Result<Task> {
            let mut task = Task::new(Uuid::new_v4().to_string(), name, agent_id, schedule);
            task.chat_session_id = format!("test-session-{}", task.id);
            task.owns_chat_session = false;

            let json_bytes = serde_json::to_vec(&task)?;
            self.inner
                .put_task_raw_with_status(&task.id, task.status.as_str(), &json_bytes)?;

            let event = TaskEvent::new(task.id.clone(), TaskEventType::Created)
                .with_message("Task created");
            self.add_event(&event)?;

            Ok(task)
        }

        /// Get an agent task by ID
        pub fn get_task(&self, id: &str) -> Result<Option<Task>> {
            if let Some(bytes) = self.inner.get_task_raw(id)? {
                let task: Task = serde_json::from_slice(&bytes)?;
                Ok(Some(task))
            } else {
                Ok(None)
            }
        }

        /// Resolve a task ID or short prefix to the full task ID.
        ///
        /// This method is designed for user-facing entry points where users may
        /// provide a short ID prefix (e.g., "9f275c7a") instead of the full UUID.
        ///
        /// # Behavior
        ///
        /// - If `id_or_prefix` matches an exact task ID, returns that ID.
        /// - Otherwise, searches for tasks whose ID starts with `id_or_prefix`.
        /// - If exactly one match is found, returns the full ID.
        /// - If no matches are found, returns an error "Task not found".
        /// - If multiple matches are found, returns an error with candidate IDs.
        ///
        /// # Example
        ///
        /// ```ignore
        /// let full_id = storage.resolve_existing_task_id("9f275c7a")?;
        /// let task = storage.get_task(&full_id)?.unwrap();
        /// ```
        pub fn resolve_existing_task_id(&self, id_or_prefix: &str) -> Result<String> {
            self.resolve_existing_task_id_typed(id_or_prefix)
                .map_err(anyhow::Error::from)
        }

        pub fn resolve_existing_task_id_typed(
            &self,
            id_or_prefix: &str,
        ) -> std::result::Result<String, ResolveTaskIdError> {
            // First, try exact match (most common case)
            if self.get_task(id_or_prefix)?.is_some() {
                return Ok(id_or_prefix.to_string());
            }

            // Search for prefix matches
            let candidates: Vec<String> = self
                .list_tasks()?
                .into_iter()
                .filter(|task| task.id.starts_with(id_or_prefix))
                .map(|task| task.id)
                .collect();

            match candidates.len() {
                0 => Err(ResolveTaskIdError::NotFound(id_or_prefix.to_string())),
                1 => Ok(candidates.into_iter().next().unwrap()),
                _ => {
                    let preview: Vec<String> = candidates
                        .iter()
                        .take(5)
                        .map(|id| {
                            // Show first 8 chars of each ID for readability
                            if id.len() > 8 {
                                format!("{}...", &id[..8])
                            } else {
                                id.clone()
                            }
                        })
                        .collect();
                    Err(ResolveTaskIdError::Ambiguous {
                        prefix: id_or_prefix.to_string(),
                        preview: preview.join(", "),
                    })
                }
            }
        }

        /// List all agent tasks
        pub fn list_tasks(&self) -> Result<Vec<Task>> {
            let tasks = self.inner.list_tasks_raw()?;
            let mut result = Vec::new();
            for (_, bytes) in tasks {
                let task: Task = serde_json::from_slice(&bytes)?;
                result.push(task);
            }
            Ok(result)
        }

        /// List tasks filtered by status
        pub fn list_tasks_by_status(&self, status: TaskStatus) -> Result<Vec<Task>> {
            let indexed = self.inner.list_tasks_by_status_indexed(status.as_str())?;
            let mut result = Vec::new();
            let mut indexed_ids = HashSet::new();
            for (_, bytes) in indexed {
                let task: Task = serde_json::from_slice(&bytes)?;
                if task.status == status {
                    indexed_ids.insert(task.id.clone());
                    result.push(task);
                }
            }

            // Reconcile with a full scan to recover from partial status index drift.
            for task in self.list_tasks()? {
                if task.status == status && !indexed_ids.contains(&task.id) {
                    result.push(task);
                }
            }

            Ok(result)
        }

        /// List tasks filtered by agent ID.
        pub fn list_tasks_by_agent_id(&self, agent_id: &str) -> Result<Vec<Task>> {
            let tasks = self.list_tasks()?;
            Ok(tasks
                .into_iter()
                .filter(|task| task.agent_id == agent_id)
                .collect())
        }

        /// List tasks bound to the specified chat session.
        pub fn list_tasks_by_chat_session_id(&self, session_id: &str) -> Result<Vec<Task>> {
            let target = session_id.trim();
            if target.is_empty() {
                return Ok(Vec::new());
            }

            let tasks = self.list_tasks()?;
            Ok(tasks
                .into_iter()
                .filter(|task| task.chat_session_id.trim() == target)
                .collect())
        }

        /// List non-terminal tasks filtered by agent ID.
        pub fn list_active_tasks_by_agent_id(&self, agent_id: &str) -> Result<Vec<Task>> {
            let tasks = self.list_tasks_by_agent_id(agent_id)?;
            Ok(tasks
                .into_iter()
                .filter(|task| {
                    matches!(
                        task.status,
                        TaskStatus::Paused | TaskStatus::Running | TaskStatus::Interrupted
                    ) || task.is_active()
                })
                .collect())
        }

        /// List tasks that are ready to run
        pub fn list_runnable_tasks(&self, current_time: i64) -> Result<Vec<Task>> {
            let mut runnable = Vec::new();
            let tasks = self.list_tasks()?;

            for task in tasks {
                let Some(task) = self.repair_runnable_task_if_needed(task)? else {
                    continue;
                };

                if self.get_active_task_run(&task.id)?.is_some() {
                    continue;
                }

                if task.should_run(current_time) {
                    runnable.push(task);
                }
            }

            Ok(runnable)
        }

        fn needs_runnable_repair(task: &Task) -> bool {
            if task.next_run_at.is_none() {
                // Self-heal old tasks that have a cron/interval schedule but no
                // computed next run time (e.g., created before cron normalization).
                return true;
            }
            if let (Some(next_run), Some(last_run)) = (task.next_run_at, task.last_run_at) {
                // Self-heal tasks where next_run_at is stale (before last_run_at).
                // This can happen if the daemon was restarted mid-execution and
                // the completion handler didn't persist the updated schedule.
                return next_run < last_run;
            }
            false
        }

        pub(crate) fn repair_runnable_task_if_needed(
            &self,
            task_snapshot: Task,
        ) -> Result<Option<Task>> {
            if task_snapshot.status != TaskStatus::Active {
                return Ok(Some(task_snapshot));
            }

            if !Self::needs_runnable_repair(&task_snapshot) {
                return Ok(Some(task_snapshot));
            }

            // Reload latest state to avoid persisting a stale task snapshot.
            let Some(mut latest) = self.get_task(&task_snapshot.id)? else {
                return Ok(None);
            };

            if latest.status != TaskStatus::Active {
                // Status changed concurrently (e.g., pause/resume race). Do not
                // repair from stale snapshot or evaluate scheduling on it.
                return Ok(None);
            }

            if !Self::needs_runnable_repair(&latest) {
                return Ok(Some(latest));
            }

            latest.update_next_run();
            let persisted = match self.update_task_if_status_matches(&latest, TaskStatus::Active) {
                Ok(persisted) => persisted,
                Err(err) => {
                    warn!(
                        "Failed to persist repaired next_run_at for task {}: {}",
                        latest.id, err
                    );
                    // Skip scheduling decisions for tasks whose repaired state
                    // failed to persist to storage.
                    return Ok(None);
                }
            };
            if !persisted {
                warn!(
                    "Skipped runnable repair for task {} due to concurrent status change",
                    latest.id
                );
                return Ok(None);
            }

            Ok(Some(latest))
        }

        /// Save an agent task (insert or replace).
        /// Unlike `update_task`, this does not require the task to already exist.
        pub fn save_task(&self, task: &Task) -> Result<()> {
            Self::validate_task_has_session(task)?;
            let json_bytes = serde_json::to_vec(task)?;
            if let Some(existing) = self.get_task(&task.id)? {
                self.inner.update_task_raw_with_status(
                    &task.id,
                    existing.status.as_str(),
                    task.status.as_str(),
                    &json_bytes,
                )?;
            } else {
                self.inner
                    .put_task_raw_with_status(&task.id, task.status.as_str(), &json_bytes)?;
            }
            Ok(())
        }

        /// Update an existing agent task.
        /// Returns an error if the task does not exist.
        pub fn update_task(&self, task: &Task) -> Result<()> {
            let previous_status = self
                .get_task(&task.id)?
                .map(|existing| existing.status)
                .ok_or_else(|| anyhow::anyhow!("Task {} not found", task.id))?;
            Self::validate_task_has_session(task)?;
            let json_bytes = serde_json::to_vec(task)?;
            self.inner.update_task_raw_with_status(
                &task.id,
                previous_status.as_str(),
                task.status.as_str(),
                &json_bytes,
            )?;
            Ok(())
        }

        fn update_task_if_status_matches(
            &self,
            task: &Task,
            expected_status: TaskStatus,
        ) -> Result<bool> {
            Self::validate_task_has_session(task)?;
            let json_bytes = serde_json::to_vec(task)?;
            self.inner.update_task_raw_if_status_matches(
                &task.id,
                expected_status.as_str(),
                task.status.as_str(),
                &json_bytes,
            )
        }

        /// Delete an agent task and all task-owned records without touching sessions.
        pub fn delete_task_record(&self, id: &str) -> Result<Option<Task>> {
            let task = self.get_task(id)?;
            let deleted = self.inner.delete_task_cascade(id)?;
            if !deleted {
                return Ok(None);
            }

            Ok(task)
        }

        /// Delete an agent task and all its owned task records.
        ///
        /// Session lifecycle side effects are owned by TaskCommandService so JSONL
        /// and legacy redb sessions remain consistent.
        pub fn delete_task(&self, id: &str) -> Result<bool> {
            Ok(self.delete_task_record(id)?.is_some())
        }

        /// Pause an agent task
        pub fn pause_task(&self, id: &str) -> Result<Task> {
            let mut task = self
                .get_task(id)?
                .ok_or_else(|| anyhow::anyhow!("Task {} not found", id))?;

            task.pause();
            self.update_task(&task)?;

            // Record the pause event
            let event =
                TaskEvent::new(task.id.clone(), TaskEventType::Paused).with_message("Task paused");
            self.add_event(&event)?;

            Ok(task)
        }

        /// Resume an agent task
        pub fn resume_task(&self, id: &str) -> Result<Task> {
            let mut task = self
                .get_task(id)?
                .ok_or_else(|| anyhow::anyhow!("Task {} not found", id))?;

            task.resume();
            self.update_task(&task)?;

            // Record the resume event
            let event = TaskEvent::new(task.id.clone(), TaskEventType::Resumed)
                .with_message("Task resumed");
            self.add_event(&event)?;

            Ok(task)
        }

        /// Mark a task as running
        pub fn start_task_execution(&self, id: &str) -> Result<Task> {
            let mut task = self
                .get_task(id)?
                .ok_or_else(|| anyhow::anyhow!("Task {} not found", id))?;

            let expected_status = if task.status == TaskStatus::Active {
                TaskStatus::Active
            } else if task.status == TaskStatus::Failed && task.next_run_at.is_some() {
                TaskStatus::Failed
            } else {
                return Err(anyhow::anyhow!(
                    "Task {} cannot start from status {}",
                    id,
                    task.status.as_str()
                ));
            };

            task.set_running();
            // Use CAS semantics so only one concurrent caller can transition the runnable task into
            // Running, including retryable Failed interval/cron tasks.
            let started = self.update_task_if_status_matches(&task, expected_status)?;
            if !started {
                let latest_status = self
                    .get_task(id)?
                    .map(|latest| latest.status.as_str().to_string())
                    .unwrap_or_else(|| "deleted".to_string());
                return Err(anyhow::anyhow!(
                    "Task {} cannot start from status {}",
                    id,
                    latest_status
                ));
            }

            // Record the start event
            let event = TaskEvent::new(task.id.clone(), TaskEventType::Started)
                .with_message("Task execution started");
            self.add_event(&event)?;

            Ok(task)
        }

        /// Mark a task as completed
        pub fn complete_task_execution(
            &self,
            id: &str,
            output: Option<String>,
            duration_ms: i64,
        ) -> Result<Task> {
            let mut task = self
                .get_task(id)?
                .ok_or_else(|| anyhow::anyhow!("Task {} not found", id))?;

            task.set_completed();
            self.update_task(&task)?;

            // Record the completion event
            let mut event = TaskEvent::new(task.id.clone(), TaskEventType::Completed)
                .with_message("Task execution completed")
                .with_duration(duration_ms);
            if let Some(out) = output {
                event = event.with_output(out);
            }
            self.add_event(&event)?;

            Ok(task)
        }

        /// Mark a task as failed
        pub fn fail_task_execution(
            &self,
            id: &str,
            error: String,
            duration_ms: i64,
        ) -> Result<Task> {
            let mut task = self
                .get_task(id)?
                .ok_or_else(|| anyhow::anyhow!("Task {} not found", id))?;

            task.set_failed(error.clone());
            self.update_task(&task)?;

            // Record the failure event
            let event = TaskEvent::new(task.id.clone(), TaskEventType::Failed)
                .with_message(error)
                .with_duration(duration_ms);
            self.add_event(&event)?;

            Ok(task)
        }
    }
}

pub use task_lifecycle::ResolveTaskIdError;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{TaskRun, TaskRunStatus};
    use std::sync::Barrier;
    use std::thread;
    use tempfile::tempdir;

    fn create_test_storage() -> TaskStorage {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        TaskStorage::new(db).unwrap()
    }

    #[test]
    fn test_file_backed_task_storage_survives_process_namespace_change() {
        let temp_dir = tempdir().unwrap();
        let task_store_path = temp_dir.path().join("tasks.json");

        {
            let db_path = temp_dir.path().join("first.db");
            let db = Arc::new(Database::create(db_path).unwrap());
            let storage = TaskStorage::new_file_backed(db, task_store_path.clone()).unwrap();
            storage
                .create_task_from_spec(TaskSpec {
                    name: "File Backed Task".to_string(),
                    agent_id: "agent-001".to_string(),
                    chat_session_id: Some("session-1".to_string()),
                    description: None,
                    input: Some("persist me".to_string()),
                    input_template: None,
                    schedule: TaskSchedule::default(),
                    execution_mode: None,
                    timeout_secs: None,
                    resource_limits: None,
                    prerequisites: Vec::new(),
                    continuation: None,
                })
                .unwrap();
        }

        let db_path = temp_dir.path().join("second.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = TaskStorage::new_file_backed(db, task_store_path).unwrap();
        let tasks = storage.list_tasks().unwrap();

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name, "File Backed Task");
    }

    #[test]
    fn test_file_backed_task_storage_serializes_parallel_creates() {
        let temp_dir = tempdir().unwrap();
        let task_store_path = temp_dir.path().join("tasks.json");
        let db_path = temp_dir.path().join("tasks.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = TaskStorage::new_file_backed(db, task_store_path).unwrap();
        let barrier = Arc::new(Barrier::new(2));

        let handles = ["Parallel Task A", "Parallel Task B"]
            .into_iter()
            .map(|name| {
                let storage = storage.clone();
                let barrier = barrier.clone();
                thread::spawn(move || {
                    barrier.wait();
                    storage
                        .create_task_from_spec(TaskSpec {
                            name: name.to_string(),
                            agent_id: "agent-001".to_string(),
                            chat_session_id: Some("session-1".to_string()),
                            description: None,
                            input: Some(name.to_string()),
                            input_template: None,
                            schedule: TaskSchedule::default(),
                            execution_mode: None,
                            timeout_secs: None,
                            resource_limits: None,
                            prerequisites: Vec::new(),
                            continuation: None,
                        })
                        .unwrap();
                })
            })
            .collect::<Vec<_>>();

        for handle in handles {
            handle.join().unwrap();
        }

        let tasks = storage.list_tasks().unwrap();
        let names = tasks
            .iter()
            .map(|task| task.name.as_str())
            .collect::<std::collections::HashSet<_>>();

        assert_eq!(tasks.len(), 2);
        assert!(names.contains("Parallel Task A"));
        assert!(names.contains("Parallel Task B"));
    }

    #[test]
    fn test_file_backed_task_storage_serializes_parallel_creates_across_namespaces() {
        let temp_dir = tempdir().unwrap();
        let task_store_path = temp_dir.path().join("tasks.json");
        let first_db = Arc::new(Database::create(temp_dir.path().join("first.db")).unwrap());
        let second_db = Arc::new(Database::create(temp_dir.path().join("second.db")).unwrap());
        let first = TaskStorage::new_file_backed(first_db, task_store_path.clone()).unwrap();
        let second = TaskStorage::new_file_backed(second_db, task_store_path.clone()).unwrap();
        let barrier = Arc::new(Barrier::new(2));

        let handles = [
            ("Cross Namespace A", first.clone()),
            ("Cross Namespace B", second.clone()),
        ]
        .into_iter()
        .map(|(name, storage)| {
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                storage
                    .create_task_from_spec(TaskSpec {
                        name: name.to_string(),
                        agent_id: "agent-001".to_string(),
                        chat_session_id: Some("session-1".to_string()),
                        description: None,
                        input: Some(name.to_string()),
                        input_template: None,
                        schedule: TaskSchedule::default(),
                        execution_mode: None,
                        timeout_secs: None,
                        resource_limits: None,
                        prerequisites: Vec::new(),
                        continuation: None,
                    })
                    .unwrap();
            })
        })
        .collect::<Vec<_>>();

        for handle in handles {
            handle.join().unwrap();
        }

        let db = Arc::new(Database::create(temp_dir.path().join("reader.db")).unwrap());
        let reader = TaskStorage::new_file_backed(db, task_store_path).unwrap();
        let tasks = reader.list_tasks().unwrap();
        let names = tasks
            .iter()
            .map(|task| task.name.as_str())
            .collect::<std::collections::HashSet<_>>();

        assert_eq!(tasks.len(), 2);
        assert!(names.contains("Cross Namespace A"));
        assert!(names.contains("Cross Namespace B"));
    }

    #[cfg(unix)]
    #[test]
    fn test_file_backed_task_storage_reuses_cached_snapshot_when_file_unchanged() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = tempdir().unwrap();
        let task_store_path = temp_dir.path().join("tasks.json");
        let db = Arc::new(Database::create(temp_dir.path().join("tasks.db")).unwrap());
        let storage = TaskStorage::new_file_backed(db, task_store_path.clone()).unwrap();
        let task = storage
            .create_task(
                "Cached Snapshot".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        let original_mode = task_store_path.metadata().unwrap().permissions().mode();
        std::fs::set_permissions(&task_store_path, std::fs::Permissions::from_mode(0o000)).unwrap();
        let result = (|| {
            let loaded = storage.get_task(&task.id)?.expect("cached task");
            assert_eq!(loaded.id, task.id);
            assert_eq!(loaded.name, "Cached Snapshot");
            anyhow::Ok(())
        })();
        std::fs::set_permissions(
            &task_store_path,
            std::fs::Permissions::from_mode(original_mode & 0o777),
        )
        .unwrap();

        result.unwrap();
    }

    // ============== Short ID Resolution Tests ==============

    #[test]
    fn test_resolve_existing_task_id_exact_match() {
        let storage = create_test_storage();

        let task = storage
            .create_task_from_spec(TaskSpec {
                name: "Test Task".to_string(),
                agent_id: "agent-001".to_string(),
                chat_session_id: Some("session-1".to_string()),
                description: None,
                input: Some("test input".to_string()),
                input_template: None,
                schedule: TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();

        // Full ID should resolve to itself
        let resolved = storage.resolve_existing_task_id(&task.id).unwrap();
        assert_eq!(resolved, task.id);
    }

    #[test]
    fn test_resolve_existing_task_id_typed_exact_match() {
        let storage = create_test_storage();

        let task = storage
            .create_task(
                "Typed Exact".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        let resolved = storage.resolve_existing_task_id_typed(&task.id).unwrap();
        assert_eq!(resolved, task.id);
    }

    #[test]
    fn test_resolve_existing_task_id_unique_prefix() {
        let storage = create_test_storage();

        let task = storage
            .create_task_from_spec(TaskSpec {
                name: "Test Task".to_string(),
                agent_id: "agent-001".to_string(),
                chat_session_id: Some("session-1".to_string()),
                description: None,
                input: Some("test input".to_string()),
                input_template: None,
                schedule: TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();

        // 8-char prefix should resolve to full ID
        let prefix = &task.id[..8];
        let resolved = storage.resolve_existing_task_id(prefix).unwrap();
        assert_eq!(resolved, task.id);
    }

    #[test]
    fn test_resolve_existing_task_id_typed_unique_prefix() {
        let storage = create_test_storage();

        let task = storage
            .create_task(
                "Typed Prefix".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        let prefix = &task.id[..8];
        let resolved = storage.resolve_existing_task_id_typed(prefix).unwrap();
        assert_eq!(resolved, task.id);
    }

    #[test]
    fn test_resolve_existing_task_id_unknown_prefix() {
        let storage = create_test_storage();

        let result = storage.resolve_existing_task_id("nonexist");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Task not found"));
    }

    #[test]
    fn test_resolve_existing_task_id_typed_returns_not_found() {
        let storage = create_test_storage();

        let result = storage.resolve_existing_task_id_typed("nonexist");
        match result {
            Err(ResolveTaskIdError::NotFound(id)) => assert_eq!(id, "nonexist"),
            other => panic!("expected not found error, got {other:?}"),
        }
    }

    #[test]
    fn test_resolve_existing_task_id_ambiguous_prefix() {
        let storage = create_test_storage();

        // Create multiple tasks
        let _task1 = storage
            .create_task_from_spec(TaskSpec {
                name: "Task 1".to_string(),
                agent_id: "agent-001".to_string(),
                chat_session_id: Some("session-1".to_string()),
                description: None,
                input: Some("test input".to_string()),
                input_template: None,
                schedule: TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();

        let _task2 = storage
            .create_task_from_spec(TaskSpec {
                name: "Task 2".to_string(),
                agent_id: "agent-001".to_string(),
                chat_session_id: Some("session-1".to_string()),
                description: None,
                input: Some("test input".to_string()),
                input_template: None,
                schedule: TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();

        // Empty string should match all tasks (ambiguous)
        let result = storage.resolve_existing_task_id("");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("ambiguous"),
            "Error should mention ambiguity"
        );
        assert!(
            err_msg.contains("Candidates"),
            "Error should list candidates"
        );
    }

    #[test]
    fn test_resolve_existing_task_id_typed_returns_ambiguous() {
        let storage = create_test_storage();
        let raw_storage = storage.inner.clone();

        for id in ["shared-1", "shared-2"] {
            let task = Task::new(
                id.to_string(),
                format!("Task {id}"),
                "agent-001".to_string(),
                TaskSchedule::default(),
            );
            let raw = serde_json::to_vec(&task).unwrap();
            raw_storage
                .put_task_raw_with_status(id, task.status.as_str(), &raw)
                .unwrap();
        }

        let result = storage.resolve_existing_task_id_typed("shared");
        match result {
            Err(ResolveTaskIdError::Ambiguous { prefix, preview }) => {
                assert_eq!(prefix, "shared");
                assert!(preview.contains("shared-1"));
                assert!(preview.contains("shared-2"));
            }
            other => panic!("expected ambiguous error, got {other:?}"),
        }
    }

    #[test]
    fn test_resolve_existing_task_id_typed_returns_internal_for_malformed_task_scan() {
        let storage = create_test_storage();
        storage
            .inner
            .put_task_raw_with_status("bad-task", "active", b"{bad-json")
            .unwrap();

        let result = storage.resolve_existing_task_id_typed("missing-prefix");
        match result {
            Err(ResolveTaskIdError::Internal(err)) => {
                assert!(err.to_string().contains("key must be a string"));
            }
            other => panic!("expected internal error, got {other:?}"),
        }
    }

    #[test]
    fn test_task_runs_round_trip() {
        let storage = create_test_storage();
        let task = storage
            .create_task(
                "Run Tracking".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        let run = storage
            .start_task_run(&task.id, "run-1", "exec-1", 100)
            .unwrap();
        assert_eq!(run.run_id, "run-1");

        let active_run = storage
            .get_active_task_run(&task.id)
            .unwrap()
            .expect("active run");
        assert_eq!(active_run.execution_id, "exec-1");

        let completed = storage
            .mark_task_run_terminal(
                "run-1",
                crate::models::TaskRunStatus::Completed,
                200,
                None,
                crate::models::TaskRunMetrics {
                    duration_ms: Some(100),
                    ..Default::default()
                },
            )
            .unwrap()
            .expect("completed run");
        assert_eq!(completed.status, crate::models::TaskRunStatus::Completed);
        assert!(storage.get_active_task_run(&task.id).unwrap().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn test_active_run_reads_do_not_rewrite_file_backed_store() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = tempdir().unwrap();
        let task_store_path = temp_dir.path().join("tasks.json");
        let db = Arc::new(Database::create(temp_dir.path().join("tasks.db")).unwrap());
        let storage = TaskStorage::new_file_backed(db, task_store_path).unwrap();
        let task = storage
            .create_task(
                "Read Only Active Run".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();
        storage
            .start_task_run(&task.id, "run-1", "exec-1", 100)
            .unwrap();

        let original_mode = temp_dir.path().metadata().unwrap().permissions().mode();
        std::fs::set_permissions(temp_dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
        let result = (|| {
            let active = storage.get_active_task_run(&task.id)?.expect("active run");
            assert_eq!(active.run_id, "run-1");

            let active_runs = storage.list_active_task_runs()?;
            assert_eq!(active_runs.len(), 1);
            assert_eq!(active_runs[0].run_id, "run-1");
            anyhow::Ok(())
        })();
        std::fs::set_permissions(
            temp_dir.path(),
            std::fs::Permissions::from_mode(original_mode & 0o777),
        )
        .unwrap();

        result.unwrap();
    }

    #[test]
    fn test_start_task_run_rejects_second_active_run_for_task() {
        let storage = create_test_storage();
        let task = storage
            .create_task(
                "Single Active Run".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        storage
            .start_task_run(&task.id, "run-1", "exec-1", 100)
            .unwrap();
        let result = storage.start_task_run(&task.id, "run-2", "exec-2", 200);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("already has active run")
        );
    }

    #[test]
    fn test_get_active_task_run_recovers_duplicate_legacy_active_runs() {
        let storage = create_test_storage();
        let task = storage
            .create_task(
                "Legacy Duplicate Runs".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        let run_one = TaskRun::new("run-1", task.id.clone(), "exec-1", 100);
        let run_two = TaskRun::new("run-2", task.id.clone(), "exec-2", 200);
        let run_one_raw = serde_json::to_vec(&run_one).unwrap();
        let run_two_raw = serde_json::to_vec(&run_two).unwrap();
        storage
            .inner
            .put_run_raw(&run_one.run_id, &run_one.task_id, &run_one_raw)
            .unwrap();
        storage
            .inner
            .put_run_raw(&run_two.run_id, &run_two.task_id, &run_two_raw)
            .unwrap();

        let active = storage
            .get_active_task_run(&task.id)
            .unwrap()
            .expect("active run should be recovered");
        assert_eq!(active.run_id, "run-2");

        let interrupted = storage
            .get_task_run("run-1")
            .unwrap()
            .expect("legacy loser run");
        assert_eq!(interrupted.status, TaskRunStatus::Interrupted);
        assert!(
            interrupted
                .error
                .as_deref()
                .is_some_and(|value| value.contains("Recovered duplicate active run"))
        );

        let indexed = storage
            .inner
            .get_active_run_raw(&task.id)
            .unwrap()
            .expect("active-run index should self-heal");
        assert_eq!(indexed.0, "run-2");
    }

    #[test]
    fn test_resolve_existing_task_id_exact_priority_over_prefix() {
        let storage = create_test_storage();

        let task = storage
            .create_task_from_spec(TaskSpec {
                name: "Test Task".to_string(),
                agent_id: "agent-001".to_string(),
                chat_session_id: Some("session-1".to_string()),
                description: None,
                input: Some("test input".to_string()),
                input_template: None,
                schedule: TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();

        // Even if there's a prefix collision, exact match should win
        // (This is already the case because we check exact first)
        let resolved = storage.resolve_existing_task_id(&task.id).unwrap();
        assert_eq!(resolved, task.id);
    }

    // ============== Original Tests ==============

    #[test]
    fn test_create_and_get_task() {
        let storage = create_test_storage();

        let task = storage
            .create_task(
                "Test Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Interval {
                    interval_ms: 3600000,
                    start_at: None,
                },
            )
            .unwrap();

        assert!(!task.id.is_empty());
        assert_eq!(task.name, "Test Task");
        assert_eq!(task.agent_id, "agent-001");
        assert_eq!(task.status, TaskStatus::Active);

        let retrieved = storage.get_task(&task.id).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Test Task");
    }

    #[test]
    fn test_list_tasks() {
        let storage = create_test_storage();

        storage
            .create_task(
                "Task 1".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();
        storage
            .create_task(
                "Task 2".to_string(),
                "agent-002".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        let tasks = storage.list_tasks().unwrap();
        assert_eq!(tasks.len(), 2);
    }

    #[test]
    fn test_get_task_returns_error_for_malformed_record() {
        let storage = create_test_storage();
        storage
            .inner
            .put_task_raw_with_status("bad-task", "active", b"{bad-json")
            .unwrap();

        let result = storage.get_task("bad-task");
        assert!(result.is_err());
    }

    #[test]
    fn test_list_tasks_returns_error_when_any_record_is_malformed() {
        let storage = create_test_storage();
        storage
            .create_task(
                "Good Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();
        storage
            .inner
            .put_task_raw_with_status("bad-task", "active", b"{bad-json")
            .unwrap();

        let result = storage.list_tasks();
        assert!(result.is_err());
    }

    #[test]
    fn test_list_tasks_by_status() {
        let storage = create_test_storage();

        let task1 = storage
            .create_task(
                "Active Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        let task2 = storage
            .create_task(
                "Will be Paused".to_string(),
                "agent-002".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        storage.pause_task(&task2.id).unwrap();

        let active_tasks = storage.list_tasks_by_status(TaskStatus::Active).unwrap();
        let paused_tasks = storage.list_tasks_by_status(TaskStatus::Paused).unwrap();

        assert_eq!(active_tasks.len(), 1);
        assert_eq!(active_tasks[0].id, task1.id);
        assert_eq!(paused_tasks.len(), 1);
        assert_eq!(paused_tasks[0].id, task2.id);
    }

    #[test]
    fn test_list_tasks_by_status_falls_back_to_full_scan_when_index_is_empty() {
        let storage = create_test_storage();

        let task = storage
            .create_task(
                "Fallback Target".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        // Corrupt index consistency intentionally:
        // persist payload as "paused" without updating status index.
        let mut paused_payload = storage.get_task(&task.id).unwrap().unwrap();
        paused_payload.status = TaskStatus::Paused;
        paused_payload.updated_at += 1;
        let raw = serde_json::to_vec(&paused_payload).unwrap();
        storage.inner.put_task_raw(&task.id, &raw).unwrap();

        // Indexed query for paused should be empty, forcing fallback to full scan.
        let indexed_paused = storage
            .inner
            .list_tasks_by_status_indexed("paused")
            .unwrap();
        assert!(indexed_paused.is_empty());

        let paused_tasks = storage.list_tasks_by_status(TaskStatus::Paused).unwrap();
        assert_eq!(paused_tasks.len(), 1);
        assert_eq!(paused_tasks[0].id, task.id);
    }

    #[test]
    fn test_list_tasks_by_status_recovers_from_partial_index_drift() {
        let storage = create_test_storage();

        let missing_from_index = storage
            .create_task(
                "Missing Paused Index".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();
        let indexed_paused = storage
            .create_task(
                "Indexed Paused".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        // Keep payload status paused, but intentionally skip status-index update.
        let mut paused_payload = storage.get_task(&missing_from_index.id).unwrap().unwrap();
        paused_payload.status = TaskStatus::Paused;
        paused_payload.updated_at += 1;
        let raw = serde_json::to_vec(&paused_payload).unwrap();
        storage
            .inner
            .put_task_raw(&missing_from_index.id, &raw)
            .unwrap();

        storage.pause_task(&indexed_paused.id).unwrap();
        let indexed_only = storage
            .inner
            .list_tasks_by_status_indexed("paused")
            .unwrap();
        assert_eq!(indexed_only.len(), 1);
        assert_eq!(indexed_only[0].0, indexed_paused.id);

        let paused_tasks = storage.list_tasks_by_status(TaskStatus::Paused).unwrap();
        let ids: std::collections::HashSet<_> =
            paused_tasks.iter().map(|task| task.id.clone()).collect();
        assert_eq!(paused_tasks.len(), 2);
        assert!(ids.contains(&missing_from_index.id));
        assert!(ids.contains(&indexed_paused.id));
    }

    #[test]
    fn test_save_task_status_transition_keeps_status_queries_consistent() {
        let storage = create_test_storage();
        let created = storage
            .create_task(
                "Save Transition".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        let mut updated = storage.get_task(&created.id).unwrap().unwrap();
        updated.pause();
        storage.save_task(&updated).unwrap();

        let active_tasks = storage.list_tasks_by_status(TaskStatus::Active).unwrap();
        let paused_tasks = storage.list_tasks_by_status(TaskStatus::Paused).unwrap();

        assert!(active_tasks.iter().all(|task| task.id != created.id));
        assert_eq!(paused_tasks.len(), 1);
        assert_eq!(paused_tasks[0].id, created.id);
    }

    #[test]
    fn test_status_index_consistency_after_multiple_status_transitions() {
        let storage = create_test_storage();
        let first = storage
            .create_task(
                "Transition A".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();
        let second = storage
            .create_task(
                "Transition B".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        let mut transitioning = storage.get_task(&first.id).unwrap().unwrap();
        transitioning.pause();
        storage.save_task(&transitioning).unwrap();

        let mut transitioning = storage.get_task(&first.id).unwrap().unwrap();
        transitioning.resume();
        storage.save_task(&transitioning).unwrap();

        let active_tasks = storage.list_tasks_by_status(TaskStatus::Active).unwrap();
        let paused_tasks = storage.list_tasks_by_status(TaskStatus::Paused).unwrap();
        assert_eq!(active_tasks.len(), 2);
        assert!(active_tasks.iter().any(|task| task.id == first.id));
        assert!(active_tasks.iter().any(|task| task.id == second.id));
        assert!(paused_tasks.iter().all(|task| task.id != first.id));

        let indexed_active = storage
            .inner
            .list_tasks_by_status_indexed("active")
            .unwrap();
        let indexed_paused = storage
            .inner
            .list_tasks_by_status_indexed("paused")
            .unwrap();
        assert_eq!(indexed_active.len(), 2);
        assert!(indexed_active.iter().any(|(id, _)| id == &first.id));
        assert!(indexed_active.iter().any(|(id, _)| id == &second.id));
        assert!(indexed_paused.iter().all(|(id, _)| id != &first.id));
    }

    #[test]
    fn test_list_tasks_by_agent_id() {
        let storage = create_test_storage();

        let task1 = storage
            .create_task(
                "Agent One Active".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();
        let task2 = storage
            .create_task(
                "Agent One Paused".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();
        let _task3 = storage
            .create_task(
                "Agent Two Active".to_string(),
                "agent-002".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        storage.pause_task(&task2.id).unwrap();

        let mut tasks = storage.list_tasks_by_agent_id("agent-001").unwrap();
        tasks.sort_by(|a, b| a.name.cmp(&b.name));

        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].id, task1.id);
        assert_eq!(tasks[1].id, task2.id);
    }

    #[test]
    fn test_list_active_tasks_by_agent_id() {
        let storage = create_test_storage();

        let active = storage
            .create_task(
                "Active".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();
        let paused = storage
            .create_task(
                "Paused".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();
        let completed = storage
            .create_task(
                "Completed".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Once {
                    run_at: chrono::Utc::now().timestamp_millis(),
                },
            )
            .unwrap();
        let recurring_failed = storage
            .create_task(
                "Recurring Failed".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Interval {
                    interval_ms: 60_000,
                    start_at: None,
                },
            )
            .unwrap();
        let once_failed = storage
            .create_task(
                "Once Failed".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Once {
                    run_at: chrono::Utc::now().timestamp_millis() - 1_000,
                },
            )
            .unwrap();

        storage.pause_task(&paused.id).unwrap();
        storage.start_task_execution(&completed.id).unwrap();
        storage
            .complete_task_execution(&completed.id, Some("done".to_string()), 100)
            .unwrap();
        storage.start_task_execution(&recurring_failed.id).unwrap();
        storage
            .fail_task_execution(&recurring_failed.id, "retry me".to_string(), 100)
            .unwrap();
        storage.start_task_execution(&once_failed.id).unwrap();
        storage
            .fail_task_execution(&once_failed.id, "terminal".to_string(), 100)
            .unwrap();

        let mut tasks = storage.list_active_tasks_by_agent_id("agent-001").unwrap();
        tasks.sort_by(|a, b| a.name.cmp(&b.name));

        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].id, active.id);
        assert_eq!(tasks[1].id, paused.id);
        assert_eq!(tasks[2].id, recurring_failed.id);
    }

    #[test]
    fn test_cleanup_old_tasks_keeps_non_terminal() {
        let storage = create_test_storage();
        let now = chrono::Utc::now().timestamp_millis();

        let terminal = storage
            .create_task(
                "Terminal Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Once {
                    run_at: now - 10_000,
                },
            )
            .unwrap();
        storage
            .fail_task_execution(&terminal.id, "failed".to_string(), 1)
            .unwrap();
        let mut terminal_updated = storage.get_task(&terminal.id).unwrap().unwrap();
        terminal_updated.updated_at = now - (10 * 24 * 60 * 60 * 1000);
        storage.update_task(&terminal_updated).unwrap();

        let mut active = storage
            .create_task(
                "Active Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();
        active.updated_at = now - (30 * 24 * 60 * 60 * 1000);
        storage.update_task(&active).unwrap();

        let recurring_failed = storage
            .create_task(
                "Recurring Failed".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Interval {
                    interval_ms: 60_000,
                    start_at: None,
                },
            )
            .unwrap();
        storage
            .fail_task_execution(&recurring_failed.id, "retryable".to_string(), 1)
            .unwrap();
        let mut recurring_failed_updated = storage.get_task(&recurring_failed.id).unwrap().unwrap();
        recurring_failed_updated.updated_at = now - (10 * 24 * 60 * 60 * 1000);
        storage.update_task(&recurring_failed_updated).unwrap();

        let cutoff = now - (7 * 24 * 60 * 60 * 1000);
        let deleted = storage.cleanup_old_tasks(cutoff).unwrap();
        assert_eq!(deleted, 1);
        assert!(storage.get_task(&terminal.id).unwrap().is_none());
        assert!(storage.get_task(&active.id).unwrap().is_some());
        assert!(storage.get_task(&recurring_failed.id).unwrap().is_some());
    }

    #[test]
    fn test_delete_task() {
        let storage = create_test_storage();

        let task = storage
            .create_task(
                "To Delete".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        // Add some events
        let event = TaskEvent::new(task.id.clone(), TaskEventType::Started);
        storage.add_event(&event).unwrap();
        let bg_message = storage
            .send_task_message(
                &task.id,
                "queued message".to_string(),
                TaskMessageSource::User,
            )
            .unwrap();
        assert_eq!(bg_message.status, TaskMessageStatus::Queued);

        // Delete the task
        let deleted = storage.delete_task(&task.id).unwrap();
        assert!(deleted);

        // Task should be gone
        let retrieved = storage.get_task(&task.id).unwrap();
        assert!(retrieved.is_none());

        // Events should also be gone
        let events = storage.list_events_for_task(&task.id).unwrap();
        assert!(events.is_empty());

        // Task messages should also be gone
        let messages = storage.list_task_messages(&task.id, 10).unwrap();
        assert!(messages.is_empty());
    }

    #[test]
    fn test_delete_task_does_not_archive_owned_chat_session() {
        let storage = create_test_storage();
        let task = storage
            .create_task_from_spec_with_binding(
                TaskSpec {
                    name: "Archive On Delete".to_string(),
                    agent_id: "agent-archive".to_string(),
                    chat_session_id: Some("session-1".to_string()),
                    description: None,
                    input: Some("archive me".to_string()),
                    input_template: None,
                    schedule: TaskSchedule::default(),
                    execution_mode: None,
                    timeout_secs: None,
                    resource_limits: None,
                    prerequisites: Vec::new(),
                    continuation: None,
                },
                TaskSessionBinding {
                    session_id: "owned-session".to_string(),
                    owns_session: true,
                },
            )
            .unwrap();

        assert!(task.owns_chat_session);
        let deleted = storage.delete_task(&task.id).unwrap();
        assert!(deleted);
        assert!(storage.get_task(&task.id).unwrap().is_none());
    }

    #[test]
    fn test_delete_task_does_not_archive_non_owned_chat_session() {
        let storage = create_test_storage();
        let shared_session_id = "shared-session".to_string();

        let task = storage
            .create_task_from_spec(TaskSpec {
                name: "External Session".to_string(),
                agent_id: "agent-shared".to_string(),
                chat_session_id: Some(shared_session_id.clone()),
                description: None,
                input: Some("keep session".to_string()),
                input_template: None,
                schedule: TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();

        assert!(!task.owns_chat_session);
        let deleted = storage.delete_task(&task.id).unwrap();
        assert!(deleted);
        assert!(storage.get_task(&task.id).unwrap().is_none());
    }

    #[test]
    fn test_create_task_accepts_raw_reused_chat_session_binding() {
        let storage = create_test_storage();
        let shared_session_id = "shared-session".to_string();
        let owner_task = storage
            .create_task_from_spec(TaskSpec {
                name: "Owner".to_string(),
                agent_id: "agent-owner".to_string(),
                chat_session_id: Some(shared_session_id.clone()),
                description: None,
                input: Some("owner".to_string()),
                input_template: None,
                schedule: TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();

        let result = storage.create_task_from_spec(TaskSpec {
            name: "Reuser".to_string(),
            agent_id: "agent-owner".to_string(),
            chat_session_id: Some(shared_session_id.clone()),
            description: None,
            input: Some("reuse".to_string()),
            input_template: None,
            schedule: TaskSchedule::default(),
            execution_mode: None,
            timeout_secs: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        });

        let reused = result.unwrap();
        assert_eq!(owner_task.chat_session_id, shared_session_id);
        assert_eq!(reused.chat_session_id, shared_session_id);
        assert!(!owner_task.owns_chat_session);
        assert!(!reused.owns_chat_session);
    }

    #[test]
    fn test_pause_and_resume_task() {
        let storage = create_test_storage();

        let task = storage
            .create_task(
                "Test Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        // Pause the task
        let paused = storage.pause_task(&task.id).unwrap();
        assert_eq!(paused.status, TaskStatus::Paused);

        // Resume the task
        let resumed = storage.resume_task(&task.id).unwrap();
        assert_eq!(resumed.status, TaskStatus::Active);

        // Check events were recorded
        let events = storage.list_events_for_task(&task.id).unwrap();
        let event_types: Vec<_> = events.iter().map(|e| &e.event_type).collect();
        assert!(event_types.contains(&&TaskEventType::Paused));
        assert!(event_types.contains(&&TaskEventType::Resumed));
    }

    #[test]
    fn test_task_execution_lifecycle() {
        let storage = create_test_storage();

        let task = storage
            .create_task(
                "Test Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        // Start execution
        let running = storage.start_task_execution(&task.id).unwrap();
        assert_eq!(running.status, TaskStatus::Running);
        assert!(running.last_run_at.is_some());

        // Complete execution
        let completed = storage
            .complete_task_execution(&task.id, Some("Success output".to_string()), 1500)
            .unwrap();
        assert_eq!(completed.status, TaskStatus::Active);
        assert_eq!(completed.success_count, 1);

        // Check events
        let events = storage.list_events_for_task(&task.id).unwrap();
        let event_types: Vec<_> = events.iter().map(|e| &e.event_type).collect();
        assert!(event_types.contains(&&TaskEventType::Started));
        assert!(event_types.contains(&&TaskEventType::Completed));
    }

    #[test]
    fn test_start_task_execution_emits_started_event_once() {
        let storage = create_test_storage();

        let task = storage
            .create_task(
                "Test Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        let running = storage.start_task_execution(&task.id).unwrap();
        assert_eq!(running.status, TaskStatus::Running);

        let err = storage
            .start_task_execution(&task.id)
            .unwrap_err()
            .to_string();
        assert!(err.contains("cannot start from status"));

        let events = storage.list_events_for_task(&task.id).unwrap();
        let started_count = events
            .iter()
            .filter(|event| event.event_type == TaskEventType::Started)
            .count();
        assert_eq!(started_count, 1);
    }

    #[test]
    fn test_task_execution_failure() {
        let storage = create_test_storage();

        let task = storage
            .create_task(
                "Test Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        // Start and fail execution
        storage.start_task_execution(&task.id).unwrap();
        let failed = storage
            .fail_task_execution(&task.id, "Test error".to_string(), 500)
            .unwrap();

        assert_eq!(failed.status, TaskStatus::Failed);
        assert_eq!(failed.failure_count, 1);
        assert_eq!(failed.last_error, Some("Test error".to_string()));

        // Check events
        let events = storage.list_events_for_task(&task.id).unwrap();
        let failed_events: Vec<_> = events
            .iter()
            .filter(|e| e.event_type == TaskEventType::Failed)
            .collect();
        assert_eq!(failed_events.len(), 1);
        assert_eq!(failed_events[0].message, Some("Test error".to_string()));
    }

    #[test]
    fn test_start_task_execution_allows_retryable_failed_task() {
        let storage = create_test_storage();

        let task = storage
            .create_task(
                "Retryable Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Interval {
                    interval_ms: 60_000,
                    start_at: None,
                },
            )
            .unwrap();

        storage.start_task_execution(&task.id).unwrap();
        let failed = storage
            .fail_task_execution(&task.id, "retry me".to_string(), 500)
            .unwrap();
        assert_eq!(failed.status, TaskStatus::Failed);
        assert!(failed.next_run_at.is_some());

        let restarted = storage.start_task_execution(&task.id).unwrap();
        assert_eq!(restarted.status, TaskStatus::Running);
    }

    #[test]
    fn test_list_recent_events() {
        let storage = create_test_storage();

        let task = storage
            .create_task(
                "Test Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        // Add multiple events
        for i in 0..5 {
            let event = TaskEvent::new(task.id.clone(), TaskEventType::Started)
                .with_message(format!("Event {}", i));
            storage.add_event(&event).unwrap();
        }

        let recent = storage.list_recent_events_for_task(&task.id, 3).unwrap();
        assert_eq!(recent.len(), 3);
    }

    #[test]
    fn test_list_runnable_tasks() {
        let storage = create_test_storage();

        // Create a task with a past run time
        let past_time = chrono::Utc::now().timestamp_millis() - 10000;
        let task1 = storage
            .create_task(
                "Ready Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Once { run_at: past_time },
            )
            .unwrap();

        // Manually fix the next_run_at to be in the past
        let mut task1_updated = task1;
        task1_updated.next_run_at = Some(past_time);
        storage.update_task(&task1_updated).unwrap();

        // Create a task with a future run time
        let future_time = chrono::Utc::now().timestamp_millis() + 3600000;
        storage
            .create_task(
                "Future Task".to_string(),
                "agent-002".to_string(),
                TaskSchedule::Once {
                    run_at: future_time,
                },
            )
            .unwrap();

        let current_time = chrono::Utc::now().timestamp_millis();
        let runnable = storage.list_runnable_tasks(current_time).unwrap();

        assert_eq!(runnable.len(), 1);
        assert_eq!(runnable[0].name, "Ready Task");
    }

    #[test]
    fn test_list_runnable_tasks_repairs_missing_next_run_for_cron() {
        let storage = create_test_storage();

        let created = storage
            .create_task_from_spec(TaskSpec {
                name: "Cron Task".to_string(),
                agent_id: "agent-001".to_string(),
                chat_session_id: Some("session-1".to_string()),
                description: None,
                input: Some("hello".to_string()),
                input_template: None,
                schedule: TaskSchedule::Cron {
                    expression: "* * * * *".to_string(),
                    timezone: None,
                },
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();

        // Simulate legacy data where next_run_at was not computed.
        let mut broken = created.clone();
        broken.next_run_at = None;
        storage.update_task(&broken).unwrap();

        let now = chrono::Utc::now().timestamp_millis();
        let _ = storage.list_runnable_tasks(now).unwrap();

        let repaired = storage.get_task(&created.id).unwrap().unwrap();
        assert!(repaired.next_run_at.is_some());
    }

    #[test]
    fn test_list_runnable_tasks_repairs_stale_next_run() {
        let storage = create_test_storage();

        let created = storage
            .create_task_from_spec(TaskSpec {
                name: "Stale Task".to_string(),
                agent_id: "agent-001".to_string(),
                chat_session_id: Some("session-1".to_string()),
                description: None,
                input: Some("hello".to_string()),
                input_template: None,
                schedule: TaskSchedule::Interval {
                    interval_ms: 900_000,
                    start_at: None,
                },
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();

        // Simulate stale state: next_run_at is before last_run_at.
        // This happens when the daemon restarts mid-execution and
        // the completion handler doesn't persist the updated schedule.
        let now = chrono::Utc::now().timestamp_millis();
        let mut broken = created.clone();
        broken.next_run_at = Some(now - 3_600_000); // 1 hour ago
        broken.last_run_at = Some(now - 1_800_000); // 30 min ago (more recent)
        storage.update_task(&broken).unwrap();

        // Verify the stale condition
        let before = storage.get_task(&created.id).unwrap().unwrap();
        assert!(before.next_run_at.unwrap() < before.last_run_at.unwrap());

        // list_runnable_tasks should repair this
        let _ = storage.list_runnable_tasks(now).unwrap();

        let repaired = storage.get_task(&created.id).unwrap().unwrap();
        assert!(
            repaired.next_run_at.unwrap() > now,
            "next_run_at should be in the future after repair"
        );
    }

    #[test]
    fn test_repair_runnable_task_does_not_overwrite_paused_status_from_stale_snapshot() {
        let storage = create_test_storage();

        let created = storage
            .create_task_from_spec(TaskSpec {
                name: "Pause Race Guard".to_string(),
                agent_id: "agent-001".to_string(),
                chat_session_id: Some("session-1".to_string()),
                description: None,
                input: Some("hello".to_string()),
                input_template: None,
                schedule: TaskSchedule::Interval {
                    interval_ms: 900_000,
                    start_at: None,
                },
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();

        let now = chrono::Utc::now().timestamp_millis();
        let mut stale_active_snapshot = storage.get_task(&created.id).unwrap().unwrap();
        stale_active_snapshot.next_run_at = Some(now - 3_600_000);
        stale_active_snapshot.last_run_at = Some(now - 1_800_000);
        storage.update_task(&stale_active_snapshot).unwrap();
        let stale_active_snapshot = storage.get_task(&created.id).unwrap().unwrap();

        storage.pause_task(&created.id).unwrap();
        let paused_before_repair = storage.get_task(&created.id).unwrap().unwrap();
        assert_eq!(paused_before_repair.status, TaskStatus::Paused);

        let repaired = storage
            .repair_runnable_task_if_needed(stale_active_snapshot)
            .unwrap();
        assert!(repaired.is_none());

        let after = storage.get_task(&created.id).unwrap().unwrap();
        assert_eq!(after.status, TaskStatus::Paused);
        assert_eq!(after.next_run_at, paused_before_repair.next_run_at);
        assert_eq!(after.last_run_at, paused_before_repair.last_run_at);
    }

    #[test]
    fn test_list_runnable_tasks_repairs_stale_task_with_duplicate_session_binding() {
        let storage = create_test_storage();

        let created = storage
            .create_task_from_spec(TaskSpec {
                name: "Repair Failure".to_string(),
                agent_id: "agent-001".to_string(),
                chat_session_id: Some("session-1".to_string()),
                description: None,
                input: Some("hello".to_string()),
                input_template: None,
                schedule: TaskSchedule::Interval {
                    interval_ms: 3_600_000,
                    start_at: None,
                },
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();

        // Create stale schedule state that requires repair before scheduling can continue.
        let now = chrono::Utc::now().timestamp_millis();
        let mut broken = storage.get_task(&created.id).unwrap().unwrap();
        broken.next_run_at = Some(now - 5_000);
        broken.last_run_at = Some(now - 1_000);
        storage.update_task(&broken).unwrap();
        assert!(broken.should_run(now));

        // Create another runnable task to prove list_runnable_tasks still returns other tasks.
        let ready = storage
            .create_task(
                "Control Runnable".to_string(),
                "agent-001".to_string(),
                TaskSchedule::Interval {
                    interval_ms: 60_000,
                    start_at: None,
                },
            )
            .unwrap();
        let mut ready_task = storage.get_task(&ready.id).unwrap().unwrap();
        ready_task.next_run_at = Some(now - 10_000);
        storage.update_task(&ready_task).unwrap();
        assert!(ready_task.should_run(now));

        // Duplicate chat-session bindings are a service-layer policy concern.
        // Storage repair persists the task record without consulting session state.
        let mut conflicting = broken.clone();
        conflicting.id = format!("conflict-{}", Uuid::new_v4());
        conflicting.status = TaskStatus::Paused;
        let conflicting_raw = serde_json::to_vec(&conflicting).unwrap();
        storage
            .inner
            .put_task_raw(&conflicting.id, &conflicting_raw)
            .unwrap();

        let runnable = storage.list_runnable_tasks(now).unwrap();
        assert!(!runnable.iter().any(|task| task.id == created.id));
        assert!(runnable.iter().any(|task| task.id == ready.id));

        let after = storage.get_task(&created.id).unwrap().unwrap();
        assert_ne!(after.next_run_at, broken.next_run_at);
        assert_eq!(after.last_run_at, broken.last_run_at);
    }

    #[test]
    fn test_get_nonexistent_task() {
        let storage = create_test_storage();

        let result = storage.get_task("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_update_nonexistent_task_returns_error() {
        use crate::models::TaskSchedule;
        let storage = create_test_storage();
        let task = Task::new(
            "nonexistent".to_string(),
            "Ghost".to_string(),
            "agent-000".to_string(),
            TaskSchedule::Once {
                run_at: chrono::Utc::now().timestamp_millis() + 60_000,
            },
        );
        let result = storage.update_task(&task);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_pause_nonexistent_task() {
        let storage = create_test_storage();

        let result = storage.pause_task("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_task_lifecycle() {
        let storage = create_test_storage();

        let created = storage
            .create_task_from_spec(TaskSpec {
                name: "BG Agent".to_string(),
                agent_id: "agent-001".to_string(),
                chat_session_id: Some("session-1".to_string()),
                description: Some("Background agent".to_string()),
                input: Some("Run checks".to_string()),
                input_template: None,
                schedule: TaskSchedule::Interval {
                    interval_ms: 60_000,
                    start_at: None,
                },
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();
        assert_eq!(created.name, "BG Agent");

        let updated = storage
            .update_task_from_patch(
                &created.id,
                TaskPatch {
                    name: Some("BG Agent Updated".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(updated.name, "BG Agent Updated");

        let paused = storage
            .control_task(&created.id, TaskControlAction::Pause)
            .unwrap();
        assert_eq!(paused.status, TaskStatus::Paused);

        let resumed = storage
            .control_task(&created.id, TaskControlAction::Resume)
            .unwrap();
        assert_eq!(resumed.status, TaskStatus::Active);

        let run_now = storage
            .control_task(&created.id, TaskControlAction::RunNow)
            .unwrap();
        assert_eq!(run_now.status, TaskStatus::Active);
        assert!(run_now.next_run_at.is_some());

        let started = storage
            .control_task(&created.id, TaskControlAction::Start)
            .unwrap();
        assert_eq!(started.status, TaskStatus::Active);
        assert!(started.next_run_at.is_some());

        let stopped = storage
            .control_task(&created.id, TaskControlAction::Stop)
            .unwrap();
        assert_eq!(stopped.status, TaskStatus::Interrupted);
    }

    #[test]
    fn test_background_message_queue_and_progress() {
        let storage = create_test_storage();
        let task = storage
            .create_task(
                "Message Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        let queued = storage
            .send_task_message(
                &task.id,
                "Please also verify logs".to_string(),
                TaskMessageSource::User,
            )
            .unwrap();
        assert_eq!(queued.status, TaskMessageStatus::Queued);

        let pending = storage.list_pending_task_messages(&task.id, 10).unwrap();
        assert_eq!(pending.len(), 1);

        let delivered = storage
            .mark_task_message_delivered(&queued.id)
            .unwrap()
            .unwrap();
        assert_eq!(delivered.status, TaskMessageStatus::Delivered);

        let consumed = storage
            .mark_task_message_consumed(&queued.id)
            .unwrap()
            .unwrap();
        assert_eq!(consumed.status, TaskMessageStatus::Consumed);

        let progress = storage.get_task_progress(&task.id, 5).unwrap();
        assert_eq!(progress.task_id, task.id);
        assert_eq!(progress.pending_message_count, 0);
    }

    #[test]
    fn test_log_task_reply_is_not_queued() {
        let storage = create_test_storage();
        let task = storage
            .create_task(
                "Reply Task".to_string(),
                "agent-001".to_string(),
                TaskSchedule::default(),
            )
            .unwrap();

        let reply = storage.log_task_reply(&task.id, "ack".to_string()).unwrap();
        assert_eq!(reply.source, TaskMessageSource::Agent);
        assert_eq!(reply.status, TaskMessageStatus::Consumed);
        assert!(reply.delivered_at.is_some());
        assert!(reply.consumed_at.is_some());

        let pending = storage.list_pending_task_messages(&task.id, 10).unwrap();
        assert!(pending.is_empty());
    }

    #[test]
    fn test_create_task_with_template() {
        let storage = create_test_storage();
        let created = storage
            .create_task_from_spec(TaskSpec {
                name: "Templated Task".to_string(),
                agent_id: "agent-001".to_string(),
                chat_session_id: Some("session-1".to_string()),
                description: None,
                input: Some("fallback".to_string()),
                input_template: Some("Run task {{task.id}}".to_string()),
                schedule: TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();

        assert_eq!(
            created.input_template.as_deref(),
            Some("Run task {{task.id}}")
        );
    }

    #[test]
    fn test_create_task_requires_explicit_chat_session_binding() {
        let storage = create_test_storage();
        let result = storage.create_task_from_spec(TaskSpec {
            name: "Bound Session Task".to_string(),
            agent_id: "agent-001".to_string(),
            chat_session_id: None,
            description: None,
            input: Some("Run with auto session".to_string()),
            input_template: None,
            schedule: TaskSchedule::default(),
            execution_mode: None,
            timeout_secs: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        });

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("task must be bound to a chat session")
        );
    }

    #[test]
    fn test_create_task_accepts_raw_chat_session_binding() {
        let storage = create_test_storage();
        let foreign_session_id = "foreign-session".to_string();

        let task = storage
            .create_task_from_spec(TaskSpec {
                name: "Reject Foreign Session".to_string(),
                agent_id: "agent-001".to_string(),
                chat_session_id: Some(foreign_session_id.clone()),
                description: None,
                input: Some("Run".to_string()),
                input_template: None,
                schedule: TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();

        assert_eq!(task.chat_session_id, foreign_session_id);
        assert!(!task.owns_chat_session);
    }

    #[test]
    fn test_update_task_agent_change_preserves_chat_session_binding() {
        let storage = create_test_storage();
        let created = storage
            .create_task_from_spec(TaskSpec {
                name: "Rebind Session Task".to_string(),
                agent_id: "agent-001".to_string(),
                chat_session_id: Some("session-1".to_string()),
                description: None,
                input: Some("Run".to_string()),
                input_template: None,
                schedule: TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();
        let original_session_id = created.chat_session_id.clone();

        let updated = storage
            .update_task_from_patch(
                &created.id,
                TaskPatch {
                    agent_id: Some("agent-002".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(updated.agent_id, "agent-002");
        assert_eq!(updated.chat_session_id, original_session_id);
        assert!(!updated.owns_chat_session);
    }

    #[test]
    fn test_update_task_accepts_raw_reused_chat_session_binding() {
        let storage = create_test_storage();
        let owner = storage
            .create_task_from_spec(TaskSpec {
                name: "Owner".to_string(),
                agent_id: "agent-001".to_string(),
                chat_session_id: Some("shared-session".to_string()),
                description: None,
                input: Some("Owner input".to_string()),
                input_template: None,
                schedule: TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();

        let other = storage
            .create_task_from_spec(TaskSpec {
                name: "Other".to_string(),
                agent_id: "agent-001".to_string(),
                chat_session_id: Some("other-session".to_string()),
                description: None,
                input: Some("Other input".to_string()),
                input_template: None,
                schedule: TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();

        let updated = storage
            .update_task_from_patch(
                &other.id,
                TaskPatch {
                    chat_session_id: Some(owner.chat_session_id.clone()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(updated.chat_session_id, owner.chat_session_id);
        assert!(!updated.owns_chat_session);
    }

    #[test]
    fn test_update_task_updates_template() {
        let storage = create_test_storage();
        let created = storage
            .create_task_from_spec(TaskSpec {
                name: "Updatable Task".to_string(),
                agent_id: "agent-001".to_string(),
                chat_session_id: Some("session-1".to_string()),
                description: None,
                input: Some("Fallback task input".to_string()),
                input_template: None,
                schedule: TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();

        let updated = storage
            .update_task_from_patch(
                &created.id,
                TaskPatch {
                    input_template: Some("Template {{task.name}}".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(
            updated.input_template.as_deref(),
            Some("Template {{task.name}}")
        );
    }

    #[test]
    fn test_create_task_rejects_timeout_below_minimum() {
        let storage = create_test_storage();
        let result = storage.create_task_from_spec(TaskSpec {
            name: "Too Fast Timeout".to_string(),
            agent_id: "agent-001".to_string(),
            chat_session_id: Some("session-1".to_string()),
            description: None,
            input: Some("Run timeout validation".to_string()),
            input_template: None,
            schedule: TaskSchedule::default(),
            execution_mode: None,
            timeout_secs: Some(5),
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        });

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("timeout_secs must be at least")
        );
    }

    #[test]
    fn test_update_task_updates_timeout_secs() {
        let storage = create_test_storage();
        let created = storage
            .create_task_from_spec(TaskSpec {
                name: "Timeout Update Task".to_string(),
                agent_id: "agent-001".to_string(),
                chat_session_id: Some("session-1".to_string()),
                description: None,
                input: Some("Run timeout update".to_string()),
                input_template: None,
                schedule: TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();

        let updated = storage
            .update_task_from_patch(
                &created.id,
                TaskPatch {
                    timeout_secs: Some(900),
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(updated.timeout_secs, Some(900));
    }

    #[test]
    fn test_task_resource_limits_roundtrip() {
        use crate::models::ResourceLimits;

        let storage = create_test_storage();
        let created = storage
            .create_task_from_spec(TaskSpec {
                name: "Resource Limits Task".to_string(),
                agent_id: "agent-001".to_string(),
                chat_session_id: Some("session-1".to_string()),
                description: None,
                input: Some("Run resource limit roundtrip".to_string()),
                input_template: None,
                schedule: TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: Some(ResourceLimits {
                    max_tool_calls: 12,
                    max_duration_secs: 90,
                    max_output_bytes: 2048,
                    max_cost_usd: Some(1.25),
                }),
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();

        let created_limits = created.resource_limits.as_ref().expect("resource limits");
        assert_eq!(created_limits.max_tool_calls, 12);
        assert_eq!(created_limits.max_duration_secs, 90);
        assert_eq!(created_limits.max_output_bytes, 2048);
        assert_eq!(created_limits.max_cost_usd, Some(1.25));

        let updated = storage
            .update_task_from_patch(
                &created.id,
                TaskPatch {
                    resource_limits: Some(ResourceLimits {
                        max_tool_calls: 34,
                        max_duration_secs: 120,
                        max_output_bytes: 4096,
                        max_cost_usd: Some(2.5),
                    }),
                    ..Default::default()
                },
            )
            .unwrap();

        let updated_limits = updated.resource_limits.as_ref().expect("resource limits");
        assert_eq!(updated_limits.max_tool_calls, 34);
        assert_eq!(updated_limits.max_duration_secs, 120);
        assert_eq!(updated_limits.max_output_bytes, 4096);
        assert_eq!(updated_limits.max_cost_usd, Some(2.5));
    }

    #[test]
    fn test_task_without_resource_limits_stays_unlimited_by_task_defaults() {
        let storage = create_test_storage();
        let created = storage
            .create_task_from_spec(TaskSpec {
                name: "No Resource Limits Task".to_string(),
                agent_id: "agent-001".to_string(),
                chat_session_id: Some("session-1".to_string()),
                description: None,
                input: Some("Run without task limits".to_string()),
                input_template: None,
                schedule: TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();

        assert_eq!(created.resource_limits, None);
    }

    #[test]
    fn test_task_continuation_roundtrip() {
        use crate::models::ContinuationConfig;

        let storage = create_test_storage();
        let created = storage
            .create_task_from_spec(TaskSpec {
                name: "Continuation Task".to_string(),
                agent_id: "agent-001".to_string(),
                chat_session_id: Some("session-1".to_string()),
                description: None,
                input: Some("Run continuation roundtrip".to_string()),
                input_template: None,
                schedule: TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: Some(ContinuationConfig {
                    enabled: true,
                    segment_iterations: 40,
                    max_total_iterations: 800,
                    max_total_cost_usd: Some(4.5),
                    inter_segment_pause_ms: 250,
                }),
            })
            .unwrap();

        assert!(created.continuation.enabled);
        assert_eq!(created.continuation.segment_iterations, 40);
        assert_eq!(created.continuation.max_total_iterations, 800);
        assert_eq!(created.continuation.max_total_cost_usd, Some(4.5));
        assert_eq!(created.continuation.inter_segment_pause_ms, 250);
        assert_eq!(created.continuation_total_iterations, 0);
        assert_eq!(created.continuation_segments_completed, 0);

        let mut advanced = created.clone();
        advanced.continuation_total_iterations = 120;
        advanced.continuation_segments_completed = 3;
        storage.update_task(&advanced).unwrap();

        let updated = storage
            .update_task_from_patch(
                &created.id,
                TaskPatch {
                    continuation: Some(ContinuationConfig {
                        enabled: true,
                        segment_iterations: 60,
                        max_total_iterations: 1_200,
                        max_total_cost_usd: Some(6.0),
                        inter_segment_pause_ms: 500,
                    }),
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(updated.continuation.segment_iterations, 60);
        assert_eq!(updated.continuation.max_total_iterations, 1_200);
        assert_eq!(updated.continuation.max_total_cost_usd, Some(6.0));
        assert_eq!(updated.continuation.inter_segment_pause_ms, 500);
        assert_eq!(updated.continuation_total_iterations, 0);
        assert_eq!(updated.continuation_segments_completed, 0);
    }

    #[test]
    fn test_create_task_rejects_missing_input_and_template() {
        let storage = create_test_storage();
        let result = storage.create_task_from_spec(TaskSpec {
            name: "Missing Input".to_string(),
            agent_id: "agent-001".to_string(),
            chat_session_id: Some("session-1".to_string()),
            description: None,
            input: None,
            input_template: None,
            schedule: TaskSchedule::default(),
            execution_mode: None,
            timeout_secs: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        });

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("requires non-empty input or input_template")
        );
    }

    #[test]
    fn test_update_task_rejects_empty_input_and_template() {
        let storage = create_test_storage();
        let created = storage
            .create_task_from_spec(TaskSpec {
                name: "Mutable Input".to_string(),
                agent_id: "agent-001".to_string(),
                chat_session_id: Some("session-1".to_string()),
                description: None,
                input: Some("Initial input".to_string()),
                input_template: Some("Template {{task.name}}".to_string()),
                schedule: TaskSchedule::default(),
                execution_mode: None,
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .unwrap();

        let result = storage.update_task_from_patch(
            &created.id,
            TaskPatch {
                input: Some("".to_string()),
                input_template: Some("   ".to_string()),
                ..Default::default()
            },
        );

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("requires non-empty input or input_template")
        );
    }

    #[test]
    fn test_create_task_allows_task_input_template_when_input_exists() {
        let storage = create_test_storage();
        let result = storage.create_task_from_spec(TaskSpec {
            name: "Task Input Template".to_string(),
            agent_id: "agent-001".to_string(),
            chat_session_id: Some("session-1".to_string()),
            description: None,
            input: Some("Use fallback".to_string()),
            input_template: Some("{{task.input}}".to_string()),
            schedule: TaskSchedule::default(),
            execution_mode: None,
            timeout_secs: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        });

        assert!(result.is_ok());
    }

    #[test]
    fn test_create_task_rejects_template_that_renders_empty_without_fallback() {
        let storage = create_test_storage();
        let result = storage.create_task_from_spec(TaskSpec {
            name: "Empty Template".to_string(),
            agent_id: "agent-001".to_string(),
            chat_session_id: Some("session-1".to_string()),
            description: None,
            input: None,
            input_template: Some("{{task.input}}".to_string()),
            schedule: TaskSchedule::default(),
            execution_mode: None,
            timeout_secs: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        });

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("requires non-empty input or input_template")
        );
    }

    #[test]
    fn test_create_task_keeps_non_empty_template_compatibility() {
        let storage = create_test_storage();
        let result = storage.create_task_from_spec(TaskSpec {
            name: "Template Compatibility".to_string(),
            agent_id: "agent-001".to_string(),
            chat_session_id: Some("session-1".to_string()),
            description: None,
            input: None,
            input_template: Some("Task {{task.name}}".to_string()),
            schedule: TaskSchedule::default(),
            execution_mode: None,
            timeout_secs: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        });

        assert!(result.is_ok());
    }
}
