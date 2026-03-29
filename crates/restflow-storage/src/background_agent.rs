//! Agent Task storage - byte-level API for agent task persistence.
//!
//! Provides low-level storage operations for scheduled agent tasks and their
//! execution events using the redb embedded database.

use anyhow::Result;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::sync::Arc;

use crate::range_utils::prefix_range;

const BACKGROUND_AGENT_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("background_agents");
const BACKGROUND_AGENT_EVENT_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("background_agent_events");
/// Index table: task_id -> event_id (for listing events by task)
const BACKGROUND_AGENT_EVENT_INDEX_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("background_agent_event_index");
/// Index table: status:task_id -> task_id (for listing tasks by status)
const BACKGROUND_AGENT_STATUS_INDEX_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("background_agent_status_index");
/// Reverse index: task_id -> status:task_id (for direct status cleanup)
const BACKGROUND_AGENT_STATUS_LOOKUP_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("background_agent_status_lookup");
/// Background execution attempt payload table.
const BACKGROUND_AGENT_RUN_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("background_agent_runs");
/// Index table: task_id:run_id -> run_id
const BACKGROUND_AGENT_RUN_TASK_INDEX_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("background_agent_run_task_index");
/// Index table: task_id -> run_id for the single active run of one task.
const BACKGROUND_AGENT_ACTIVE_RUN_INDEX_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("background_agent_active_run_index");
/// Background message payload table
const BACKGROUND_MESSAGE_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("background_messages");
/// Index table: task_id:message_id -> message_id
const BACKGROUND_MESSAGE_TASK_INDEX_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("background_message_task_index");
/// Index table: status:task_id:message_id -> message_id
const BACKGROUND_MESSAGE_STATUS_INDEX_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("background_message_status_index");
/// Reverse index: message_id -> status:task_id:message_id
const BACKGROUND_MESSAGE_STATUS_LOOKUP_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("background_message_status_lookup");

/// Low-level agent task storage with byte-level API
#[derive(Clone)]
pub struct BackgroundAgentStorage {
    db: Arc<Database>,
}

