//! Typed agent task storage wrapper.
//!
//! Provides type-safe access to agent task storage by wrapping the byte-level
//! APIs from restflow-storage with Rust types from our models.

use crate::models::{
    AgentCheckpoint, Task, TaskControlAction, TaskEvent, TaskEventType, TaskMessage,
    TaskMessageSource, TaskMessageStatus, TaskPatch, TaskProgress, TaskSchedule, TaskSpec,
    TaskStatus,
};
use anyhow::Result;
use redb::Database;
use std::collections::HashSet;
use std::sync::Arc;
use tracing::warn;
use uuid::Uuid;

use super::{CheckpointStorage, ExecutionTraceStorage};

/// Typed agent task storage wrapper around restflow-storage::TaskStorage.
#[derive(Clone)]
pub struct TaskStorage {
    inner: restflow_storage::TaskStorage,
    checkpoints: CheckpointStorage,
    execution_traces: ExecutionTraceStorage,
}

#[derive(Debug, Clone)]
pub struct TaskSessionBinding {
    pub session_id: String,
    pub owns_session: bool,
}

impl TaskStorage {
    const MIN_TASK_TIMEOUT_SECS: u64 = 10;

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
        let checkpoints = CheckpointStorage::new(db.clone())?;
        let execution_traces = ExecutionTraceStorage::new(db.clone())?;
        Ok(Self {
            inner: restflow_storage::TaskStorage::new(db.clone())?,
            checkpoints,
            execution_traces,
        })
    }

    /// Access the execution trace storage.
    pub fn execution_traces(&self) -> &ExecutionTraceStorage {
        &self.execution_traces
    }

    fn event_stage_label(event_type: &TaskEventType) -> String {
        match event_type {
            TaskEventType::Created => "created",
            TaskEventType::Started => "running",
            TaskEventType::Completed => "completed",
            TaskEventType::Failed => "failed",
            TaskEventType::Paused => "paused",
            TaskEventType::Resumed => "active",
            TaskEventType::NotificationSent => "notification_sent",
            TaskEventType::NotificationFailed => "notification_failed",
            TaskEventType::Compaction => "compaction",
            TaskEventType::Interrupted => "interrupted",
        }
        .to_string()
    }
}

mod checkpoint_bridge;
mod cleanup;
mod event_log;
mod message_queue;
mod run_records;
mod session_binding;
mod task_lifecycle;

pub use task_lifecycle::ResolveTaskIdError;

#[cfg(test)]
mod tests;
