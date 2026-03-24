use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use chrono::{Local, TimeZone};
use thiserror::Error;

use crate::models::{
    BackgroundAgent, ChatSession, ChatSessionSource, ExecutionContainerKind,
    ExecutionContainerSummary, ExecutionSessionKind, ExecutionSessionListQuery,
    ExecutionSessionSummary, ExecutionThread, ExecutionThreadQuery, ExecutionTraceEvent,
    ExecutionTraceQuery,
};
use crate::services::session_policy::{EffectiveSessionSource, SessionPolicy};
use crate::storage::Storage;
use crate::telemetry::get_execution_timeline;

const WORKSPACE_CONTAINER_ID: &str = "workspace";

#[derive(Debug, Error)]
pub enum ExecutionThreadError {
    #[error("execution thread query requires run_id, session_id, or task_id")]
    InvalidQuery,
    #[error("session '{0}' not found")]
    SessionNotFound(String),
    #[error("run '{0}' not found")]
    RunNotFound(String),
    #[error("background task '{0}' not found")]
    TaskNotFound(String),
    #[error("background task '{0}' has no runs")]
    TaskHasNoRuns(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

#[derive(Clone)]
pub struct ExecutionConsoleService {
    storage: Arc<Storage>,
}

#[derive(Clone)]
struct SessionContext {
    session: ChatSession,
    source: EffectiveSessionSource,
    bound_task: Option<BackgroundAgent>,
}

#[derive(Clone)]
struct ExternalGroup {
    id: String,
    source_channel: ChatSessionSource,
    conversation_id: String,
    sessions: Vec<ChatSession>,
    updated_at: i64,
}

impl ExecutionConsoleService {
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }

    pub fn from_storage(storage: &Arc<Storage>) -> Self {
        Self::new(storage.clone())
    }

    pub fn list_execution_containers(&self) -> Result<Vec<ExecutionContainerSummary>> {
        let session_contexts = self.load_session_contexts()?;
        let workspace_sessions = session_contexts
            .iter()
            .filter(|ctx| {
                ctx.source.source == ChatSessionSource::Workspace && ctx.bound_task.is_none()
            })
            .map(|ctx| ctx.session.clone())
            .collect::<Vec<_>>();
        let external_groups = self.group_external_sessions(&session_contexts);
        let background_tasks = self.storage.background_agents.list_tasks()?;

        let mut containers = Vec::new();
        containers.push(self.build_workspace_container(&workspace_sessions));

        for task in background_tasks {
            containers.push(ExecutionContainerSummary {
                id: task.id.clone(),
                kind: ExecutionContainerKind::BackgroundTask,
                title: task.name.clone(),
                subtitle: task.description.clone(),
                updated_at: task.updated_at,
                status: Some(task.status.as_str().to_string()),
                session_count: task.success_count + task.failure_count,
                latest_session_id: Some(task.chat_session_id.clone()),
                latest_run_id: None,
                agent_id: Some(task.agent_id.clone()),
                source_channel: None,
                source_conversation_id: None,
            });
        }

        for group in external_groups.into_values() {
            let latest_session = group
                .sessions
                .iter()
                .max_by(|left, right| left.updated_at.cmp(&right.updated_at));
            containers.push(ExecutionContainerSummary {
                id: group.id,
                kind: ExecutionContainerKind::ExternalChannel,
                title: group.conversation_id.clone(),
                subtitle: latest_session.map(|session| session.name.clone()),
                updated_at: group.updated_at,
                status: Some("active".to_string()),
                session_count: group.sessions.len() as u32,
                latest_session_id: latest_session.map(|session| session.id.clone()),
                latest_run_id: None,
                agent_id: latest_session.map(|session| session.agent_id.clone()),
                source_channel: Some(group.source_channel),
                source_conversation_id: Some(group.conversation_id),
            });
        }

        containers.sort_by(|left, right| {
            execution_container_sort_key(left)
                .cmp(&execution_container_sort_key(right))
                .then_with(|| right.updated_at.cmp(&left.updated_at))
                .then_with(|| left.id.cmp(&right.id))
        });

        Ok(containers)
    }

    pub fn list_execution_sessions(
        &self,
        query: &ExecutionSessionListQuery,
    ) -> Result<Vec<ExecutionSessionSummary>> {
        match query.container.kind {
            ExecutionContainerKind::Workspace => self.list_workspace_sessions(),
            ExecutionContainerKind::BackgroundTask => {
                self.list_background_task_sessions(&query.container.id)
            }
            ExecutionContainerKind::ExternalChannel => {
                self.list_external_channel_sessions(&query.container.id)
            }
        }
    }

    pub fn get_execution_thread(
        &self,
        query: &ExecutionThreadQuery,
    ) -> std::result::Result<ExecutionThread, ExecutionThreadError> {
        if let Some(run_id) = query
            .run_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            return self.get_run_thread(run_id);
        }

        if let Some(session_id) = query
            .session_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            return self.get_session_thread(session_id);
        }