impl BackgroundAgentStorage {
    fn parse_chat_session_id(data: &[u8]) -> Result<Option<String>> {
        let value: serde_json::Value =
            serde_json::from_slice(data).map_err(|error| anyhow::anyhow!("{}", error))?;
        let Some(raw_session_id) = value.get("chat_session_id") else {
            return Ok(None);
        };
        let session_id = raw_session_id
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("chat_session_id is not a string"))?
            .trim();
        if session_id.is_empty() {
            Ok(None)
        } else {
            Ok(Some(session_id.to_string()))
        }
    }

    fn extract_chat_session_id(data: &[u8]) -> Option<String> {
        Self::parse_chat_session_id(data).ok().flatten()
    }

    fn extract_task_name(data: &[u8]) -> Option<String> {
        let value: serde_json::Value = serde_json::from_slice(data).ok()?;
        value
            .get("name")
            .and_then(|name| name.as_str())
            .map(str::to_string)
    }

    fn parse_task_status(data: &[u8]) -> Result<String> {
        let value: serde_json::Value =
            serde_json::from_slice(data).map_err(|error| anyhow::anyhow!("{}", error))?;
        let status = value
            .get("status")
            .and_then(|status| status.as_str())
            .ok_or_else(|| anyhow::anyhow!("status is missing or not a string"))?;
        Ok(status.to_string())
    }

    fn parse_run_task_id(data: &[u8]) -> Result<String> {
        let value: serde_json::Value =
            serde_json::from_slice(data).map_err(|error| anyhow::anyhow!("{}", error))?;
        let task_id = value
            .get("task_id")
            .and_then(|task_id| task_id.as_str())
            .ok_or_else(|| anyhow::anyhow!("task_id is missing or not a string"))?;
        Ok(task_id.to_string())
    }

    fn parse_run_status(data: &[u8]) -> Result<String> {
        let value: serde_json::Value =
            serde_json::from_slice(data).map_err(|error| anyhow::anyhow!("{}", error))?;
        let status = value
            .get("status")
            .and_then(|status| status.as_str())
            .ok_or_else(|| anyhow::anyhow!("status is missing or not a string"))?;
        Ok(status.to_string())
    }

    fn validate_run_payload(run_id: &str, task_id: &str, status: &str, data: &[u8]) -> Result<()> {
        let payload_task_id = Self::parse_run_task_id(data)?;
        if payload_task_id != task_id {
            anyhow::bail!(
                "background run '{}' payload task_id '{}' does not match '{}'",
                run_id,
                payload_task_id,
                task_id
            );
        }

        let payload_status = Self::parse_run_status(data)?;
        if payload_status != status {
            anyhow::bail!(
                "background run '{}' payload status '{}' does not match '{}'",
                run_id,
                payload_status,
                status
            );
        }

        Ok(())
    }

    fn ensure_task_exists(
        table: &redb::Table<&str, &[u8]>,
        task_id: &str,
        run_id: &str,
    ) -> Result<()> {
        if table.get(task_id)?.is_none() {
            anyhow::bail!(
                "background run '{}' references missing background task '{}'",
                run_id,
                task_id
            );
        }
        Ok(())
    }

    fn reconcile_active_run_slot(
        active_index: &mut redb::Table<&str, &str>,
        run_table: &redb::Table<&str, &[u8]>,
        task_id: &str,
        run_id: &str,
        status: &str,
    ) -> Result<()> {
        let wants_active = status == "running";
        let active_entry = active_index
            .get(task_id)?
            .map(|value| value.value().to_string());

        if let Some(existing_run_id) = active_entry {
            if existing_run_id != run_id {
                if let Some(existing_raw) = run_table.get(existing_run_id.as_str())? {
                    let existing_task_id = Self::parse_run_task_id(existing_raw.value())?;
                    let existing_status = Self::parse_run_status(existing_raw.value())?;
                    if existing_task_id == task_id && existing_status == "running" {
                        if wants_active {
                            anyhow::bail!(
                                "background task '{}' already has active run '{}'",
                                task_id,
                                existing_run_id
                            );
                        }
                        return Ok(());
                    }
                }
                active_index.remove(task_id)?;
            } else if !wants_active {
                active_index.remove(task_id)?;
                return Ok(());
            }
        } else if !wants_active {
            return Ok(());
        }

        if wants_active {
            active_index.insert(task_id, run_id)?;
        }

        Ok(())
    }

    fn ensure_unique_chat_session_binding(
        table: &redb::Table<&str, &[u8]>,
        task_id: &str,
        target_chat_session_id: Option<&str>,
    ) -> Result<()> {
        let Some(target_chat_session_id) = target_chat_session_id else {
            return Ok(());
        };

        for item in table.iter()? {
            let (key, value) = item?;
            let existing_task_id = key.value();
            if existing_task_id == task_id {
                continue;
            }

            let existing_chat_session_id = Self::parse_chat_session_id(value.value()).map_err(
                |error| {
                    anyhow::anyhow!(
                        "failed to parse existing background task '{}' while validating chat_session_id uniqueness: {}",
                        existing_task_id,
                        error
                    )
                },
            )?;
            if existing_chat_session_id.as_deref() != Some(target_chat_session_id) {
                continue;
            }

            let existing_task_name =
                Self::extract_task_name(value.value()).unwrap_or_else(|| "unknown".to_string());
            return Err(anyhow::anyhow!(
                "chat_session_id '{}' is already bound to background task '{}' ({})",
                target_chat_session_id,
                existing_task_id,
                existing_task_name
            ));
        }

        Ok(())
    }

    /// Create a new BackgroundAgentStorage instance
    pub fn new(db: Arc<Database>) -> Result<Self> {
        // Initialize all tables
        let write_txn = db.begin_write()?;
        write_txn.open_table(BACKGROUND_AGENT_TABLE)?;
        write_txn.open_table(BACKGROUND_AGENT_EVENT_TABLE)?;
        write_txn.open_table(BACKGROUND_AGENT_EVENT_INDEX_TABLE)?;
        write_txn.open_table(BACKGROUND_AGENT_STATUS_INDEX_TABLE)?;
        write_txn.open_table(BACKGROUND_AGENT_STATUS_LOOKUP_TABLE)?;
        write_txn.open_table(BACKGROUND_AGENT_RUN_TABLE)?;
        write_txn.open_table(BACKGROUND_AGENT_RUN_TASK_INDEX_TABLE)?;
        write_txn.open_table(BACKGROUND_AGENT_ACTIVE_RUN_INDEX_TABLE)?;
        write_txn.open_table(BACKGROUND_MESSAGE_TABLE)?;
        write_txn.open_table(BACKGROUND_MESSAGE_TASK_INDEX_TABLE)?;
        write_txn.open_table(BACKGROUND_MESSAGE_STATUS_INDEX_TABLE)?;
        write_txn.open_table(BACKGROUND_MESSAGE_STATUS_LOOKUP_TABLE)?;
        write_txn.commit()?;

        Ok(Self { db })
    }

    // ============== Agent Task Operations ==============

    /// Store raw agent task data
    pub fn put_task_raw(&self, id: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(BACKGROUND_AGENT_TABLE)?;
            table.insert(id, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Store raw agent task data with status index
    pub fn put_task_raw_with_status(&self, id: &str, status: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(BACKGROUND_AGENT_TABLE)?;
            let chat_session_id = Self::extract_chat_session_id(data);
            Self::ensure_unique_chat_session_binding(&table, id, chat_session_id.as_deref())?;
            table.insert(id, data)?;

            let mut status_index = write_txn.open_table(BACKGROUND_AGENT_STATUS_INDEX_TABLE)?;
            let mut status_lookup = write_txn.open_table(BACKGROUND_AGENT_STATUS_LOOKUP_TABLE)?;
            if let Some(previous_key) = status_lookup.get(id)? {
                status_index.remove(previous_key.value())?;
            }

            let status_key = format!("{}:{}", status, id);
            status_index.insert(status_key.as_str(), id)?;
            status_lookup.insert(id, status_key.as_str())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Update raw agent task data while keeping the status index consistent
    pub fn update_task_raw_with_status(
        &self,
        id: &str,
        old_status: &str,
        new_status: &str,
        data: &[u8],
    ) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(BACKGROUND_AGENT_TABLE)?;
            let chat_session_id = Self::extract_chat_session_id(data);
            Self::ensure_unique_chat_session_binding(&table, id, chat_session_id.as_deref())?;
            table.insert(id, data)?;

            let mut status_index = write_txn.open_table(BACKGROUND_AGENT_STATUS_INDEX_TABLE)?;
            let mut status_lookup = write_txn.open_table(BACKGROUND_AGENT_STATUS_LOOKUP_TABLE)?;
            if let Some(previous_key) = status_lookup.get(id)? {
                status_index.remove(previous_key.value())?;
            } else if old_status != new_status {
                let old_key = format!("{}:{}", old_status, id);
                status_index.remove(old_key.as_str())?;
            }

            let new_key = format!("{}:{}", new_status, id);
            status_index.insert(new_key.as_str(), id)?;
            status_lookup.insert(id, new_key.as_str())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Compare-and-set update for task payload and status index.
    ///
    /// Returns `Ok(true)` when the record existed and its current payload status
    /// matched `expected_status`, so the update was committed.
    ///
    /// Returns `Ok(false)` when the record does not exist or its current payload
    /// status does not match `expected_status`.
    pub fn update_task_raw_if_status_matches(
        &self,
        id: &str,
        expected_status: &str,
        new_status: &str,
        data: &[u8],
    ) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let mut table = write_txn.open_table(BACKGROUND_AGENT_TABLE)?;
        let Some(existing) = table.get(id)? else {
            drop(table);
            write_txn.abort()?;
            return Ok(false);
        };

        let current_status = Self::parse_task_status(existing.value())?;
        drop(existing);
        if current_status != expected_status {
            drop(table);
            write_txn.abort()?;
            return Ok(false);
        }

        let chat_session_id = Self::extract_chat_session_id(data);
        Self::ensure_unique_chat_session_binding(&table, id, chat_session_id.as_deref())?;
        table.insert(id, data)?;
        drop(table);

        let mut status_index = write_txn.open_table(BACKGROUND_AGENT_STATUS_INDEX_TABLE)?;
        let mut status_lookup = write_txn.open_table(BACKGROUND_AGENT_STATUS_LOOKUP_TABLE)?;
        if let Some(previous_key) = status_lookup.get(id)? {
            status_index.remove(previous_key.value())?;
        } else if current_status != new_status {
            let old_key = format!("{}:{}", current_status, id);
            status_index.remove(old_key.as_str())?;
        }

        let new_key = format!("{}:{}", new_status, id);
        status_index.insert(new_key.as_str(), id)?;
        status_lookup.insert(id, new_key.as_str())?;
        drop(status_lookup);
        drop(status_index);
        write_txn.commit()?;
        Ok(true)
    }

    /// Get raw agent task data by ID
    pub fn get_task_raw(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(BACKGROUND_AGENT_TABLE)?;

        if let Some(value) = table.get(id)? {
            Ok(Some(value.value().to_vec()))
        } else {
            Ok(None)
        }
    }

    /// List all raw agent task data
    pub fn list_tasks_raw(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(BACKGROUND_AGENT_TABLE)?;

        let mut tasks = Vec::new();
        for item in table.iter()? {
            let (key, value) = item?;
            tasks.push((key.value().to_string(), value.value().to_vec()));
        }

        Ok(tasks)
    }

    /// List tasks by status using the status index
    pub fn list_tasks_by_status_indexed(&self, status: &str) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let status_index = read_txn.open_table(BACKGROUND_AGENT_STATUS_INDEX_TABLE)?;
        let task_table = read_txn.open_table(BACKGROUND_AGENT_TABLE)?;

        let prefix = format!("{}:", status);
        let (start, end) = prefix_range(&prefix);
        let mut tasks = Vec::new();

        for item in status_index.range(start.as_str()..end.as_str())? {
            let (_, value) = item?;
            let task_id = value.value();
            if let Some(data) = task_table.get(task_id)? {
                tasks.push((task_id.to_string(), data.value().to_vec()));
            }
        }

        Ok(tasks)
    }

    /// Delete agent task by ID
    pub fn delete_task(&self, id: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let existed = {
            let mut table = write_txn.open_table(BACKGROUND_AGENT_TABLE)?;
            let existed = table.remove(id)?.is_some();

            let mut status_index = write_txn.open_table(BACKGROUND_AGENT_STATUS_INDEX_TABLE)?;
            let mut status_lookup = write_txn.open_table(BACKGROUND_AGENT_STATUS_LOOKUP_TABLE)?;
            let mut active_run_index =
                write_txn.open_table(BACKGROUND_AGENT_ACTIVE_RUN_INDEX_TABLE)?;
            if let Some(previous_key) = status_lookup.get(id)? {
                status_index.remove(previous_key.value())?;
            }
            status_lookup.remove(id)?;
            active_run_index.remove(id)?;

            existed
        };
        write_txn.commit()?;
        Ok(existed)
    }

    /// Delete agent task by ID with status index cleanup
    pub fn delete_task_with_status(&self, id: &str, status: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let existed = {
            let mut table = write_txn.open_table(BACKGROUND_AGENT_TABLE)?;
            let existed = table.remove(id)?.is_some();

            let mut status_index = write_txn.open_table(BACKGROUND_AGENT_STATUS_INDEX_TABLE)?;
            let mut status_lookup = write_txn.open_table(BACKGROUND_AGENT_STATUS_LOOKUP_TABLE)?;
            let mut active_run_index =
                write_txn.open_table(BACKGROUND_AGENT_ACTIVE_RUN_INDEX_TABLE)?;
            if let Some(previous_key) = status_lookup.get(id)? {
                status_index.remove(previous_key.value())?;
            } else {
                let status_key = format!("{}:{}", status, id);
                status_index.remove(status_key.as_str())?;
            }
            status_lookup.remove(id)?;
            active_run_index.remove(id)?;

            existed
        };
        write_txn.commit()?;
        Ok(existed)
    }

    /// Delete a task and all related task/message/event records atomically.
    pub fn delete_task_cascade(&self, id: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let existed = {
            let mut task_table = write_txn.open_table(BACKGROUND_AGENT_TABLE)?;
            let existed = task_table.get(id)?.is_some();

            let prefix = format!("{}:", id);
            let (start, end) = prefix_range(&prefix);

            let mut event_table = write_txn.open_table(BACKGROUND_AGENT_EVENT_TABLE)?;
            let mut event_index = write_txn.open_table(BACKGROUND_AGENT_EVENT_INDEX_TABLE)?;
            let mut event_keys = Vec::new();
            for item in event_index.range(start.as_str()..end.as_str())? {
                let (key, value) = item?;
                event_keys.push((key.value().to_string(), value.value().to_string()));
            }
            for (event_key, event_id) in event_keys {
                event_index.remove(event_key.as_str())?;
                event_table.remove(event_id.as_str())?;
            }

            let mut run_table = write_txn.open_table(BACKGROUND_AGENT_RUN_TABLE)?;
            let mut run_task_index = write_txn.open_table(BACKGROUND_AGENT_RUN_TASK_INDEX_TABLE)?;
            let mut active_run_index =
                write_txn.open_table(BACKGROUND_AGENT_ACTIVE_RUN_INDEX_TABLE)?;
            let mut run_keys = Vec::new();
            for item in run_task_index.range(start.as_str()..end.as_str())? {
                let (key, value) = item?;
                run_keys.push((key.value().to_string(), value.value().to_string()));
            }
            for (run_key, run_id) in run_keys {
                run_task_index.remove(run_key.as_str())?;
                run_table.remove(run_id.as_str())?;
            }
            active_run_index.remove(id)?;

            let mut message_table = write_txn.open_table(BACKGROUND_MESSAGE_TABLE)?;
            let mut message_task_index =
                write_txn.open_table(BACKGROUND_MESSAGE_TASK_INDEX_TABLE)?;
            let mut message_status_index =
                write_txn.open_table(BACKGROUND_MESSAGE_STATUS_INDEX_TABLE)?;
            let mut message_status_lookup =
                write_txn.open_table(BACKGROUND_MESSAGE_STATUS_LOOKUP_TABLE)?;
            let mut message_keys = Vec::new();
            for item in message_task_index.range(start.as_str()..end.as_str())? {
                let (key, value) = item?;
                message_keys.push((key.value().to_string(), value.value().to_string()));
            }
            for (message_key, message_id) in message_keys {
                message_task_index.remove(message_key.as_str())?;
                if let Some(status_key) = message_status_lookup.get(message_id.as_str())? {
                    message_status_index.remove(status_key.value())?;
                }
                message_status_lookup.remove(message_id.as_str())?;
                message_table.remove(message_id.as_str())?;
            }

            let mut task_status_index =
                write_txn.open_table(BACKGROUND_AGENT_STATUS_INDEX_TABLE)?;
            let mut task_status_lookup =
                write_txn.open_table(BACKGROUND_AGENT_STATUS_LOOKUP_TABLE)?;
            if let Some(status_key) = task_status_lookup.get(id)? {
                task_status_index.remove(status_key.value())?;
            }
            task_status_lookup.remove(id)?;

            if existed {
                task_table.remove(id)?;
            }

            existed
        };
        write_txn.commit()?;
        Ok(existed)
    }

    /// Store raw background run data with a task index entry.
    pub fn put_run_raw(&self, run_id: &str, task_id: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut run_table = write_txn.open_table(BACKGROUND_AGENT_RUN_TABLE)?;
            run_table.insert(run_id, data)?;

            let mut task_index = write_txn.open_table(BACKGROUND_AGENT_RUN_TASK_INDEX_TABLE)?;
            let task_key = format!("{}:{}", task_id, run_id);
            task_index.insert(task_key.as_str(), run_id)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Store raw background run data and keep the active-run index consistent.
    pub fn put_run_raw_with_status(
        &self,
        run_id: &str,
        task_id: &str,
        status: &str,
        data: &[u8],
    ) -> Result<()> {
        Self::validate_run_payload(run_id, task_id, status, data)?;

        let write_txn = self.db.begin_write()?;
        {
            let task_table = write_txn.open_table(BACKGROUND_AGENT_TABLE)?;
            Self::ensure_task_exists(&task_table, task_id, run_id)?;
            drop(task_table);

            let mut run_table = write_txn.open_table(BACKGROUND_AGENT_RUN_TABLE)?;
            let mut task_index = write_txn.open_table(BACKGROUND_AGENT_RUN_TASK_INDEX_TABLE)?;
            let mut active_index = write_txn.open_table(BACKGROUND_AGENT_ACTIVE_RUN_INDEX_TABLE)?;

            if let Some(existing) = run_table.get(run_id)? {
                let existing_task_id = Self::parse_run_task_id(existing.value())?;
                if existing_task_id != task_id {
                    anyhow::bail!(
                        "background run '{}' is indexed under task '{}', not '{}'",
                        run_id,
                        existing_task_id,
                        task_id
                    );
                }
            }

            Self::reconcile_active_run_slot(
                &mut active_index,
                &run_table,
                task_id,
                run_id,
                status,
            )?;
            run_table.insert(run_id, data)?;

            let task_key = format!("{}:{}", task_id, run_id);
            task_index.insert(task_key.as_str(), run_id)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Update raw background run data while preserving the task index.
    pub fn update_run_raw(&self, run_id: &str, task_id: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut run_table = write_txn.open_table(BACKGROUND_AGENT_RUN_TABLE)?;
            run_table.insert(run_id, data)?;

            let mut task_index = write_txn.open_table(BACKGROUND_AGENT_RUN_TASK_INDEX_TABLE)?;
            let task_key = format!("{}:{}", task_id, run_id);
            task_index.insert(task_key.as_str(), run_id)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Update raw background run data while preserving task index and active-run consistency.
    pub fn update_run_raw_with_status(
        &self,
        run_id: &str,
        task_id: &str,
        old_status: &str,
        new_status: &str,
        data: &[u8],
    ) -> Result<()> {
        Self::validate_run_payload(run_id, task_id, new_status, data)?;

        let write_txn = self.db.begin_write()?;
        {
            let task_table = write_txn.open_table(BACKGROUND_AGENT_TABLE)?;
            Self::ensure_task_exists(&task_table, task_id, run_id)?;
            drop(task_table);

            let mut run_table = write_txn.open_table(BACKGROUND_AGENT_RUN_TABLE)?;
            let current_status = match run_table.get(run_id)? {
                Some(existing) => {
                    let existing_task_id = Self::parse_run_task_id(existing.value())?;
                    if existing_task_id != task_id {
                        anyhow::bail!(
                            "background run '{}' is indexed under task '{}', not '{}'",
                            run_id,
                            existing_task_id,
                            task_id
                        );
                    }
                    Self::parse_run_status(existing.value())?
                }
                None => old_status.to_string(),
            };

            let mut task_index = write_txn.open_table(BACKGROUND_AGENT_RUN_TASK_INDEX_TABLE)?;
            let mut active_index = write_txn.open_table(BACKGROUND_AGENT_ACTIVE_RUN_INDEX_TABLE)?;

            Self::reconcile_active_run_slot(
                &mut active_index,
                &run_table,
                task_id,
                run_id,
                new_status,
            )?;
            run_table.insert(run_id, data)?;

            let task_key = format!("{}:{}", task_id, run_id);
            task_index.insert(task_key.as_str(), run_id)?;

            if current_status == "running"
                && new_status != "running"
                && active_index
                    .get(task_id)?
                    .is_some_and(|value| value.value() == run_id)
            {
                active_index.remove(task_id)?;
            }
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get raw background run data by ID.
    pub fn get_run_raw(&self, run_id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let run_table = read_txn.open_table(BACKGROUND_AGENT_RUN_TABLE)?;
        Ok(run_table.get(run_id)?.map(|value| value.value().to_vec()))
    }

    /// List all raw background run payloads.
    pub fn list_runs_raw(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let run_table = read_txn.open_table(BACKGROUND_AGENT_RUN_TABLE)?;
        let mut runs = Vec::new();
        for item in run_table.iter()? {
            let (key, value) = item?;
            runs.push((key.value().to_string(), value.value().to_vec()));
        }
        Ok(runs)
    }

    /// List raw background run payloads for one task.
    pub fn list_runs_by_task_raw(&self, task_id: &str) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let task_index = read_txn.open_table(BACKGROUND_AGENT_RUN_TASK_INDEX_TABLE)?;
        let run_table = read_txn.open_table(BACKGROUND_AGENT_RUN_TABLE)?;

        let prefix = format!("{}:", task_id);
        let (start, end) = prefix_range(&prefix);
        let mut runs = Vec::new();
        for item in task_index.range(start.as_str()..end.as_str())? {
            let (_, value) = item?;
            let run_id = value.value();
            if let Some(data) = run_table.get(run_id)? {
                runs.push((run_id.to_string(), data.value().to_vec()));
            }
        }
        Ok(runs)
    }

    /// Return the active run payload referenced by the task-level active-run index.
    pub fn get_active_run_raw(&self, task_id: &str) -> Result<Option<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let active_index = read_txn.open_table(BACKGROUND_AGENT_ACTIVE_RUN_INDEX_TABLE)?;
        let run_table = read_txn.open_table(BACKGROUND_AGENT_RUN_TABLE)?;

        let Some(run_id) = active_index.get(task_id)? else {
            return Ok(None);
        };
        let run_id = run_id.value().to_string();
        let Some(data) = run_table.get(run_id.as_str())? else {
            return Ok(None);
        };
        Ok(Some((run_id, data.value().to_vec())))
    }

    /// Remove one task-level active-run index entry.
    pub fn clear_active_run_raw(&self, task_id: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut active_index = write_txn.open_table(BACKGROUND_AGENT_ACTIVE_RUN_INDEX_TABLE)?;
            active_index.remove(task_id)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// List all active run payloads referenced by the task-level active-run index.
    pub fn list_active_runs_raw(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let active_index = read_txn.open_table(BACKGROUND_AGENT_ACTIVE_RUN_INDEX_TABLE)?;
        let run_table = read_txn.open_table(BACKGROUND_AGENT_RUN_TABLE)?;
        let mut runs = Vec::new();

        for item in active_index.iter()? {
            let (_, run_id) = item?;
            let run_id = run_id.value();
            if let Some(data) = run_table.get(run_id)? {
                runs.push((run_id.to_string(), data.value().to_vec()));
            }
        }

        Ok(runs)
    }

    // ============== Background Message Operations ==============

    /// Store raw background message data with task/status indices.
    pub fn put_background_message_raw_with_status(
        &self,
        message_id: &str,
        task_id: &str,
        status: &str,
        data: &[u8],
    ) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut message_table = write_txn.open_table(BACKGROUND_MESSAGE_TABLE)?;
            message_table.insert(message_id, data)?;

            let mut task_index = write_txn.open_table(BACKGROUND_MESSAGE_TASK_INDEX_TABLE)?;
            let task_key = format!("{}:{}", task_id, message_id);
            task_index.insert(task_key.as_str(), message_id)?;

            let mut status_index = write_txn.open_table(BACKGROUND_MESSAGE_STATUS_INDEX_TABLE)?;
            let mut status_lookup = write_txn.open_table(BACKGROUND_MESSAGE_STATUS_LOOKUP_TABLE)?;
            if let Some(previous_key) = status_lookup.get(message_id)? {
                status_index.remove(previous_key.value())?;
            }
            let status_key = format!("{}:{}:{}", status, task_id, message_id);
            status_index.insert(status_key.as_str(), message_id)?;
            status_lookup.insert(message_id, status_key.as_str())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Update raw background message data and keep status index consistent.
    pub fn update_background_message_raw_with_status(
        &self,
        message_id: &str,
        task_id: &str,
        old_status: &str,
        new_status: &str,
        data: &[u8],
    ) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut message_table = write_txn.open_table(BACKGROUND_MESSAGE_TABLE)?;
            message_table.insert(message_id, data)?;

            let mut status_index = write_txn.open_table(BACKGROUND_MESSAGE_STATUS_INDEX_TABLE)?;
            let mut status_lookup = write_txn.open_table(BACKGROUND_MESSAGE_STATUS_LOOKUP_TABLE)?;
            if let Some(previous_key) = status_lookup.get(message_id)? {
                status_index.remove(previous_key.value())?;
            } else if old_status != new_status {
                let old_key = format!("{}:{}:{}", old_status, task_id, message_id);
                status_index.remove(old_key.as_str())?;
            }

            let new_key = format!("{}:{}:{}", new_status, task_id, message_id);
            status_index.insert(new_key.as_str(), message_id)?;
            status_lookup.insert(message_id, new_key.as_str())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get raw background message data by ID.
    pub fn get_background_message_raw(&self, message_id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(BACKGROUND_MESSAGE_TABLE)?;

        if let Some(value) = table.get(message_id)? {
            Ok(Some(value.value().to_vec()))
        } else {
            Ok(None)
        }
    }

    /// List raw background messages for a task.
    pub fn list_background_messages_for_task_raw(
        &self,
        task_id: &str,
    ) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let task_index = read_txn.open_table(BACKGROUND_MESSAGE_TASK_INDEX_TABLE)?;
        let message_table = read_txn.open_table(BACKGROUND_MESSAGE_TABLE)?;

        let prefix = format!("{}:", task_id);
        let (start, end) = prefix_range(&prefix);
        let mut messages = Vec::new();

        for item in task_index.range(start.as_str()..end.as_str())? {
            let (_, value) = item?;
            let message_id = value.value();
            if let Some(data) = message_table.get(message_id)? {
                messages.push((message_id.to_string(), data.value().to_vec()));
            }
        }

        Ok(messages)
    }

    /// List raw background messages for a task by status.
    pub fn list_background_messages_by_status_for_task_raw(
        &self,
        task_id: &str,
        status: &str,
    ) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let status_index = read_txn.open_table(BACKGROUND_MESSAGE_STATUS_INDEX_TABLE)?;
        let message_table = read_txn.open_table(BACKGROUND_MESSAGE_TABLE)?;

        let prefix = format!("{}:{}:", status, task_id);
        let (start, end) = prefix_range(&prefix);
        let mut messages = Vec::new();

        for item in status_index.range(start.as_str()..end.as_str())? {
            let (_, value) = item?;
            let message_id = value.value();
            if let Some(data) = message_table.get(message_id)? {
                messages.push((message_id.to_string(), data.value().to_vec()));
            }
        }

        Ok(messages)
    }

    /// Delete one background message and related indices.
    pub fn delete_background_message(
        &self,
        message_id: &str,
        task_id: &str,
        status: &str,
    ) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let existed = {
            let mut message_table = write_txn.open_table(BACKGROUND_MESSAGE_TABLE)?;
            let existed = message_table.remove(message_id)?.is_some();

            let mut task_index = write_txn.open_table(BACKGROUND_MESSAGE_TASK_INDEX_TABLE)?;
            let task_key = format!("{}:{}", task_id, message_id);
            task_index.remove(task_key.as_str())?;

            let mut status_index = write_txn.open_table(BACKGROUND_MESSAGE_STATUS_INDEX_TABLE)?;
            let mut status_lookup = write_txn.open_table(BACKGROUND_MESSAGE_STATUS_LOOKUP_TABLE)?;
            if let Some(previous_key) = status_lookup.get(message_id)? {
                status_index.remove(previous_key.value())?;
            } else {
                let status_key = format!("{}:{}:{}", status, task_id, message_id);
                status_index.remove(status_key.as_str())?;
            }
            status_lookup.remove(message_id)?;

            existed
        };
        write_txn.commit()?;
        Ok(existed)
    }

    /// Delete all background messages for a task.
    pub fn delete_background_messages_for_task(&self, task_id: &str) -> Result<u32> {
        let messages = self.list_background_messages_for_task_raw(task_id)?;
        let count = messages.len() as u32;

        if count == 0 {
            return Ok(0);
        }

        let write_txn = self.db.begin_write()?;
        {
            let mut message_table = write_txn.open_table(BACKGROUND_MESSAGE_TABLE)?;
            let mut task_index = write_txn.open_table(BACKGROUND_MESSAGE_TASK_INDEX_TABLE)?;
            let mut status_index = write_txn.open_table(BACKGROUND_MESSAGE_STATUS_INDEX_TABLE)?;
            let mut status_lookup = write_txn.open_table(BACKGROUND_MESSAGE_STATUS_LOOKUP_TABLE)?;

            for (message_id, data) in &messages {
                message_table.remove(message_id.as_str())?;

                let task_key = format!("{}:{}", task_id, message_id);
                task_index.remove(task_key.as_str())?;

                if let Some(previous_key) = status_lookup.get(message_id.as_str())? {
                    status_index.remove(previous_key.value())?;
                } else if let Ok(value) = serde_json::from_slice::<serde_json::Value>(data)
                    && let Some(status) = value.get("status").and_then(|s| s.as_str())
                {
                    let status_key = format!("{}:{}:{}", status, task_id, message_id);
                    status_index.remove(status_key.as_str())?;
                }
                status_lookup.remove(message_id.as_str())?;
            }
        }
        write_txn.commit()?;
        Ok(count)
    }

    // ============== Task Event Operations ==============

    /// Store raw task event data with index
    pub fn put_event_raw(&self, event_id: &str, task_id: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut event_table = write_txn.open_table(BACKGROUND_AGENT_EVENT_TABLE)?;
            event_table.insert(event_id, data)?;

            // Create composite index key: task_id:timestamp:event_id for ordered retrieval
            let mut index_table = write_txn.open_table(BACKGROUND_AGENT_EVENT_INDEX_TABLE)?;
            let index_key = format!("{}:{}", task_id, event_id);
            index_table.insert(index_key.as_str(), event_id)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get raw task event data by ID
    pub fn get_event_raw(&self, event_id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(BACKGROUND_AGENT_EVENT_TABLE)?;

        if let Some(value) = table.get(event_id)? {
            Ok(Some(value.value().to_vec()))
        } else {
            Ok(None)
        }
    }

    /// List all events for a specific task
    pub fn list_events_for_task_raw(&self, task_id: &str) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let index_table = read_txn.open_table(BACKGROUND_AGENT_EVENT_INDEX_TABLE)?;
        let event_table = read_txn.open_table(BACKGROUND_AGENT_EVENT_TABLE)?;

        let prefix = format!("{}:", task_id);
        let (start, end) = prefix_range(&prefix);
        let mut events = Vec::new();

        for item in index_table.range(start.as_str()..end.as_str())? {
            let (_, value) = item?;
            let event_id = value.value();
            if let Some(event_data) = event_table.get(event_id)? {
                events.push((event_id.to_string(), event_data.value().to_vec()));
            }
        }

        Ok(events)
    }

    /// Delete a task event by ID
    pub fn delete_event(&self, event_id: &str, task_id: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let existed = {
            let mut event_table = write_txn.open_table(BACKGROUND_AGENT_EVENT_TABLE)?;
            let existed = event_table.remove(event_id)?.is_some();

            // Remove from index
            let mut index_table = write_txn.open_table(BACKGROUND_AGENT_EVENT_INDEX_TABLE)?;
            let index_key = format!("{}:{}", task_id, event_id);
            index_table.remove(index_key.as_str())?;

            existed
        };
        write_txn.commit()?;
        Ok(existed)
    }

    /// Delete all events for a specific task
    pub fn delete_events_for_task(&self, task_id: &str) -> Result<u32> {
        // First, collect all event IDs for this task
        let events = self.list_events_for_task_raw(task_id)?;
        let count = events.len() as u32;

        if count == 0 {
            return Ok(0);
        }

        let write_txn = self.db.begin_write()?;
        {
            let mut event_table = write_txn.open_table(BACKGROUND_AGENT_EVENT_TABLE)?;
            let mut index_table = write_txn.open_table(BACKGROUND_AGENT_EVENT_INDEX_TABLE)?;

            for (event_id, _) in &events {
                event_table.remove(event_id.as_str())?;
                let index_key = format!("{}:{}", task_id, event_id);
                index_table.remove(index_key.as_str())?;
            }
        }
        write_txn.commit()?;

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn task_payload(id: &str, name: &str, chat_session_id: &str) -> Vec<u8> {
        format!(
            r#"{{"id":"{}","name":"{}","chat_session_id":"{}"}}"#,
            id, name, chat_session_id
        )
        .into_bytes()
    }

    fn task_status_payload(id: &str, status: &str, chat_session_id: &str) -> Vec<u8> {
        format!(
            r#"{{"id":"{}","status":"{}","chat_session_id":"{}"}}"#,
            id, status, chat_session_id
        )
        .into_bytes()
    }

    fn run_payload(
        run_id: &str,
        task_id: &str,
        execution_id: &str,
        status: &str,
        started_at: i64,
    ) -> Vec<u8> {
        format!(
            concat!(
                r#"{{"run_id":"{}","task_id":"{}","execution_id":"{}","status":"{}","#,
                r#""started_at":{},"updated_at":{},"ended_at":null,"checkpoint_id":null,"error":null,"metrics":{{}}}}"#
            ),
            run_id, task_id, execution_id, status, started_at, started_at
        )
        .into_bytes()
    }

    fn create_test_storage() -> BackgroundAgentStorage {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        BackgroundAgentStorage::new(db).unwrap()
    }

    #[test]
    fn test_put_and_get_task_raw() {
        let storage = create_test_storage();

        let data = b"test task data";
        storage.put_task_raw("task-001", data).unwrap();

        let retrieved = storage.get_task_raw("task-001").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), data);
    }

    #[test]
    fn test_get_nonexistent_task() {
        let storage = create_test_storage();

        let result = storage.get_task_raw("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_list_tasks_raw() {
        let storage = create_test_storage();

        storage.put_task_raw("task-001", b"data1").unwrap();
        storage.put_task_raw("task-002", b"data2").unwrap();
        storage.put_task_raw("task-003", b"data3").unwrap();

        let tasks = storage.list_tasks_raw().unwrap();
        assert_eq!(tasks.len(), 3);
    }

    #[test]
    fn test_list_tasks_by_status_indexed() {
        let storage = create_test_storage();

        storage
            .put_task_raw_with_status("task-001", "active", b"data1")
            .unwrap();
        storage
            .put_task_raw_with_status("task-002", "paused", b"data2")
            .unwrap();
        storage
            .put_task_raw_with_status("task-003", "active", b"data3")
            .unwrap();

        let active_tasks = storage.list_tasks_by_status_indexed("active").unwrap();
        let paused_tasks = storage.list_tasks_by_status_indexed("paused").unwrap();

        assert_eq!(active_tasks.len(), 2);
        assert_eq!(paused_tasks.len(), 1);
    }

    #[test]
    fn test_update_task_raw_with_status() {
        let storage = create_test_storage();

        storage
            .put_task_raw_with_status("task-001", "active", b"data1")
            .unwrap();
        storage
            .update_task_raw_with_status("task-001", "active", "paused", b"data2")
            .unwrap();

        let active_tasks = storage.list_tasks_by_status_indexed("active").unwrap();
        let paused_tasks = storage.list_tasks_by_status_indexed("paused").unwrap();

        assert!(active_tasks.is_empty());
        assert_eq!(paused_tasks.len(), 1);
    }

    #[test]
    fn test_update_task_raw_if_status_matches_updates_when_expected_status_matches() {
        let storage = create_test_storage();
        let original = task_status_payload("task-001", "active", "session-1");
        let updated = task_status_payload("task-001", "completed", "session-1");

        storage
            .put_task_raw_with_status("task-001", "active", &original)
            .unwrap();
        let changed = storage
            .update_task_raw_if_status_matches("task-001", "active", "completed", &updated)
            .unwrap();

        assert!(changed);
        assert!(
            storage
                .list_tasks_by_status_indexed("active")
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            storage
                .list_tasks_by_status_indexed("completed")
                .unwrap()
                .len(),
            1
        );
        assert_eq!(storage.get_task_raw("task-001").unwrap().unwrap(), updated);
    }

    #[test]
    fn test_update_task_raw_if_status_matches_returns_false_on_status_mismatch() {
        let storage = create_test_storage();
        let original = task_status_payload("task-001", "active", "session-1");
        let candidate = task_status_payload("task-001", "completed", "session-1");

        storage
            .put_task_raw_with_status("task-001", "active", &original)
            .unwrap();
        let changed = storage
            .update_task_raw_if_status_matches("task-001", "paused", "completed", &candidate)
            .unwrap();

        assert!(!changed);
        assert_eq!(storage.get_task_raw("task-001").unwrap().unwrap(), original);
        assert_eq!(
            storage
                .list_tasks_by_status_indexed("active")
                .unwrap()
                .len(),
            1
        );
        assert!(
            storage
                .list_tasks_by_status_indexed("completed")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn test_update_task_status_migration_clears_all_previous_status_index_entries() {
        let storage = create_test_storage();

        storage
            .put_task_raw_with_status("task-001", "active", b"v1")
            .unwrap();
        storage
            .update_task_raw_with_status("task-001", "active", "paused", b"v2")
            .unwrap();
        storage
            .update_task_raw_with_status("task-001", "paused", "completed", b"v3")
            .unwrap();

        assert!(
            storage
                .list_tasks_by_status_indexed("active")
                .unwrap()
                .is_empty()
        );
        assert!(
            storage
                .list_tasks_by_status_indexed("paused")
                .unwrap()
                .is_empty()
        );

        let completed_tasks = storage.list_tasks_by_status_indexed("completed").unwrap();
        assert_eq!(completed_tasks.len(), 1);
        assert_eq!(completed_tasks[0].0, "task-001");
        assert_eq!(completed_tasks[0].1, b"v3");
    }

    #[test]
    fn test_repeated_task_updates_do_not_create_duplicate_visible_status_entries() {
        let storage = create_test_storage();

        storage
            .put_task_raw_with_status("task-001", "active", b"v1")
            .unwrap();
        storage
            .update_task_raw_with_status("task-001", "active", "active", b"v2")
            .unwrap();
        storage
            .update_task_raw_with_status("task-001", "active", "active", b"v3")
            .unwrap();

        let active_tasks = storage.list_tasks_by_status_indexed("active").unwrap();
        assert_eq!(active_tasks.len(), 1);
        assert_eq!(active_tasks[0].0, "task-001");
        assert_eq!(active_tasks[0].1, b"v3");
    }

    #[test]
    fn test_put_task_raw_with_status_replaces_previous_status_index_entry() {
        let storage = create_test_storage();

        storage
            .put_task_raw_with_status("task-001", "active", b"data1")
            .unwrap();
        storage
            .put_task_raw_with_status("task-001", "paused", b"data2")
            .unwrap();

        let active_tasks = storage.list_tasks_by_status_indexed("active").unwrap();
        let paused_tasks = storage.list_tasks_by_status_indexed("paused").unwrap();

        assert!(active_tasks.is_empty());
        assert_eq!(paused_tasks.len(), 1);
        assert_eq!(paused_tasks[0].0, "task-001");
    }

    #[test]
    fn test_delete_task() {
        let storage = create_test_storage();

        storage.put_task_raw("task-001", b"data").unwrap();

        let deleted = storage.delete_task("task-001").unwrap();
        assert!(deleted);

        let retrieved = storage.get_task_raw("task-001").unwrap();
        assert!(retrieved.is_none());

        // Deleting again should return false
        let deleted_again = storage.delete_task("task-001").unwrap();
        assert!(!deleted_again);
    }

    #[test]
    fn test_delete_task_removes_status_lookup_entries() {
        let storage = create_test_storage();

        storage
            .put_task_raw_with_status("task-001", "active", b"raw")
            .unwrap();
        assert_eq!(
            storage
                .list_tasks_by_status_indexed("active")
                .unwrap()
                .len(),
            1
        );

        let deleted = storage.delete_task("task-001").unwrap();
        assert!(deleted);
        assert!(
            storage
                .list_tasks_by_status_indexed("active")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn test_delete_task_with_status() {
        let storage = create_test_storage();

        storage
            .put_task_raw_with_status("task-001", "active", b"data")
            .unwrap();

        let deleted = storage
            .delete_task_with_status("task-001", "active")
            .unwrap();
        assert!(deleted);

        let retrieved = storage.get_task_raw("task-001").unwrap();
        assert!(retrieved.is_none());

        let active_tasks = storage.list_tasks_by_status_indexed("active").unwrap();
        assert!(active_tasks.is_empty());
    }

    #[test]
    fn test_put_run_with_status_rejects_second_active_run_for_task() {
        let storage = create_test_storage();
        storage
            .put_task_raw("task-1", br#"{"id":"task-1"}"#)
            .unwrap();

        storage
            .put_run_raw_with_status(
                "run-1",
                "task-1",
                "running",
                &run_payload("run-1", "task-1", "exec-1", "running", 100),
            )
            .unwrap();
        let result = storage.put_run_raw_with_status(
            "run-2",
            "task-1",
            "running",
            &run_payload("run-2", "task-1", "exec-2", "running", 200),
        );

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("already has active run")
        );
    }

    #[test]
    fn test_marking_active_run_terminal_clears_active_run_index() {
        let storage = create_test_storage();
        storage
            .put_task_raw("task-1", br#"{"id":"task-1"}"#)
            .unwrap();

        storage
            .put_run_raw_with_status(
                "run-1",
                "task-1",
                "running",
                &run_payload("run-1", "task-1", "exec-1", "running", 100),
            )
            .unwrap();
        assert_eq!(
            storage.get_active_run_raw("task-1").unwrap().unwrap().0,
            "run-1"
        );

        storage
            .update_run_raw_with_status(
                "run-1",
                "task-1",
                "running",
                "completed",
                &run_payload("run-1", "task-1", "exec-1", "completed", 100),
            )
            .unwrap();
        assert!(storage.get_active_run_raw("task-1").unwrap().is_none());
    }

    #[test]
    fn test_put_run_with_status_rejects_cross_task_run_rebind() {
        let storage = create_test_storage();
        storage
            .put_task_raw("task-1", br#"{"id":"task-1"}"#)
            .unwrap();
        storage
            .put_task_raw("task-2", br#"{"id":"task-2"}"#)
            .unwrap();

        storage
            .put_run_raw_with_status(
                "run-1",
                "task-1",
                "completed",
                &run_payload("run-1", "task-1", "exec-1", "completed", 100),
            )
            .unwrap();
        let result = storage.put_run_raw_with_status(
            "run-1",
            "task-2",
            "completed",
            &run_payload("run-1", "task-2", "exec-2", "completed", 200),
        );

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("is indexed under task")
        );
    }

    #[test]
    fn test_put_and_get_background_message_raw() {
        let storage = create_test_storage();
        let data = br#"{"id":"msg-1","status":"queued"}"#;

        storage
            .put_background_message_raw_with_status("msg-1", "task-1", "queued", data)
            .unwrap();

        let raw = storage.get_background_message_raw("msg-1").unwrap();
        assert!(raw.is_some());
        assert_eq!(raw.unwrap(), data);
    }

    #[test]
    fn test_list_background_messages_for_task_raw() {
        let storage = create_test_storage();
        storage
            .put_background_message_raw_with_status(
                "msg-1",
                "task-1",
                "queued",
                br#"{"id":"msg-1","status":"queued"}"#,
            )
            .unwrap();
        storage
            .put_background_message_raw_with_status(
                "msg-2",
                "task-1",
                "delivered",
                br#"{"id":"msg-2","status":"delivered"}"#,
            )
            .unwrap();
        storage
            .put_background_message_raw_with_status(
                "msg-3",
                "task-2",
                "queued",
                br#"{"id":"msg-3","status":"queued"}"#,
            )
            .unwrap();

        let task1 = storage
            .list_background_messages_for_task_raw("task-1")
            .unwrap();
        let queued_task1 = storage
            .list_background_messages_by_status_for_task_raw("task-1", "queued")
            .unwrap();

        assert_eq!(task1.len(), 2);
        assert_eq!(queued_task1.len(), 1);
    }

    #[test]
    fn test_update_background_message_raw_with_status() {
        let storage = create_test_storage();
        storage
            .put_background_message_raw_with_status(
                "msg-1",
                "task-1",
                "queued",
                br#"{"id":"msg-1","status":"queued"}"#,
            )
            .unwrap();
        storage
            .update_background_message_raw_with_status(
                "msg-1",
                "task-1",
                "queued",
                "delivered",
                br#"{"id":"msg-1","status":"delivered"}"#,
            )
            .unwrap();

        let queued = storage
            .list_background_messages_by_status_for_task_raw("task-1", "queued")
            .unwrap();
        let delivered = storage
            .list_background_messages_by_status_for_task_raw("task-1", "delivered")
            .unwrap();
        assert!(queued.is_empty());
        assert_eq!(delivered.len(), 1);
    }

    #[test]
    fn test_delete_background_messages_for_task() {
        let storage = create_test_storage();
        storage
            .put_background_message_raw_with_status(
                "msg-1",
                "task-1",
                "queued",
                br#"{"id":"msg-1","status":"queued"}"#,
            )
            .unwrap();
        storage
            .put_background_message_raw_with_status(
                "msg-2",
                "task-1",
                "delivered",
                br#"{"id":"msg-2","status":"delivered"}"#,
            )
            .unwrap();
        storage
            .put_background_message_raw_with_status(
                "msg-3",
                "task-2",
                "queued",
                br#"{"id":"msg-3","status":"queued"}"#,
            )
            .unwrap();

        let deleted = storage
            .delete_background_messages_for_task("task-1")
            .unwrap();
        assert_eq!(deleted, 2);

        let remaining_task1 = storage
            .list_background_messages_for_task_raw("task-1")
            .unwrap();
        let remaining_task2 = storage
            .list_background_messages_for_task_raw("task-2")
            .unwrap();
        assert!(remaining_task1.is_empty());
        assert_eq!(remaining_task2.len(), 1);
    }

    #[test]
    fn test_delete_background_messages_for_task_removes_status_index_for_non_json_payload() {
        let storage = create_test_storage();
        storage
            .put_background_message_raw_with_status("msg-1", "task-1", "queued", b"raw-msg-1")
            .unwrap();
        storage
            .put_background_message_raw_with_status("msg-2", "task-1", "queued", b"raw-msg-2")
            .unwrap();

        assert_eq!(
            storage
                .list_background_messages_by_status_for_task_raw("task-1", "queued")
                .unwrap()
                .len(),
            2
        );

        let deleted = storage
            .delete_background_messages_for_task("task-1")
            .unwrap();
        assert_eq!(deleted, 2);
        assert!(
            storage
                .list_background_messages_by_status_for_task_raw("task-1", "queued")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn test_put_and_get_event_raw() {
        let storage = create_test_storage();

        let data = b"test event data";
        storage
            .put_event_raw("event-001", "task-001", data)
            .unwrap();

        let retrieved = storage.get_event_raw("event-001").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), data);
    }

    #[test]
    fn test_list_events_for_task() {
        let storage = create_test_storage();

        // Add events for task-001
        storage
            .put_event_raw("event-001", "task-001", b"data1")
            .unwrap();
        storage
            .put_event_raw("event-002", "task-001", b"data2")
            .unwrap();

        // Add events for task-002
        storage
            .put_event_raw("event-003", "task-002", b"data3")
            .unwrap();

        let events_task1 = storage.list_events_for_task_raw("task-001").unwrap();
        assert_eq!(events_task1.len(), 2);

        let events_task2 = storage.list_events_for_task_raw("task-002").unwrap();
        assert_eq!(events_task2.len(), 1);

        let events_task3 = storage.list_events_for_task_raw("task-003").unwrap();
        assert_eq!(events_task3.len(), 0);
    }

    #[test]
    fn test_delete_event() {
        let storage = create_test_storage();

        storage
            .put_event_raw("event-001", "task-001", b"data")
            .unwrap();

        let deleted = storage.delete_event("event-001", "task-001").unwrap();
        assert!(deleted);

        let retrieved = storage.get_event_raw("event-001").unwrap();
        assert!(retrieved.is_none());

        // Should also be removed from the index
        let events = storage.list_events_for_task_raw("task-001").unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn test_delete_events_for_task() {
        let storage = create_test_storage();

        storage
            .put_event_raw("event-001", "task-001", b"data1")
            .unwrap();
        storage
            .put_event_raw("event-002", "task-001", b"data2")
            .unwrap();
        storage
            .put_event_raw("event-003", "task-002", b"data3")
            .unwrap();

        let count = storage.delete_events_for_task("task-001").unwrap();
        assert_eq!(count, 2);

        let events_task1 = storage.list_events_for_task_raw("task-001").unwrap();
        assert!(events_task1.is_empty());

        // Events for task-002 should still exist
        let events_task2 = storage.list_events_for_task_raw("task-002").unwrap();
        assert_eq!(events_task2.len(), 1);
    }

    #[test]
    fn test_update_task() {
        let storage = create_test_storage();

        storage.put_task_raw("task-001", b"original data").unwrap();
        storage.put_task_raw("task-001", b"updated data").unwrap();

        let retrieved = storage.get_task_raw("task-001").unwrap();
        assert_eq!(retrieved.unwrap(), b"updated data");
    }

    #[test]
    fn test_put_task_with_status_rejects_duplicate_chat_session_binding() {
        let storage = create_test_storage();

        let first_task = task_payload("task-1", "Task One", "session-1");
        let second_task = task_payload("task-2", "Task Two", "session-1");

        storage
            .put_task_raw_with_status("task-1", "active", &first_task)
            .unwrap();
        let result = storage.put_task_raw_with_status("task-2", "active", &second_task);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("already bound to background task")
        );
    }

    #[test]
    fn test_update_task_with_status_rejects_duplicate_chat_session_binding() {
        let storage = create_test_storage();

        let task_one = task_payload("task-1", "Task One", "session-1");
        let task_two = task_payload("task-2", "Task Two", "session-2");
        let task_two_rebind = task_payload("task-2", "Task Two", "session-1");

        storage
            .put_task_raw_with_status("task-1", "active", &task_one)
            .unwrap();
        storage
            .put_task_raw_with_status("task-2", "active", &task_two)
            .unwrap();

        let result =
            storage.update_task_raw_with_status("task-2", "active", "active", &task_two_rebind);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("already bound to background task")
        );
    }

    #[test]
    fn test_put_task_with_status_rejects_when_existing_record_is_malformed() {
        let storage = create_test_storage();

        storage
            .put_task_raw_with_status("task-malformed", "active", b"{not-json")
            .unwrap();

        let result = storage.put_task_raw_with_status(
            "task-2",
            "active",
            &task_payload("task-2", "Task Two", "session-1"),
        );

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("failed to parse existing background task")
        );
    }

    #[test]
    fn test_delete_task_cascade_removes_related_records_atomically() {
        let storage = create_test_storage();

        storage
            .put_task_raw_with_status("task-1", "active", br#"{"id":"task-1"}"#)
            .unwrap();
        storage
            .put_task_raw_with_status("task-2", "active", br#"{"id":"task-2"}"#)
            .unwrap();

        storage
            .put_event_raw("event-1", "task-1", b"event-1")
            .unwrap();
        storage
            .put_event_raw("event-2", "task-1", b"event-2")
            .unwrap();
        storage
            .put_event_raw("event-3", "task-2", b"event-3")
            .unwrap();

        storage
            .put_run_raw_with_status(
                "run-1",
                "task-1",
                "running",
                &run_payload("run-1", "task-1", "exec-1", "running", 100),
            )
            .unwrap();
        storage
            .put_run_raw_with_status(
                "run-2",
                "task-2",
                "running",
                &run_payload("run-2", "task-2", "exec-2", "running", 100),
            )
            .unwrap();

        storage
            .put_background_message_raw_with_status(
                "msg-1",
                "task-1",
                "queued",
                br#"{"id":"msg-1","status":"queued"}"#,
            )
            .unwrap();
        storage
            .put_background_message_raw_with_status(
                "msg-2",
                "task-1",
                "delivered",
                br#"{"id":"msg-2","status":"delivered"}"#,
            )
            .unwrap();
        storage
            .put_background_message_raw_with_status(
                "msg-3",
                "task-2",
                "queued",
                br#"{"id":"msg-3","status":"queued"}"#,
            )
            .unwrap();

        let deleted = storage.delete_task_cascade("task-1").unwrap();
        assert!(deleted);

        assert!(storage.get_task_raw("task-1").unwrap().is_none());
        assert_eq!(storage.list_events_for_task_raw("task-1").unwrap().len(), 0);
        assert!(storage.get_run_raw("run-1").unwrap().is_none());
        assert!(storage.get_active_run_raw("task-1").unwrap().is_none());
        assert_eq!(
            storage
                .list_background_messages_for_task_raw("task-1")
                .unwrap()
                .len(),
            0
        );

        assert!(storage.get_task_raw("task-2").unwrap().is_some());
        assert_eq!(storage.list_events_for_task_raw("task-2").unwrap().len(), 1);
        assert!(storage.get_run_raw("run-2").unwrap().is_some());
        assert_eq!(
            storage.get_active_run_raw("task-2").unwrap().unwrap().0,
            "run-2"
        );
        assert_eq!(
            storage
                .list_background_messages_for_task_raw("task-2")
                .unwrap()
                .len(),
            1
        );

        let active_tasks = storage.list_tasks_by_status_indexed("active").unwrap();
        assert_eq!(active_tasks.len(), 1);
        assert_eq!(active_tasks[0].0, "task-2");
    }
}
