//! Typed agent task storage wrapper.
//!
//! Provides type-safe access to agent task storage by wrapping the byte-level
//! APIs from restflow-storage with Rust types from our models.

use crate::models::{
    AgentCheckpoint, BackgroundAgent, BackgroundAgentControlAction, BackgroundAgentEvent,
    BackgroundAgentEventType, BackgroundAgentPatch, BackgroundAgentSchedule, BackgroundAgentSpec,
    BackgroundAgentStatus, BackgroundMessage, BackgroundMessageSource, BackgroundMessageStatus,
    BackgroundProgress, ChatSession, ModelId,
};
use anyhow::Result;
use redb::Database;
use std::collections::HashSet;
use std::sync::Arc;
use tracing::warn;
use uuid::Uuid;

use super::{AgentStorage, ChatSessionStorage, CheckpointStorage, ExecutionTraceStorage};

/// Typed agent task storage wrapper around restflow-storage::BackgroundAgentStorage.
#[derive(Clone)]
pub struct BackgroundAgentStorage {
    inner: restflow_storage::BackgroundAgentStorage,
    checkpoints: CheckpointStorage,
    agents: AgentStorage,
    chat_sessions: ChatSessionStorage,
    execution_traces: ExecutionTraceStorage,
}

#[derive(Debug, Clone)]
struct SessionBindingResolution {
    session_id: String,
    owns_session: bool,
}

impl BackgroundAgentStorage {
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
            "background agent requires non-empty input or input_template"
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
        let replacements = std::collections::HashMap::from([
            ("{{task.input}}", input_value),
            ("{{input}}", input_value),
        ]);
        crate::template::render_template_single_pass(template, &replacements)
    }

    fn resolve_agent_model_for_session(&self, agent_id: &str) -> Result<String> {
        let fallback_model = ModelId::Gpt5.as_serialized_str().to_string();
        let Some(agent) = self.agents.get_agent(agent_id.to_string())? else {
            return Ok(fallback_model);
        };

        Ok(agent
            .agent
            .model
            .map(|model| model.as_serialized_str().to_string())
            .unwrap_or(fallback_model))
    }

    fn create_bound_chat_session(&self, agent_id: &str, task_name: &str) -> Result<String> {
        let model = self.resolve_agent_model_for_session(agent_id)?;
        let session_name = format!("Background: {}", task_name);
        let session = ChatSession::new(agent_id.to_string(), model).with_name(session_name);
        let session_id = session.id.clone();
        self.chat_sessions.create(&session)?;
        Ok(session_id)
    }

    fn ensure_chat_session_binding(&self, chat_session_id: &str, agent_id: &str) -> Result<()> {
        let session = self
            .chat_sessions
            .get(chat_session_id)?
            .ok_or_else(|| anyhow::anyhow!("chat_session_id '{}' not found", chat_session_id))?;

        if session.agent_id != agent_id {
            return Err(anyhow::anyhow!(
                "chat_session_id '{}' is bound to agent '{}', expected '{}'",
                chat_session_id,
                session.agent_id,
                agent_id
            ));
        }

        Ok(())
    }

    fn ensure_unique_chat_session_binding(
        &self,
        chat_session_id: &str,
        current_task_id: Option<&str>,
    ) -> Result<()> {
        let target = chat_session_id.trim();
        if target.is_empty() {
            return Ok(());
        }

        if let Some(conflict) = self.list_tasks()?.into_iter().find(|task| {
            let same_session = task.chat_session_id.trim() == target;
            let same_task = current_task_id.is_some_and(|task_id| task.id == task_id);
            same_session && !same_task
        }) {
            return Err(anyhow::anyhow!(
                "chat_session_id '{}' is already bound to background task '{}' ({})",
                target,
                conflict.id,
                conflict.name
            ));
        }

        Ok(())
    }

    fn resolve_chat_session_id_for_create(
        &self,
        requested_chat_session_id: Option<String>,
        agent_id: &str,
        task_name: &str,
    ) -> Result<SessionBindingResolution> {
        if let Some(chat_session_id) = Self::normalize_optional_id(requested_chat_session_id) {
            self.ensure_chat_session_binding(&chat_session_id, agent_id)?;
            self.ensure_unique_chat_session_binding(&chat_session_id, None)?;
            return Ok(SessionBindingResolution {
                session_id: chat_session_id,
                owns_session: false,
            });
        }

        Ok(SessionBindingResolution {
            session_id: self.create_bound_chat_session(agent_id, task_name)?,
            owns_session: true,
        })
    }

    fn resolve_chat_session_id_for_update(
        &self,
        task: &BackgroundAgent,
        requested_chat_session_id: Option<String>,
        next_agent_id: &str,
    ) -> Result<SessionBindingResolution> {
        if let Some(chat_session_id) = Self::normalize_optional_id(requested_chat_session_id) {
            self.ensure_chat_session_binding(&chat_session_id, next_agent_id)?;
            self.ensure_unique_chat_session_binding(&chat_session_id, Some(&task.id))?;
            return Ok(SessionBindingResolution {
                session_id: chat_session_id,
                owns_session: false,
            });
        }

        let current_chat_session_id = task.chat_session_id.trim();
        if !current_chat_session_id.is_empty()
            && self
                .ensure_chat_session_binding(current_chat_session_id, next_agent_id)
                .is_ok()
        {
            self.ensure_unique_chat_session_binding(current_chat_session_id, Some(&task.id))?;
            return Ok(SessionBindingResolution {
                session_id: current_chat_session_id.to_string(),
                owns_session: task.owns_chat_session,
            });
        }

        Ok(SessionBindingResolution {
            session_id: self.create_bound_chat_session(next_agent_id, &task.name)?,
            owns_session: true,
        })
    }

    /// Create a new BackgroundAgentStorage instance
    pub fn new(db: Arc<Database>) -> Result<Self> {
        let checkpoints = CheckpointStorage::new(db.clone())?;
        let execution_traces = ExecutionTraceStorage::new(db.clone())?;
        Ok(Self {
            inner: restflow_storage::BackgroundAgentStorage::new(db.clone())?,
            checkpoints,
            agents: AgentStorage::new(db.clone())?,
            chat_sessions: ChatSessionStorage::new(db)?,
            execution_traces,
        })
    }

    /// Access the underlying chat session storage.
    pub fn chat_sessions(&self) -> &ChatSessionStorage {
        &self.chat_sessions
    }

    /// Access the execution trace storage.
    pub fn execution_traces(&self) -> &ExecutionTraceStorage {
        &self.execution_traces
    }

    fn event_stage_label(event_type: &BackgroundAgentEventType) -> String {
        match event_type {
            BackgroundAgentEventType::Created => "created",
            BackgroundAgentEventType::Started => "running",
            BackgroundAgentEventType::Completed => "completed",
            BackgroundAgentEventType::Failed => "failed",
            BackgroundAgentEventType::Paused => "paused",
            BackgroundAgentEventType::Resumed => "active",
            BackgroundAgentEventType::NotificationSent => "notification_sent",
            BackgroundAgentEventType::NotificationFailed => "notification_failed",
            BackgroundAgentEventType::Compaction => "compaction",
            BackgroundAgentEventType::Interrupted => "interrupted",
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