        if let Some(task_id) = query
            .task_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            let task = self
                .storage
                .background_agents
                .get_task(task_id)
                .map_err(ExecutionThreadError::from)?
                .ok_or_else(|| ExecutionThreadError::TaskNotFound(task_id.to_string()))?;
            let runs = self
                .list_background_task_runs(&task)
                .map_err(ExecutionThreadError::from)?;
            if let Some(run) = runs.first()
                && let Some(run_id) = run.run_id.as_deref()
            {
                return self.get_run_thread(run_id);
            }

            return Err(ExecutionThreadError::TaskHasNoRuns(task_id.to_string()));
        }

        Err(ExecutionThreadError::InvalidQuery)
    }

    pub fn list_child_execution_sessions(
        &self,
        parent_run_id: &str,
    ) -> Result<Vec<ExecutionSessionSummary>> {
        let parent_run_id = parent_run_id.trim();
        if parent_run_id.is_empty() {
            return Ok(Vec::new());
        }

        let events = self.storage.execution_traces.query(&ExecutionTraceQuery {
            parent_run_id: Some(parent_run_id.to_string()),
            limit: Some(usize::MAX),
            ..ExecutionTraceQuery::default()
        })?;

        let mut groups: HashMap<String, Vec<ExecutionTraceEvent>> = HashMap::new();
        for event in events {
            let Some(run_id) = event.run_id.clone() else {
                continue;
            };
            groups.entry(run_id).or_default().push(event);
        }

        let mut sessions = groups
            .into_iter()
            .map(|(run_id, mut events)| {
                events.sort_by(|left, right| left.timestamp.cmp(&right.timestamp));
                self.build_run_summary(
                    &run_id,
                    &run_id,
                    ExecutionSessionKind::SubagentRun,
                    &events,
                    Some("Subagent run".to_string()),
                    None,
                )
            })
            .collect::<Vec<_>>();

        sessions.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(sessions)
    }

    fn build_workspace_container(&self, sessions: &[ChatSession]) -> ExecutionContainerSummary {
        let latest = sessions
            .iter()
            .max_by(|left, right| left.updated_at.cmp(&right.updated_at));
        ExecutionContainerSummary {
            id: WORKSPACE_CONTAINER_ID.to_string(),
            kind: ExecutionContainerKind::Workspace,
            title: "Workspace".to_string(),
            subtitle: Some("Local sessions".to_string()),
            updated_at: latest.map(|session| session.updated_at).unwrap_or_default(),
            status: None,
            session_count: sessions.len() as u32,
            latest_session_id: latest.map(|session| session.id.clone()),
            latest_run_id: None,
            agent_id: None,
            source_channel: Some(ChatSessionSource::Workspace),
            source_conversation_id: None,
        }
    }

    fn load_session_contexts(&self) -> Result<Vec<SessionContext>> {
        let sessions = self.storage.chat_sessions.list()?;
        let session_policy = SessionPolicy::from_storage(&self.storage);
        let mut bound_tasks_by_session_id = HashMap::new();
        for task in self.storage.background_agents.list_tasks()? {
            let trimmed_session_id = task.chat_session_id.trim();
            if trimmed_session_id.is_empty() {
                continue;
            }
            bound_tasks_by_session_id.insert(trimmed_session_id.to_string(), task);
        }
        let mut contexts = Vec::with_capacity(sessions.len());

        for session in sessions {
            let source = session_policy.effective_source(&session)?;
            let bound_task = bound_tasks_by_session_id.get(&session.id).cloned();
            contexts.push(SessionContext {
                session,
                source,
                bound_task,
            });
        }

        Ok(contexts)
    }

    fn list_workspace_sessions(&self) -> Result<Vec<ExecutionSessionSummary>> {
        let contexts = self.load_session_contexts()?;
        let mut sessions = contexts
            .into_iter()
            .filter(|ctx| {
                ctx.source.source == ChatSessionSource::Workspace && ctx.bound_task.is_none()
            })
            .map(|ctx| {
                self.build_navigation_session_summary(
                    &ctx.session,
                    WORKSPACE_CONTAINER_ID,
                    ExecutionSessionKind::WorkspaceSession,
                    Some(ctx.source),
                )
            })
            .collect::<Result<Vec<_>>>()?;
        sessions.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(sessions)
    }

    fn list_background_task_sessions(&self, task_id: &str) -> Result<Vec<ExecutionSessionSummary>> {
        let task = self
            .storage
            .background_agents
            .get_task(task_id)?
            .ok_or_else(|| anyhow!("background task '{}' not found", task_id))?;
        self.list_background_task_runs(&task)
    }

    fn list_background_task_runs(
        &self,
        task: &BackgroundAgent,
    ) -> Result<Vec<ExecutionSessionSummary>> {
        let events = self.storage.execution_traces.query(&ExecutionTraceQuery {
            task_id: Some(task.id.clone()),
            limit: Some(usize::MAX),
            ..ExecutionTraceQuery::default()
        })?;

        let mut groups: HashMap<String, Vec<ExecutionTraceEvent>> = HashMap::new();
        for event in events {
            if event.parent_run_id.is_some() {
                continue;
            }
            let Some(run_id) = event.run_id.clone() else {
                continue;
            };
            groups.entry(run_id).or_default().push(event);
        }

        let mut runs = groups
            .into_iter()
            .map(|(run_id, mut run_events)| {
                run_events.sort_by(|left, right| left.timestamp.cmp(&right.timestamp));
                self.build_run_summary(
                    &run_id,
                    &task.id,
                    ExecutionSessionKind::BackgroundRun,
                    &run_events,
                    Some(format_run_title(
                        run_events
                            .first()
                            .map(|event| event.timestamp)
                            .unwrap_or(task.updated_at),
                    )),
                    Some(task.name.clone()),
                )
            })
            .collect::<Vec<_>>();

        runs.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(runs)
    }

    fn list_external_channel_sessions(
        &self,
        container_id: &str,
    ) -> Result<Vec<ExecutionSessionSummary>> {
        let contexts = self.load_session_contexts()?;
        let mut sessions = contexts
            .into_iter()
            .filter(|ctx| {
                ctx.source.source != ChatSessionSource::Workspace
                    && external_container_id(
                        ctx.source.source,
                        ctx.source
                            .conversation_id
                            .as_deref()
                            .unwrap_or(&ctx.session.id),
                    ) == container_id
            })
            .map(|ctx| {
                self.build_navigation_session_summary(
                    &ctx.session,
                    container_id,
                    ExecutionSessionKind::ExternalSession,
                    Some(ctx.source),
                )
            })
            .collect::<Result<Vec<_>>>()?;
        sessions.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(sessions)
    }

    fn build_session_summary(
        &self,
        session: &ChatSession,
        container_id: &str,
        kind: ExecutionSessionKind,
        source: Option<EffectiveSessionSource>,
    ) -> Result<ExecutionSessionSummary> {
        let events = self.storage.execution_traces.query(&ExecutionTraceQuery {
            session_id: Some(session.id.clone()),
            limit: Some(usize::MAX),
            ..ExecutionTraceQuery::default()
        })?;
        let mut sorted = events;
        sorted.sort_by(|left, right| left.timestamp.cmp(&right.timestamp));

        let latest_run_id = latest_top_level_run_id(&sorted);
        let status = latest_lifecycle_status(&sorted).unwrap_or_else(|| "idle".to_string());
        let started_at = sorted.first().map(|event| event.timestamp);
        let ended_at = latest_terminal_timestamp(&sorted);
        let effective_model =
            latest_effective_model(&sorted).or_else(|| Some(session.model.clone()));
        let provider = latest_provider(&sorted);
        let conversation_id = source
            .as_ref()
            .and_then(|value| value.conversation_id.clone());
        let subtitle = conversation_id
            .clone()
            .or_else(|| session.skill_id.clone())
            .or_else(|| {
                session
                    .messages
                    .last()
                    .map(|message| truncate_text(&message.content, 72))
            });
        let source_channel = source
            .as_ref()
            .map(|value| value.source)
            .or(session.source_channel);
        let source_conversation_id =
            conversation_id.or_else(|| session.source_conversation_id.clone());

        Ok(ExecutionSessionSummary {
            id: session.id.clone(),
            kind,
            container_id: container_id.to_string(),
            title: session.name.clone(),
            subtitle,
            status,
            updated_at: session.updated_at,
            started_at,
            ended_at,
            session_id: Some(session.id.clone()),
            run_id: latest_run_id,
            task_id: None,
            parent_run_id: None,
            agent_id: Some(session.agent_id.clone()),
            source_channel,
            source_conversation_id,
            effective_model,
            provider,
            event_count: sorted.len() as u64,
        })
    }

    fn build_navigation_session_summary(
        &self,
        session: &ChatSession,
        container_id: &str,
        kind: ExecutionSessionKind,
        source: Option<EffectiveSessionSource>,
    ) -> Result<ExecutionSessionSummary> {
        let conversation_id = source
            .as_ref()
            .and_then(|value| value.conversation_id.clone());
        let subtitle = conversation_id
            .clone()
            .or_else(|| session.skill_id.clone())
            .or_else(|| {
                session
                    .messages
                    .last()
                    .map(|message| truncate_text(&message.content, 72))
            });
        let source_channel = source
            .as_ref()
            .map(|value| value.source)
            .or(session.source_channel);
        let source_conversation_id =
            conversation_id.or_else(|| session.source_conversation_id.clone());
        let status = if session.messages.is_empty() {
            "pending".to_string()
        } else {
            "completed".to_string()
        };

        Ok(ExecutionSessionSummary {
            id: session.id.clone(),
            kind,
            container_id: container_id.to_string(),
            title: session.name.clone(),
            subtitle,
            status,
            updated_at: session.updated_at,
            started_at: Some(session.created_at),
            ended_at: None,
            session_id: Some(session.id.clone()),
            run_id: None,
            task_id: None,
            parent_run_id: None,
            agent_id: Some(session.agent_id.clone()),
            source_channel,
            source_conversation_id,
            effective_model: Some(session.model.clone()),
            provider: None,
            event_count: 0,
        })
    }

    fn build_run_summary(
        &self,
        run_id: &str,
        container_id: &str,
        kind: ExecutionSessionKind,
        events: &[ExecutionTraceEvent],
        title: Option<String>,
        subtitle: Option<String>,
    ) -> ExecutionSessionSummary {
        let first = events.first();
        let last = events.last();
        let started_at = first.map(|event| event.timestamp);
        let ended_at = latest_terminal_timestamp(events);
        let status = latest_lifecycle_status(events).unwrap_or_else(|| "running".to_string());
        let session_id = last.and_then(|event| event.session_id.clone());
        let task_id = last.map(|event| event.task_id.clone());
        let parent_run_id = last.and_then(|event| event.parent_run_id.clone());
        let agent_id = last.map(|event| event.agent_id.clone());
        let source_channel = None;
        let source_conversation_id = None;
        let effective_model = latest_effective_model(events);
        let provider = latest_provider(events);

        ExecutionSessionSummary {
            id: run_id.to_string(),
            kind,
            container_id: container_id.to_string(),
            title: title.unwrap_or_else(|| format!("Run {}", short_id(run_id))),
            subtitle,
            status,
            updated_at: last.map(|event| event.timestamp).unwrap_or_default(),
            started_at,
            ended_at,
            session_id,
            run_id: Some(run_id.to_string()),
            task_id,
            parent_run_id,
            agent_id,
            source_channel,
            source_conversation_id,
            effective_model,
            provider,
            event_count: events.len() as u64,
        }
    }

    fn group_external_sessions(
        &self,
        contexts: &[SessionContext],
    ) -> HashMap<String, ExternalGroup> {
        let mut groups = HashMap::new();
        for context in contexts {
            if context.source.source == ChatSessionSource::Workspace {
                continue;
            }
            let conversation_id = context
                .source
                .conversation_id
                .clone()
                .unwrap_or_else(|| context.session.id.clone());
            let container_id = external_container_id(context.source.source, &conversation_id);
            let entry = groups
                .entry(container_id.clone())
                .or_insert_with(|| ExternalGroup {
                    id: container_id.clone(),
                    source_channel: context.source.source,
                    conversation_id: conversation_id.clone(),
                    sessions: Vec::new(),
                    updated_at: context.session.updated_at,
                });
            entry.updated_at = entry.updated_at.max(context.session.updated_at);
            entry.sessions.push(context.session.clone());
        }
        groups
    }

    fn get_session_thread(
        &self,
        session_id: &str,
    ) -> std::result::Result<ExecutionThread, ExecutionThreadError> {
        let session = self
            .storage
            .chat_sessions
            .get(session_id)
            .map_err(ExecutionThreadError::from)?
            .ok_or_else(|| ExecutionThreadError::SessionNotFound(session_id.to_string()))?;
        let policy = SessionPolicy::from_storage(&self.storage);
        let source = policy
            .effective_source(&session)
            .map_err(ExecutionThreadError::from)?;
        let bound_task = policy
            .bound_background_task(session_id)
            .map_err(ExecutionThreadError::from)?;
        let container_id = if let Some(task) = bound_task.as_ref() {
            task.id.clone()
        } else if source.source == ChatSessionSource::Workspace {
            WORKSPACE_CONTAINER_ID.to_string()
        } else {
            external_container_id(
                source.source,
                source.conversation_id.as_deref().unwrap_or(session_id),
            )
        };
        let kind = if source.source == ChatSessionSource::Workspace {
            ExecutionSessionKind::WorkspaceSession
        } else {
            ExecutionSessionKind::ExternalSession
        };
        let focus = self
            .build_session_summary(&session, &container_id, kind, Some(source))
            .map_err(ExecutionThreadError::from)?;
        let timeline = get_execution_timeline(
            &self.storage.execution_traces,
            &ExecutionTraceQuery {
                session_id: Some(session_id.to_string()),
                limit: Some(usize::MAX),
                ..ExecutionTraceQuery::default()
            },
        )
        .map_err(ExecutionThreadError::from)?;
        let child_sessions = if let Some(run_id) = focus.run_id.as_deref() {
            self.list_child_execution_sessions(run_id)
                .map_err(ExecutionThreadError::from)?
        } else {
            Vec::new()
        };
        Ok(ExecutionThread {
            focus,
            timeline,
            child_sessions,
        })
    }

    fn get_run_thread(
        &self,
        run_id: &str,
    ) -> std::result::Result<ExecutionThread, ExecutionThreadError> {
        let timeline = get_execution_timeline(
            &self.storage.execution_traces,
            &ExecutionTraceQuery {
                run_id: Some(run_id.to_string()),
                limit: Some(usize::MAX),
                ..ExecutionTraceQuery::default()
            },
        )
        .map_err(ExecutionThreadError::from)?;
        if timeline.events.is_empty() {
            return Err(ExecutionThreadError::RunNotFound(run_id.to_string()));
        }
        let focus = self
            .build_focus_for_run(run_id, &timeline.events)
            .map_err(ExecutionThreadError::from)?;
        let child_sessions = self
            .list_child_execution_sessions(run_id)
            .map_err(ExecutionThreadError::from)?;
        Ok(ExecutionThread {
            focus,
            timeline,
            child_sessions,
        })
    }

    fn build_focus_for_run(
        &self,
        run_id: &str,
        events: &[ExecutionTraceEvent],
    ) -> Result<ExecutionSessionSummary> {
        let latest = events
            .last()
            .ok_or_else(|| anyhow!("run '{}' has no events", run_id))?;
        if latest.parent_run_id.is_none()
            && let Ok(Some(task)) = self.storage.background_agents.get_task(&latest.task_id)
        {
            return Ok(self.build_run_summary(
                run_id,
                &task.id,
                ExecutionSessionKind::BackgroundRun,
                events,
                Some(format_run_title(
                    events
                        .first()
                        .map(|event| event.timestamp)
                        .unwrap_or(task.updated_at),
                )),
                Some(task.name),
            ));
        }

        Ok(self.build_run_summary(
            run_id,
            latest.parent_run_id.as_deref().unwrap_or(run_id),
            ExecutionSessionKind::SubagentRun,
            events,
            Some(format_run_title(
                events
                    .first()
                    .map(|event| event.timestamp)
                    .unwrap_or(latest.timestamp),
            )),
            latest.session_id.clone(),
        ))
    }
}

fn latest_lifecycle_status(events: &[ExecutionTraceEvent]) -> Option<String> {
    events.iter().rev().find_map(|event| {
        event
            .lifecycle
            .as_ref()
            .map(|lifecycle| lifecycle.status.clone())
    })
}

fn latest_terminal_timestamp(events: &[ExecutionTraceEvent]) -> Option<i64> {
    events.iter().rev().find_map(|event| {
        let status = event.lifecycle.as_ref()?.status.to_ascii_lowercase();
        match status.as_str() {
            "completed" | "failed" | "interrupted" | "cancelled" => Some(event.timestamp),
            _ => None,
        }
    })
}

fn latest_effective_model(events: &[ExecutionTraceEvent]) -> Option<String> {
    events.iter().rev().find_map(|event| {
        event
            .effective_model
            .clone()
            .or_else(|| event.llm_call.as_ref().map(|call| call.model.clone()))
    })
}

fn latest_provider(events: &[ExecutionTraceEvent]) -> Option<String> {
    events.iter().rev().find_map(|event| event.provider.clone())
}

fn latest_top_level_run_id(events: &[ExecutionTraceEvent]) -> Option<String> {
    events
        .iter()
        .rev()
        .find(|event| event.parent_run_id.is_none())
        .and_then(|event| event.run_id.clone())
}

fn truncate_text(text: &str, max_chars: usize) -> String {
    let preview: String = text.chars().take(max_chars).collect();
    if text.chars().count() > max_chars {
        format!("{}...", preview)
    } else {
        preview
    }
}

fn short_id(value: &str) -> String {
    value.chars().take(8).collect()
}

fn format_run_title(timestamp: i64) -> String {
    Local
        .timestamp_millis_opt(timestamp)
        .single()
        .map(|value| format!("Run {}", value.format("%Y-%m-%d %H:%M")))
        .unwrap_or_else(|| "Run".to_string())
}

fn external_container_id(source: ChatSessionSource, conversation_id: &str) -> String {
    format!("{}:{}", external_channel_key(source), conversation_id)
}

fn external_channel_key(source: ChatSessionSource) -> &'static str {
    match source {
        ChatSessionSource::Telegram => "telegram",
        ChatSessionSource::Discord => "discord",
        ChatSessionSource::Slack => "slack",
        ChatSessionSource::ExternalLegacy => "external",
        ChatSessionSource::Workspace => "workspace",
    }
}

fn execution_container_sort_key(container: &ExecutionContainerSummary) -> u8 {
    match container.kind {
        ExecutionContainerKind::Workspace => 0,
        ExecutionContainerKind::BackgroundTask => 1,
        ExecutionContainerKind::ExternalChannel => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        BackgroundAgentSchedule, BackgroundAgentSpec, ChatMessage, ChatSession,
        ExecutionContainerRef, ExecutionMode, LifecycleTrace, NotificationConfig,
    };
    use crate::storage::Storage;
    use crate::{ExecutionTraceCategory, ExecutionTraceEvent, ExecutionTraceSource};
    use std::sync::Arc;

    fn create_storage() -> Arc<Storage> {
        Arc::new(Storage::new(":memory:").expect("storage"))
    }

    fn store_run_events(
        storage: &Arc<Storage>,
        task_id: &str,
        session_id: &str,
        run_id: &str,
        parent_run_id: Option<&str>,
    ) {
        let trace = restflow_telemetry::RestflowTrace::new(
            run_id.to_string(),
            session_id.to_string(),
            task_id.to_string(),
            "agent-1".to_string(),
        )
        .with_parent_run_id(parent_run_id.map(|value| value.to_string()));
        let start = ExecutionTraceEvent::lifecycle(
            task_id,
            "agent-1",
            LifecycleTrace {
                status: "running".to_string(),
                message: Some("started".to_string()),
                error: None,
                ai_duration_ms: None,
            },
        )
        .with_trace_context(&trace)
        .with_effective_model("openai/gpt-5")
        .with_provider("openai");
        let end = ExecutionTraceEvent::new(
            task_id,
            "agent-1",
            ExecutionTraceCategory::Lifecycle,
            ExecutionTraceSource::Runtime,
        )
        .with_trace_context(&trace)
        .with_lifecycle(LifecycleTrace {
            status: "completed".to_string(),
            message: Some("done".to_string()),
            error: None,
            ai_duration_ms: Some(1200),
        })
        .with_effective_model("openai/gpt-5")
        .with_provider("openai");
        storage.execution_traces.store(&start).expect("store start");
        storage.execution_traces.store(&end).expect("store end");
    }

    #[test]
    fn lists_workspace_and_external_containers() {
        let storage = create_storage();
        let service = ExecutionConsoleService::from_storage(&storage);

        let mut workspace = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        workspace.name = "Workspace Session".to_string();
        workspace.add_message(ChatMessage::user("hello"));
        storage
            .chat_sessions
            .create(&workspace)
            .expect("workspace session");

        let mut telegram = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        telegram.name = "Telegram Thread".to_string();
        telegram.source_channel = Some(ChatSessionSource::Telegram);
        telegram.source_conversation_id = Some("chat-42".to_string());
        storage
            .chat_sessions
            .create(&telegram)
            .expect("telegram session");

        let containers = service.list_execution_containers().expect("containers");
        assert!(
            containers
                .iter()
                .any(|container| container.id == WORKSPACE_CONTAINER_ID)
        );
        assert!(
            containers
                .iter()
                .any(|container| container.id == "telegram:chat-42")
        );
    }

    #[test]
    fn excludes_background_bound_sessions_from_workspace_projection() {
        let storage = create_storage();
        let service = ExecutionConsoleService::from_storage(&storage);

        let mut workspace = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        workspace.name = "Workspace Session".to_string();
        storage
            .chat_sessions
            .create(&workspace)
            .expect("workspace session");

        let task = storage
            .background_agents
            .create_background_agent(BackgroundAgentSpec {
                name: "Digest".to_string(),
                description: None,
                agent_id: "agent-1".to_string(),
                chat_session_id: None,
                input: Some("digest".to_string()),
                input_template: None,
                schedule: BackgroundAgentSchedule::default(),
                notification: Some(NotificationConfig::default()),
                execution_mode: Some(ExecutionMode::default()),
                timeout_secs: None,
                memory: None,
                durability_mode: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("task");

        let containers = service.list_execution_containers().expect("containers");
        let workspace_container = containers
            .iter()
            .find(|container| container.id == WORKSPACE_CONTAINER_ID)
            .expect("workspace container");
        assert!(workspace_container.session_count >= 1);

        let sessions = service
            .list_execution_sessions(&ExecutionSessionListQuery {
                container: ExecutionContainerRef {
                    kind: ExecutionContainerKind::Workspace,
                    id: WORKSPACE_CONTAINER_ID.to_string(),
                },
            })
            .expect("workspace sessions");

        assert!(
            sessions
                .iter()
                .any(|session| session.session_id.as_deref() == Some(workspace.id.as_str()))
        );
        assert!(
            sessions
                .iter()
                .all(|session| session.session_id.as_deref() != Some(task.chat_session_id.as_str()))
        );
    }

    #[test]
    fn lists_background_task_runs_and_child_runs() {
        let storage = create_storage();
        let service = ExecutionConsoleService::from_storage(&storage);

        let task = storage
            .background_agents
            .create_background_agent(BackgroundAgentSpec {
                name: "Digest".to_string(),
                description: None,
                agent_id: "agent-1".to_string(),
                chat_session_id: None,
                input: Some("digest".to_string()),
                input_template: None,
                schedule: BackgroundAgentSchedule::default(),
                notification: Some(NotificationConfig::default()),
                execution_mode: Some(ExecutionMode::default()),
                timeout_secs: None,
                memory: None,
                durability_mode: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("task");

        store_run_events(
            &storage,
            &task.id,
            &task.chat_session_id,
            "run-parent",
            None,
        );
        store_run_events(
            &storage,
            &task.id,
            &task.chat_session_id,
            "run-child",
            Some("run-parent"),
        );

        let runs = service
            .list_execution_sessions(&ExecutionSessionListQuery {
                container: ExecutionContainerRef {
                    kind: ExecutionContainerKind::BackgroundTask,
                    id: task.id.clone(),
                },
            })
            .expect("task runs");
        assert!(
            runs.iter()
                .any(|run| run.run_id.as_deref() == Some("run-parent"))
        );

        let child_runs = service
            .list_child_execution_sessions("run-parent")
            .expect("child runs");
        assert_eq!(child_runs.len(), 1);
        assert_eq!(child_runs[0].run_id.as_deref(), Some("run-child"));
    }

    #[test]
    fn resolves_session_thread_with_latest_run_and_children() {
        let storage = create_storage();
        let service = ExecutionConsoleService::from_storage(&storage);

        let session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        let session_id = session.id.clone();
        storage.chat_sessions.create(&session).expect("session");

        store_run_events(&storage, "task-1", &session_id, "run-1", None);
        store_run_events(&storage, "task-1", &session_id, "run-2", Some("run-1"));

        let thread = service
            .get_execution_thread(&ExecutionThreadQuery {
                session_id: Some(session_id),
                run_id: None,
                task_id: None,
            })
            .expect("thread");
        assert_eq!(thread.focus.run_id.as_deref(), Some("run-1"));
        assert!(thread.timeline.events.len() >= 2);
        assert_eq!(thread.child_sessions.len(), 1);
        assert_eq!(thread.child_sessions[0].run_id.as_deref(), Some("run-2"));
    }
}
