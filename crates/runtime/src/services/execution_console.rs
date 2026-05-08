use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{Result, anyhow};
use chrono::{Local, TimeZone};
use serde_json::Value;
use thiserror::Error;

use crate::models::{
    ChatRole, ChatSession, ChatSessionSource, ChatTurn, ChatTurnEventKind, ChatTurnStatus,
    ExecutionContainerKind, ExecutionContainerSummary, ExecutionThread, ExecutionTimeline,
    ExecutionTraceCategory, ExecutionTraceEvent, ExecutionTraceQuery, ExecutionTraceSource,
    LifecycleTrace, MessageTrace, RunKind, RunListQuery, RunSummary, Task, TaskRun, ToolCallPhase,
    ToolCallTrace,
};
use crate::services::session::SessionService;
use crate::services::session_policy::{EffectiveSessionSource, SessionPolicy};
use crate::storage::Storage;
use crate::telemetry::{execution_trace_stats_for_events, get_execution_timeline};

#[derive(Debug, Error)]
pub enum ExecutionThreadError {
    #[error("execution thread query requires run_id, session_id, or task_id")]
    InvalidQuery,
    #[error("run '{0}' not found")]
    RunNotFound(String),
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
    bound_task: Option<Task>,
}

#[derive(Clone, Default)]
struct RunSummaryMeta {
    title: Option<String>,
    subtitle: Option<String>,
    source_channel: Option<ChatSessionSource>,
    source_conversation_id: Option<String>,
}

#[derive(Clone)]
struct LatestRunProjection {
    run_id: String,
    updated_at: i64,
    session_id: Option<String>,
    task_id: String,
}

#[derive(Clone)]
struct RootRunContext {
    container_id: String,
    root_run_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SubagentResultProjection {
    task_id: String,
    agent: Option<String>,
    task: Option<String>,
    status: String,
    output: Option<String>,
    duration_ms: Option<i64>,
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
        let latest_runs = self.load_latest_top_level_runs()?;
        let latest_run_by_session_id = latest_run_by_session_id(&latest_runs);
        let latest_run_by_task_id = latest_run_by_task_id(&latest_runs);
        let workspace_containers = session_contexts
            .iter()
            .filter(|ctx| {
                ctx.source.source == ChatSessionSource::Workspace && ctx.bound_task.is_none()
            })
            .map(|ctx| {
                self.build_workspace_container(
                    ctx,
                    latest_run_by_session_id
                        .get(ctx.session.id.as_str())
                        .map(|entry| entry.run_id.clone()),
                )
            })
            .collect::<Vec<_>>();
        let tasks = self.storage.tasks.list_tasks()?;

        let mut containers = workspace_containers;

        for task in tasks {
            containers.push(ExecutionContainerSummary {
                id: task.id.clone(),
                kind: ExecutionContainerKind::Task,
                title: task.name.clone(),
                subtitle: task.description.clone(),
                updated_at: task.updated_at,
                status: Some(task.status.as_str().to_string()),
                session_count: task.success_count + task.failure_count,
                latest_session_id: Some(task.chat_session_id.clone()),
                latest_run_id: latest_run_by_task_id
                    .get(task.id.as_str())
                    .map(|entry| entry.run_id.clone()),
                agent_id: Some(task.agent_id.clone()),
                source_channel: None,
                source_conversation_id: None,
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

    pub fn list_runs(&self, query: &RunListQuery) -> Result<Vec<RunSummary>> {
        match query.container.kind {
            ExecutionContainerKind::Workspace => self.list_workspace_runs(&query.container.id),
            ExecutionContainerKind::Task => self.list_task_sessions(&query.container.id),
        }
    }

    pub fn get_execution_run_thread(
        &self,
        run_id: &str,
    ) -> std::result::Result<ExecutionThread, ExecutionThreadError> {
        let run_id = run_id.trim();
        if run_id.is_empty() {
            return Err(ExecutionThreadError::InvalidQuery);
        }

        self.get_run_thread(run_id)
    }

    pub fn get_execution_run_timeline(
        &self,
        run_id: &str,
    ) -> std::result::Result<ExecutionTimeline, ExecutionThreadError> {
        let run_id = run_id.trim();
        if run_id.is_empty() {
            return Err(ExecutionThreadError::InvalidQuery);
        }

        let timeline = get_execution_timeline(
            &self.storage.execution_traces,
            &ExecutionTraceQuery {
                run_id: Some(run_id.to_string()),
                limit: Some(usize::MAX),
                ..ExecutionTraceQuery::default()
            },
        )
        .map_err(ExecutionThreadError::from)?;
        if !timeline.events.is_empty() {
            return Ok(timeline);
        }

        match self.get_run_thread(run_id) {
            Ok(thread) => Ok(thread.timeline),
            Err(ExecutionThreadError::RunNotFound(_)) => Ok(timeline),
            Err(error) => Err(error),
        }
    }

    pub fn list_child_runs(&self, parent_run_id: &str) -> Result<Vec<RunSummary>> {
        let parent_run_id = parent_run_id.trim();
        if parent_run_id.is_empty() {
            return Ok(Vec::new());
        }

        let seed_events = self.storage.execution_traces.query(&ExecutionTraceQuery {
            parent_run_id: Some(parent_run_id.to_string()),
            limit: Some(usize::MAX),
            ..ExecutionTraceQuery::default()
        })?;

        let child_run_ids = seed_events
            .iter()
            .filter_map(|event| event.run_id.clone())
            .collect::<HashSet<_>>();
        if child_run_ids.is_empty() {
            return self.list_session_child_runs(parent_run_id);
        }

        let events = self.storage.execution_traces.query(&ExecutionTraceQuery {
            limit: Some(usize::MAX),
            ..ExecutionTraceQuery::default()
        })?;

        let mut groups: HashMap<String, Vec<ExecutionTraceEvent>> = HashMap::new();
        for event in events {
            let Some(run_id) = event.run_id.clone() else {
                continue;
            };
            if !child_run_ids.contains(&run_id) {
                continue;
            }
            groups.entry(run_id).or_default().push(event);
        }

        let mut sessions = groups
            .into_iter()
            .map(|(run_id, mut events)| -> Result<RunSummary> {
                sort_trace_events(&mut events);
                let root = self.resolve_root_run_context(
                    &run_id,
                    events
                        .last()
                        .and_then(|event| event.parent_run_id.as_deref()),
                )?;
                Ok(self.build_run_summary(
                    &run_id,
                    &root.container_id,
                    RunKind::SubagentRun,
                    &events,
                    RunSummaryMeta {
                        title: Some("Subagent run".to_string()),
                        ..RunSummaryMeta::default()
                    },
                    Some(root.root_run_id),
                ))
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

    fn build_workspace_container(
        &self,
        context: &SessionContext,
        latest_run_id: Option<String>,
    ) -> ExecutionContainerSummary {
        ExecutionContainerSummary {
            id: context.session.id.clone(),
            kind: ExecutionContainerKind::Workspace,
            title: context.session.name.clone(),
            subtitle: context
                .session
                .messages
                .last()
                .map(|message| truncate_text(&message.content, 72)),
            updated_at: context.session.updated_at,
            status: Some(if context.session.messages.is_empty() {
                "pending".to_string()
            } else {
                "completed".to_string()
            }),
            session_count: 0,
            latest_session_id: Some(context.session.id.clone()),
            latest_run_id,
            agent_id: Some(context.session.agent_id.clone()),
            source_channel: Some(ChatSessionSource::Workspace),
            source_conversation_id: None,
        }
    }

    fn load_latest_top_level_runs(&self) -> Result<HashMap<String, LatestRunProjection>> {
        let events = self.storage.execution_traces.query(&ExecutionTraceQuery {
            limit: Some(usize::MAX),
            ..ExecutionTraceQuery::default()
        })?;

        let mut projections = HashMap::new();
        for event in events {
            if event.parent_run_id.is_some() {
                continue;
            }

            let Some(run_id) = event.run_id.clone() else {
                continue;
            };

            let entry = projections
                .entry(run_id.clone())
                .or_insert_with(|| LatestRunProjection {
                    run_id: run_id.clone(),
                    updated_at: event.timestamp,
                    session_id: event.session_id.clone(),
                    task_id: event.task_id.clone(),
                });

            if event.timestamp >= entry.updated_at {
                entry.updated_at = event.timestamp;
                entry.session_id = event.session_id.clone();
                entry.task_id = event.task_id.clone();
            }
        }

        Ok(projections)
    }

    fn load_session_contexts(&self) -> Result<Vec<SessionContext>> {
        let session_service = SessionService::from_storage(&self.storage);
        let sessions = session_service.list_session_views(None, None, false)?;
        let mut bound_tasks_by_session_id = HashMap::new();
        for task in self.storage.tasks.list_tasks()? {
            let trimmed_session_id = task.chat_session_id.trim();
            if trimmed_session_id.is_empty() {
                continue;
            }
            bound_tasks_by_session_id.insert(trimmed_session_id.to_string(), task);
        }
        let mut contexts = Vec::with_capacity(sessions.len());

        for session in sessions {
            let (source, conversation_id) = session_service.effective_source(&session)?;
            let source = EffectiveSessionSource {
                source,
                conversation_id,
            };
            let bound_task = bound_tasks_by_session_id.get(&session.id).cloned();
            contexts.push(SessionContext {
                session,
                source,
                bound_task,
            });
        }

        Ok(contexts)
    }

    fn list_workspace_runs(&self, session_id: &str) -> Result<Vec<RunSummary>> {
        let session_service = SessionService::from_storage(&self.storage);
        let session = session_service
            .get_session_view(session_id)?
            .ok_or_else(|| anyhow!("workspace session '{}' not found", session_id))?;
        let policy = SessionPolicy::from_storage(&self.storage);
        let (source, conversation_id) = session_service.effective_source(&session)?;
        let source = EffectiveSessionSource {
            source,
            conversation_id,
        };
        let bound_task = policy.bound_task(session_id)?;
        if source.source != ChatSessionSource::Workspace || bound_task.is_some() {
            return Err(anyhow!("workspace session '{}' not found", session_id));
        }

        self.list_session_runs(
            &session,
            &session.id,
            RunKind::WorkspaceRun,
            Some(source.source),
            None,
            Some(session.name.clone()),
        )
    }

    fn list_task_sessions(&self, task_id: &str) -> Result<Vec<RunSummary>> {
        let task = self
            .storage
            .tasks
            .get_task(task_id)?
            .ok_or_else(|| anyhow!("task '{}' not found", task_id))?;
        self.list_task_runs(&task)
    }

    fn list_task_runs(&self, task: &Task) -> Result<Vec<RunSummary>> {
        let task_runs = self.storage.tasks.list_task_runs(&task.id)?;
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
                sort_trace_events(&mut run_events);
                self.build_run_summary(
                    &run_id,
                    &task.id,
                    RunKind::TaskRun,
                    &run_events,
                    RunSummaryMeta {
                        title: Some(format_run_title(
                            run_events
                                .first()
                                .map(|event| event.timestamp)
                                .unwrap_or(task.updated_at),
                        )),
                        subtitle: Some(task.name.clone()),
                        ..RunSummaryMeta::default()
                    },
                    Some(run_id.clone()),
                )
            })
            .collect::<Vec<_>>();
        let traced_run_ids = runs
            .iter()
            .filter_map(|run| run.run_id.clone())
            .collect::<HashSet<_>>();
        runs.extend(
            task_runs
                .iter()
                .filter(|run| !traced_run_ids.contains(&run.run_id))
                .map(|run| self.build_task_run_summary(task, run)),
        );

        runs.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(runs)
    }

    fn build_task_run_summary(&self, task: &Task, run: &TaskRun) -> RunSummary {
        RunSummary {
            id: run.run_id.clone(),
            kind: RunKind::TaskRun,
            container_id: task.id.clone(),
            root_run_id: Some(run.run_id.clone()),
            title: format_run_title(run.started_at),
            subtitle: Some(task.name.clone()),
            status: run.status.as_str().to_string(),
            updated_at: run.updated_at,
            started_at: Some(run.started_at),
            ended_at: run.ended_at,
            session_id: Some(task.chat_session_id.clone()),
            run_id: Some(run.run_id.clone()),
            task_id: Some(task.id.clone()),
            parent_run_id: None,
            agent_id: Some(task.agent_id.clone()),
            source_channel: None,
            source_conversation_id: None,
            effective_model: None,
            provider: None,
            event_count: 0,
        }
    }

    fn list_session_runs(
        &self,
        session: &ChatSession,
        container_id: &str,
        kind: RunKind,
        source_channel: Option<ChatSessionSource>,
        source_conversation_id: Option<String>,
        subtitle: Option<String>,
    ) -> Result<Vec<RunSummary>> {
        let events = self.storage.execution_traces.query(&ExecutionTraceQuery {
            session_id: Some(session.id.clone()),
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
                sort_trace_events(&mut run_events);
                self.build_run_summary(
                    &run_id,
                    container_id,
                    kind,
                    &run_events,
                    RunSummaryMeta {
                        title: Some(format_run_title(
                            run_events
                                .first()
                                .map(|event| event.timestamp)
                                .unwrap_or(session.updated_at),
                        )),
                        subtitle: subtitle.clone(),
                        source_channel,
                        source_conversation_id: source_conversation_id.clone(),
                    },
                    Some(run_id.clone()),
                )
            })
            .collect::<Vec<_>>();

        let traced_run_ids = runs
            .iter()
            .filter_map(|run| run.run_id.clone())
            .collect::<HashSet<_>>();
        runs.extend(session.turns.iter().filter_map(|turn| {
            if traced_run_ids.contains(&turn.id) {
                return None;
            }
            Some(build_turn_run_summary(
                session,
                turn,
                container_id,
                kind,
                source_channel,
                source_conversation_id.clone(),
                subtitle.clone(),
            ))
        }));

        runs.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.id.cmp(&right.id))
        });

        Ok(runs)
    }

    fn build_run_summary(
        &self,
        run_id: &str,
        container_id: &str,
        kind: RunKind,
        events: &[ExecutionTraceEvent],
        meta: RunSummaryMeta,
        root_run_id: Option<String>,
    ) -> RunSummary {
        let first = events.first();
        let last = events.last();
        let started_at = first.map(|event| event.timestamp);
        let ended_at = latest_terminal_timestamp(events);
        let status = latest_lifecycle_status(events).unwrap_or_else(|| "running".to_string());
        let session_id = last.and_then(|event| event.session_id.clone());
        let task_id = last.map(|event| event.task_id.clone());
        let parent_run_id = last.and_then(|event| event.parent_run_id.clone());
        let agent_id = last.map(|event| event.agent_id.clone());
        let effective_model = latest_effective_model(events);
        let provider = latest_provider(events);

        RunSummary {
            id: run_id.to_string(),
            kind,
            container_id: container_id.to_string(),
            root_run_id,
            title: meta
                .title
                .unwrap_or_else(|| format!("Run {}", short_id(run_id))),
            subtitle: meta.subtitle,
            status,
            updated_at: last.map(|event| event.timestamp).unwrap_or_default(),
            started_at,
            ended_at,
            session_id,
            run_id: Some(run_id.to_string()),
            task_id,
            parent_run_id,
            agent_id,
            source_channel: meta.source_channel,
            source_conversation_id: meta.source_conversation_id,
            effective_model,
            provider,
            event_count: events.len() as u64,
        }
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
            return self.get_turn_thread(run_id);
        }
        let focus = self
            .build_focus_for_run(run_id, &timeline.events)
            .map_err(ExecutionThreadError::from)?;
        Ok(ExecutionThread { focus, timeline })
    }

    fn get_turn_thread(
        &self,
        run_id: &str,
    ) -> std::result::Result<ExecutionThread, ExecutionThreadError> {
        let session_service = SessionService::from_storage(&self.storage);
        let policy = SessionPolicy::from_storage(&self.storage);
        let sessions = session_service
            .list_session_views(None, None, true)
            .map_err(ExecutionThreadError::from)?;

        for session in sessions {
            let Some(turn) = session.turns.iter().find(|turn| turn.id == run_id) else {
                continue;
            };
            let (kind, container_id, subtitle, source_channel, source_conversation_id) =
                if let Some(task) = policy
                    .bound_task(&session.id)
                    .map_err(ExecutionThreadError::from)?
                {
                    (RunKind::TaskRun, task.id, Some(task.name), None, None)
                } else {
                    let (source, conversation_id) = session_service
                        .effective_source(&session)
                        .map_err(ExecutionThreadError::from)?;
                    (
                        RunKind::WorkspaceRun,
                        session.id.clone(),
                        Some(session.name.clone()),
                        Some(source),
                        conversation_id,
                    )
                };
            let focus = build_turn_run_summary(
                &session,
                turn,
                &container_id,
                kind,
                source_channel,
                source_conversation_id,
                subtitle,
            );
            let events = turn_execution_trace_events(&session, turn, &container_id);
            let stats = execution_trace_stats_for_events(&events);
            return Ok(ExecutionThread {
                focus,
                timeline: ExecutionTimeline { events, stats },
            });
        }

        if let Some(thread) = self.find_session_subagent_thread(run_id)? {
            return Ok(thread);
        }
        if let Some(thread) = self.find_task_run_record_thread(run_id)? {
            return Ok(thread);
        }

        Err(ExecutionThreadError::RunNotFound(run_id.to_string()))
    }

    fn find_task_run_record_thread(
        &self,
        run_id: &str,
    ) -> std::result::Result<Option<ExecutionThread>, ExecutionThreadError> {
        let Some(task_run) = self
            .storage
            .tasks
            .get_task_run(run_id)
            .map_err(ExecutionThreadError::from)?
        else {
            return Ok(None);
        };
        let Some(task) = self
            .storage
            .tasks
            .get_task(&task_run.task_id)
            .map_err(ExecutionThreadError::from)?
        else {
            return Ok(None);
        };

        let session_service = SessionService::from_storage(&self.storage);
        let mut events = session_service
            .get_session_view(&task.chat_session_id)
            .map_err(ExecutionThreadError::from)?
            .and_then(|session| {
                session
                    .turns
                    .iter()
                    .max_by_key(|turn| turn.updated_at)
                    .map(|turn| turn_execution_trace_events(&session, turn, &task.id))
            })
            .unwrap_or_default();
        if events.is_empty()
            && let Some(session) = session_service
                .get_session_view(&task.chat_session_id)
                .map_err(ExecutionThreadError::from)?
        {
            events = legacy_message_trace_events(&session, &task, &task_run.run_id);
        }
        for event in &mut events {
            event.run_id = Some(task_run.run_id.clone());
            event.session_id = Some(task.chat_session_id.clone());
            event.task_id = task.id.clone();
            event.agent_id = task.agent_id.clone();
        }

        let stats = execution_trace_stats_for_events(&events);
        let mut focus = self.build_task_run_summary(&task, &task_run);
        focus.event_count = events.len() as u64;
        Ok(Some(ExecutionThread {
            focus,
            timeline: ExecutionTimeline { events, stats },
        }))
    }

    fn list_session_child_runs(&self, parent_run_id: &str) -> Result<Vec<RunSummary>> {
        let session_service = SessionService::from_storage(&self.storage);
        let Some(session) = session_service.get_session_view_by_turn_id(parent_run_id)? else {
            return Ok(Vec::new());
        };
        let mut runs = Vec::new();
        let Some(turn) = session.turns.iter().find(|turn| turn.id == parent_run_id) else {
            return Ok(Vec::new());
        };
        for (event_timestamp, projection) in subagent_results_for_turn(turn) {
            runs.push(subagent_result_run_summary(
                &session,
                turn,
                event_timestamp,
                &projection,
            ));
        }
        runs.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(runs)
    }

    fn find_session_subagent_thread(
        &self,
        run_id: &str,
    ) -> std::result::Result<Option<ExecutionThread>, ExecutionThreadError> {
        let session_service = SessionService::from_storage(&self.storage);
        let sessions = session_service
            .list_session_views(None, None, true)
            .map_err(ExecutionThreadError::from)?;
        for session in sessions {
            for turn in &session.turns {
                for (event_timestamp, projection) in subagent_results_for_turn(turn) {
                    if projection.task_id != run_id {
                        continue;
                    }
                    let focus =
                        subagent_result_run_summary(&session, turn, event_timestamp, &projection);
                    let events =
                        subagent_result_trace_events(&session, turn, event_timestamp, &projection);
                    let stats = execution_trace_stats_for_events(&events);
                    return Ok(Some(ExecutionThread {
                        focus,
                        timeline: ExecutionTimeline { events, stats },
                    }));
                }
            }
        }
        Ok(None)
    }

    fn load_run_events(&self, run_id: &str) -> Result<Vec<ExecutionTraceEvent>> {
        let mut events = self.storage.execution_traces.query(&ExecutionTraceQuery {
            run_id: Some(run_id.to_string()),
            limit: Some(usize::MAX),
            ..ExecutionTraceQuery::default()
        })?;
        sort_trace_events(&mut events);
        Ok(events)
    }

    fn resolve_root_run_context(
        &self,
        run_id: &str,
        parent_run_id: Option<&str>,
    ) -> Result<RootRunContext> {
        let Some(initial_parent_run_id) = parent_run_id else {
            return Ok(RootRunContext {
                container_id: run_id.to_string(),
                root_run_id: run_id.to_string(),
            });
        };

        let mut current_run_id = initial_parent_run_id.to_string();
        let mut visited = HashSet::new();

        loop {
            if !visited.insert(current_run_id.clone()) {
                return Err(anyhow!(
                    "detected cyclic run ancestry while resolving root run for '{}'",
                    run_id
                ));
            }

            let events = self.load_run_events(&current_run_id)?;
            let latest = events
                .last()
                .ok_or_else(|| anyhow!("run '{}' has no events", current_run_id))?;

            if latest.parent_run_id.is_none() {
                let focus = self.build_focus_for_run(&current_run_id, &events)?;
                return Ok(RootRunContext {
                    container_id: focus.container_id,
                    root_run_id: current_run_id,
                });
            }

            current_run_id = latest
                .parent_run_id
                .clone()
                .ok_or_else(|| anyhow!("run '{}' ancestry is incomplete", current_run_id))?;
        }
    }

    fn build_focus_for_run(
        &self,
        run_id: &str,
        events: &[ExecutionTraceEvent],
    ) -> Result<RunSummary> {
        let latest = events
            .last()
            .ok_or_else(|| anyhow!("run '{}' has no events", run_id))?;
        if latest.parent_run_id.is_none()
            && let Ok(Some(task)) = self.storage.tasks.get_task(&latest.task_id)
        {
            return Ok(self.build_run_summary(
                run_id,
                &task.id,
                RunKind::TaskRun,
                events,
                RunSummaryMeta {
                    title: Some(format_run_title(
                        events
                            .first()
                            .map(|event| event.timestamp)
                            .unwrap_or(task.updated_at),
                    )),
                    subtitle: Some(task.name),
                    ..RunSummaryMeta::default()
                },
                Some(run_id.to_string()),
            ));
        }

        if latest.parent_run_id.is_some() {
            let root = self.resolve_root_run_context(run_id, latest.parent_run_id.as_deref())?;
            return Ok(self.build_run_summary(
                run_id,
                &root.container_id,
                RunKind::SubagentRun,
                events,
                RunSummaryMeta {
                    title: Some(format_run_title(
                        events
                            .first()
                            .map(|event| event.timestamp)
                            .unwrap_or(latest.timestamp),
                    )),
                    subtitle: latest.session_id.clone(),
                    ..RunSummaryMeta::default()
                },
                Some(root.root_run_id),
            ));
        }

        let session_service = SessionService::from_storage(&self.storage);
        if let Some(session_id) = latest.session_id.as_deref()
            && let Ok(Some(session)) = session_service.get_session_view(session_id)
        {
            let policy = SessionPolicy::from_storage(&self.storage);
            let (source, conversation_id) = session_service.effective_source(&session)?;
            let source = EffectiveSessionSource {
                source,
                conversation_id,
            };
            if let Some(task) = policy.bound_task(session_id)? {
                return Ok(self.build_run_summary(
                    run_id,
                    &task.id,
                    RunKind::TaskRun,
                    events,
                    RunSummaryMeta {
                        title: Some(format_run_title(
                            events
                                .first()
                                .map(|event| event.timestamp)
                                .unwrap_or(latest.timestamp),
                        )),
                        subtitle: Some(task.name),
                        ..RunSummaryMeta::default()
                    },
                    Some(run_id.to_string()),
                ));
            }

            return Ok(self.build_run_summary(
                run_id,
                &session.id,
                RunKind::WorkspaceRun,
                events,
                RunSummaryMeta {
                    title: Some(format_run_title(
                        events
                            .first()
                            .map(|event| event.timestamp)
                            .unwrap_or(latest.timestamp),
                    )),
                    subtitle: Some(session.name),
                    source_channel: Some(source.source),
                    source_conversation_id: source.conversation_id,
                },
                Some(run_id.to_string()),
            ));
        }

        Ok(self.build_run_summary(
            run_id,
            run_id,
            RunKind::SubagentRun,
            events,
            RunSummaryMeta {
                title: Some(format_run_title(
                    events
                        .first()
                        .map(|event| event.timestamp)
                        .unwrap_or(latest.timestamp),
                )),
                subtitle: latest.session_id.clone(),
                ..RunSummaryMeta::default()
            },
            Some(run_id.to_string()),
        ))
    }
}

fn build_turn_run_summary(
    session: &ChatSession,
    turn: &ChatTurn,
    container_id: &str,
    kind: RunKind,
    source_channel: Option<ChatSessionSource>,
    source_conversation_id: Option<String>,
    subtitle: Option<String>,
) -> RunSummary {
    RunSummary {
        id: turn.id.clone(),
        kind,
        container_id: container_id.to_string(),
        root_run_id: Some(turn.id.clone()),
        title: format_run_title(turn.started_at),
        subtitle,
        status: chat_turn_status_label(turn.status).to_string(),
        updated_at: turn.updated_at,
        started_at: Some(turn.started_at),
        ended_at: turn.completed_at,
        session_id: Some(session.id.clone()),
        run_id: Some(turn.id.clone()),
        task_id: None,
        parent_run_id: None,
        agent_id: Some(session.agent_id.clone()),
        source_channel,
        source_conversation_id,
        effective_model: Some(session.model.clone()),
        provider: Some(session.provider.clone()),
        event_count: turn.events.len() as u64,
    }
}

fn subagent_results_for_turn(turn: &ChatTurn) -> Vec<(i64, SubagentResultProjection)> {
    let mut projections = Vec::new();
    for event in &turn.events {
        let ChatTurnEventKind::ToolResult {
            success: true,
            result,
            ..
        } = &event.kind
        else {
            continue;
        };
        projections.extend(
            parse_subagent_result_projections(result)
                .into_iter()
                .map(|projection| (event.timestamp, projection)),
        );
    }
    projections
}

fn parse_subagent_result_projections(result: &str) -> Vec<SubagentResultProjection> {
    let Ok(value) = serde_json::from_str::<Value>(result) else {
        return Vec::new();
    };
    let Some(results) = value.get("results").and_then(Value::as_array) else {
        return Vec::new();
    };
    results
        .iter()
        .filter_map(|entry| {
            let task_id = entry
                .get("task_id")
                .and_then(Value::as_str)?
                .trim()
                .to_string();
            if task_id.is_empty() {
                return None;
            }
            Some(SubagentResultProjection {
                task_id,
                agent: entry
                    .get("agent")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string),
                task: entry
                    .get("task")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string),
                status: entry
                    .get("status")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("completed")
                    .to_string(),
                output: entry
                    .get("output")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string),
                duration_ms: entry.get("duration_ms").and_then(Value::as_i64),
            })
        })
        .collect()
}

fn subagent_result_run_summary(
    session: &ChatSession,
    turn: &ChatTurn,
    event_timestamp: i64,
    projection: &SubagentResultProjection,
) -> RunSummary {
    let terminal = is_terminal_run_status(&projection.status);
    RunSummary {
        id: projection.task_id.clone(),
        kind: RunKind::SubagentRun,
        container_id: session.id.clone(),
        root_run_id: Some(turn.id.clone()),
        title: projection
            .agent
            .as_deref()
            .map(|agent| format!("Subagent run: {agent}"))
            .unwrap_or_else(|| "Subagent run".to_string()),
        subtitle: projection.task.clone().or_else(|| {
            projection
                .output
                .as_ref()
                .map(|output| compact_summary(output, 80))
        }),
        status: projection.status.clone(),
        updated_at: event_timestamp,
        started_at: Some(turn.started_at),
        ended_at: terminal.then_some(event_timestamp),
        session_id: Some(session.id.clone()),
        run_id: Some(projection.task_id.clone()),
        task_id: None,
        parent_run_id: Some(turn.id.clone()),
        agent_id: Some(session.agent_id.clone()),
        source_channel: session.source_channel,
        source_conversation_id: session.source_conversation_id.clone(),
        effective_model: Some(session.model.clone()),
        provider: Some(session.provider.clone()),
        event_count: if projection.output.is_some() { 2 } else { 1 },
    }
}

fn subagent_result_trace_events(
    session: &ChatSession,
    turn: &ChatTurn,
    event_timestamp: i64,
    projection: &SubagentResultProjection,
) -> Vec<ExecutionTraceEvent> {
    let mut events = vec![ExecutionTraceEvent {
        id: format!("{}:lifecycle", projection.task_id),
        task_id: projection.task_id.clone(),
        agent_id: session.agent_id.clone(),
        category: ExecutionTraceCategory::Lifecycle,
        source: ExecutionTraceSource::Runtime,
        timestamp: event_timestamp,
        subflow_path: Vec::new(),
        run_id: Some(projection.task_id.clone()),
        parent_run_id: Some(turn.id.clone()),
        session_id: Some(session.id.clone()),
        turn_id: Some(turn.id.clone()),
        requested_model: None,
        effective_model: Some(session.model.clone()),
        provider: Some(session.provider.clone()),
        attempt: None,
        llm_call: None,
        tool_call: None,
        model_switch: None,
        lifecycle: Some(LifecycleTrace {
            status: projection.status.clone(),
            message: Some(
                projection
                    .agent
                    .as_deref()
                    .map(|agent| format!("Subagent {agent} {}", projection.status))
                    .unwrap_or_else(|| format!("Subagent {}", projection.status)),
            ),
            error: None,
            ai_duration_ms: projection.duration_ms,
        }),
        message: None,
        metric_sample: None,
        provider_health: None,
        log_record: None,
    }];
    if let Some(output) = projection.output.as_ref() {
        events.push(ExecutionTraceEvent {
            id: format!("{}:message", projection.task_id),
            task_id: projection.task_id.clone(),
            agent_id: session.agent_id.clone(),
            category: ExecutionTraceCategory::Message,
            source: ExecutionTraceSource::Runtime,
            timestamp: event_timestamp,
            subflow_path: Vec::new(),
            run_id: Some(projection.task_id.clone()),
            parent_run_id: Some(turn.id.clone()),
            session_id: Some(session.id.clone()),
            turn_id: Some(turn.id.clone()),
            requested_model: None,
            effective_model: Some(session.model.clone()),
            provider: Some(session.provider.clone()),
            attempt: None,
            llm_call: None,
            tool_call: None,
            model_switch: None,
            lifecycle: None,
            message: Some(MessageTrace {
                role: "assistant".to_string(),
                content_preview: Some(output.clone()),
                tool_call_count: None,
            }),
            metric_sample: None,
            provider_health: None,
            log_record: None,
        });
    }
    events
}

fn is_terminal_run_status(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "completed" | "failed" | "canceled" | "cancelled" | "interrupted" | "timed_out"
    )
}

fn compact_summary(value: &str, max_chars: usize) -> String {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.chars().count() <= max_chars {
        return value;
    }
    let keep = max_chars.saturating_sub(3);
    let mut output = value.chars().take(keep).collect::<String>();
    output.push_str("...");
    output
}

fn turn_execution_trace_events(
    session: &ChatSession,
    turn: &ChatTurn,
    container_id: &str,
) -> Vec<ExecutionTraceEvent> {
    turn.events
        .iter()
        .map(|event| {
            let (category, lifecycle, message, tool_call) = match &event.kind {
                ChatTurnEventKind::UserMessage { content } => (
                    ExecutionTraceCategory::Message,
                    None,
                    Some(MessageTrace {
                        role: "user".to_string(),
                        content_preview: Some(content.clone()),
                        tool_call_count: None,
                    }),
                    None,
                ),
                ChatTurnEventKind::AssistantMessage { content } => (
                    ExecutionTraceCategory::Message,
                    None,
                    Some(MessageTrace {
                        role: "assistant".to_string(),
                        content_preview: Some(content.clone()),
                        tool_call_count: None,
                    }),
                    None,
                ),
                ChatTurnEventKind::ToolCall {
                    call_id,
                    name,
                    arguments,
                } => (
                    ExecutionTraceCategory::ToolCall,
                    None,
                    None,
                    Some(ToolCallTrace {
                        phase: ToolCallPhase::Started,
                        tool_call_id: call_id.clone(),
                        tool_name: name.clone(),
                        input: Some(arguments.clone()),
                        input_summary: None,
                        output: None,
                        output_ref: None,
                        success: None,
                        error: None,
                        duration_ms: None,
                    }),
                ),
                ChatTurnEventKind::ToolResult {
                    call_id,
                    success,
                    result,
                } => (
                    ExecutionTraceCategory::ToolCall,
                    None,
                    None,
                    Some(ToolCallTrace {
                        phase: ToolCallPhase::Completed,
                        tool_call_id: call_id.clone(),
                        tool_name: "tool".to_string(),
                        input: None,
                        input_summary: None,
                        output: if *success { Some(result.clone()) } else { None },
                        output_ref: None,
                        success: Some(*success),
                        error: if *success { None } else { Some(result.clone()) },
                        duration_ms: None,
                    }),
                ),
                ChatTurnEventKind::Error { message } => (
                    ExecutionTraceCategory::Lifecycle,
                    Some(LifecycleTrace {
                        status: "failed".to_string(),
                        message: Some(message.clone()),
                        error: Some(message.clone()),
                        ai_duration_ms: None,
                    }),
                    None,
                    None,
                ),
                ChatTurnEventKind::Canceled => (
                    ExecutionTraceCategory::Lifecycle,
                    Some(LifecycleTrace {
                        status: "canceled".to_string(),
                        message: Some("Turn canceled".to_string()),
                        error: None,
                        ai_duration_ms: None,
                    }),
                    None,
                    None,
                ),
            };
            ExecutionTraceEvent {
                id: event.id.clone(),
                task_id: container_id.to_string(),
                agent_id: session.agent_id.clone(),
                category,
                source: ExecutionTraceSource::Runtime,
                timestamp: event.timestamp,
                subflow_path: Vec::new(),
                run_id: Some(turn.id.clone()),
                parent_run_id: None,
                session_id: Some(session.id.clone()),
                turn_id: Some(turn.id.clone()),
                requested_model: None,
                effective_model: Some(session.model.clone()),
                provider: Some(session.provider.clone()),
                attempt: None,
                llm_call: None,
                tool_call,
                model_switch: None,
                lifecycle,
                message,
                metric_sample: None,
                provider_health: None,
                log_record: None,
            }
        })
        .collect()
}

fn legacy_message_trace_events(
    session: &ChatSession,
    task: &Task,
    run_id: &str,
) -> Vec<ExecutionTraceEvent> {
    session
        .messages
        .iter()
        .filter_map(|message| {
            let role = match message.role {
                ChatRole::User => "user",
                ChatRole::Assistant => "assistant",
                ChatRole::System => return None,
            };
            Some(ExecutionTraceEvent {
                id: message.id.clone(),
                task_id: task.id.clone(),
                agent_id: task.agent_id.clone(),
                category: ExecutionTraceCategory::Message,
                source: ExecutionTraceSource::Runtime,
                timestamp: message.timestamp,
                subflow_path: Vec::new(),
                run_id: Some(run_id.to_string()),
                parent_run_id: None,
                session_id: Some(session.id.clone()),
                turn_id: None,
                requested_model: None,
                effective_model: Some(session.model.clone()),
                provider: Some(session.provider.clone()),
                attempt: None,
                llm_call: None,
                tool_call: None,
                model_switch: None,
                lifecycle: None,
                message: Some(MessageTrace {
                    role: role.to_string(),
                    content_preview: Some(message.content.clone()),
                    tool_call_count: None,
                }),
                metric_sample: None,
                provider_health: None,
                log_record: None,
            })
        })
        .collect()
}

fn chat_turn_status_label(status: ChatTurnStatus) -> &'static str {
    match status {
        ChatTurnStatus::Running => "running",
        ChatTurnStatus::Completed => "completed",
        ChatTurnStatus::Canceled => "canceled",
        ChatTurnStatus::Failed => "failed",
    }
}

fn sort_trace_events(events: &mut [ExecutionTraceEvent]) {
    events.sort_by(compare_trace_events);
}

fn compare_trace_events(left: &ExecutionTraceEvent, right: &ExecutionTraceEvent) -> Ordering {
    left.timestamp
        .cmp(&right.timestamp)
        .then_with(|| lifecycle_sort_rank(left).cmp(&lifecycle_sort_rank(right)))
        .then_with(|| left.id.cmp(&right.id))
}

fn lifecycle_sort_rank(event: &ExecutionTraceEvent) -> u8 {
    let Some(lifecycle) = event.lifecycle.as_ref() else {
        return 1;
    };
    match lifecycle.status.to_ascii_lowercase().as_str() {
        "running" | "started" | "starting" => 0,
        "completed" | "failed" | "interrupted" | "cancelled" | "canceled" => 2,
        _ => 1,
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

fn execution_container_sort_key(container: &ExecutionContainerSummary) -> u8 {
    match container.kind {
        ExecutionContainerKind::Workspace => 0,
        ExecutionContainerKind::Task => 1,
    }
}

fn latest_run_by_session_id(
    projections: &HashMap<String, LatestRunProjection>,
) -> HashMap<&str, &LatestRunProjection> {
    let mut by_session_id = HashMap::new();
    for projection in projections.values() {
        let Some(session_id) = projection.session_id.as_deref() else {
            continue;
        };
        let replace = by_session_id
            .get(session_id)
            .map(|existing: &&LatestRunProjection| projection.updated_at >= existing.updated_at)
            .unwrap_or(true);
        if replace {
            by_session_id.insert(session_id, projection);
        }
    }
    by_session_id
}

fn latest_run_by_task_id(
    projections: &HashMap<String, LatestRunProjection>,
) -> HashMap<&str, &LatestRunProjection> {
    let mut by_task_id = HashMap::new();
    for projection in projections.values() {
        let task_id = projection.task_id.as_str();
        if task_id.trim().is_empty() {
            continue;
        }
        let replace = by_task_id
            .get(task_id)
            .map(|existing: &&LatestRunProjection| projection.updated_at >= existing.updated_at)
            .unwrap_or(true);
        if replace {
            by_task_id.insert(task_id, projection);
        }
    }
    by_task_id
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        ChatMessage, ChatSession, ExecutionContainerRef, ExecutionMode, LifecycleTrace,
        TaskRunStatus, TaskSchedule, TaskSpec, execution_trace_builders,
    };
    use crate::storage::Storage;
    use crate::{ExecutionTraceCategory, ExecutionTraceSource};
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn create_storage() -> (Arc<Storage>, TempDir) {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let db_path = temp_dir.path().join("execution-console-tests.redb");
        let storage = Arc::new(Storage::new(db_path.to_str().expect("db path")).expect("storage"));
        (storage, temp_dir)
    }

    fn store_run_events(
        storage: &Arc<Storage>,
        task_id: &str,
        session_id: &str,
        run_id: &str,
        parent_run_id: Option<&str>,
    ) {
        let trace = ai::telemetry::RestflowTrace::new(
            run_id.to_string(),
            session_id.to_string(),
            task_id.to_string(),
            "agent-1".to_string(),
        )
        .with_parent_run_id(parent_run_id.map(|value| value.to_string()));
        let start = execution_trace_builders::with_provider(
            execution_trace_builders::with_effective_model(
                execution_trace_builders::with_trace_context(
                    execution_trace_builders::lifecycle(
                        task_id,
                        "agent-1",
                        LifecycleTrace {
                            status: "running".to_string(),
                            message: Some("started".to_string()),
                            error: None,
                            ai_duration_ms: None,
                        },
                    ),
                    &trace,
                ),
                "openai/gpt-5",
            ),
            "openai",
        );
        let end = execution_trace_builders::with_provider(
            execution_trace_builders::with_effective_model(
                execution_trace_builders::with_lifecycle(
                    execution_trace_builders::with_trace_context(
                        execution_trace_builders::new_event(
                            task_id,
                            "agent-1",
                            ExecutionTraceCategory::Lifecycle,
                            ExecutionTraceSource::Runtime,
                        ),
                        &trace,
                    ),
                    LifecycleTrace {
                        status: "completed".to_string(),
                        message: Some("done".to_string()),
                        error: None,
                        ai_duration_ms: Some(1200),
                    },
                ),
                "openai/gpt-5",
            ),
            "openai",
        );
        storage.execution_traces.store(&start).expect("store start");
        storage.execution_traces.store(&end).expect("store end");
    }

    #[test]
    fn lists_workspace_containers() {
        let (storage, _temp_dir) = create_storage();
        let service = ExecutionConsoleService::from_storage(&storage);

        let mut workspace = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        workspace.name = "Workspace Session".to_string();
        workspace.add_message(ChatMessage::user("hello"));
        storage
            .chat_sessions
            .create(&workspace)
            .expect("workspace session");

        store_run_events(
            &storage,
            &workspace.id,
            &workspace.id,
            "run-workspace",
            None,
        );

        let containers = service.list_execution_containers().expect("containers");
        let workspace_container = containers
            .iter()
            .find(|container| container.id == workspace.id)
            .expect("workspace container");
        assert_eq!(
            workspace_container.latest_run_id.as_deref(),
            Some("run-workspace")
        );
    }

    #[test]
    fn excludes_background_bound_sessions_from_workspace_projection() {
        let (storage, _temp_dir) = create_storage();
        let service = ExecutionConsoleService::from_storage(&storage);

        let mut workspace = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        workspace.name = "Workspace Session".to_string();
        storage
            .chat_sessions
            .create(&workspace)
            .expect("workspace session");

        let task = storage
            .tasks
            .create_task_from_spec(TaskSpec {
                name: "Digest".to_string(),
                description: None,
                agent_id: "agent-1".to_string(),
                chat_session_id: None,
                input: Some("digest".to_string()),
                input_template: None,
                schedule: TaskSchedule::default(),
                execution_mode: Some(ExecutionMode::default()),
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("task");

        let containers = service.list_execution_containers().expect("containers");
        let workspace_container = containers
            .iter()
            .find(|container| container.id == workspace.id)
            .expect("workspace container");
        assert!(workspace_container.latest_session_id.as_deref() == Some(workspace.id.as_str()));
        assert!(
            containers
                .iter()
                .filter(|container| container.kind == ExecutionContainerKind::Workspace)
                .all(|container| container.id != task.chat_session_id)
        );
    }

    #[test]
    fn lists_workspace_runs_from_session_turns_when_trace_index_is_empty() {
        let (storage, _temp_dir) = create_storage();
        let service = ExecutionConsoleService::from_storage(&storage);

        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        session.name = "Workspace Session".to_string();
        session.record_turn_user_message("turn-1", "run pwd");
        session.record_turn_event(
            "turn-1",
            crate::models::ChatTurnEventKind::ToolCall {
                call_id: "call-1".to_string(),
                name: "bash".to_string(),
                arguments: "{\"command\":\"pwd\"}".to_string(),
            },
        );
        session.complete_turn_with_assistant_message("turn-1", "done");
        storage.chat_sessions.create(&session).expect("session");

        let runs = service
            .list_runs(&RunListQuery {
                container: ExecutionContainerRef {
                    kind: ExecutionContainerKind::Workspace,
                    id: session.id.clone(),
                },
            })
            .expect("workspace runs");

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].run_id.as_deref(), Some("turn-1"));
        assert_eq!(runs[0].status, "completed");
        assert_eq!(runs[0].event_count, 3);
        assert_eq!(runs[0].container_id, session.id);
    }

    #[test]
    fn opens_workspace_thread_from_session_turn_when_trace_index_is_empty() {
        let (storage, _temp_dir) = create_storage();
        let service = ExecutionConsoleService::from_storage(&storage);

        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        session.record_turn_user_message("turn-1", "hello");
        session.complete_turn_with_assistant_message("turn-1", "hi");
        storage.chat_sessions.create(&session).expect("session");

        let thread = service
            .get_execution_run_thread("turn-1")
            .expect("turn thread");

        assert_eq!(thread.focus.run_id.as_deref(), Some("turn-1"));
        assert_eq!(thread.focus.status, "completed");
        assert_eq!(thread.timeline.events.len(), 2);
        assert_eq!(thread.timeline.stats.message_count, 2);
    }

    #[test]
    fn opens_subagent_thread_from_parent_turn_tool_result_when_trace_index_is_empty() {
        let (storage, _temp_dir) = create_storage();
        let service = ExecutionConsoleService::from_storage(&storage);

        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        let session_id = session.id.clone();
        session.record_turn_user_message("parent-run", "spawn workers");
        session.record_turn_event(
            "parent-run",
            ChatTurnEventKind::ToolCall {
                call_id: "call-1".to_string(),
                name: "spawn_subagent_batch".to_string(),
                arguments: "{}".to_string(),
            },
        );
        session.record_turn_event(
            "parent-run",
            ChatTurnEventKind::ToolResult {
                call_id: "call-1".to_string(),
                success: true,
                result: json!({
                    "operation": "spawn",
                    "results": [
                        {
                            "agent": "worker-A",
                            "duration_ms": 25,
                            "output": "EXACT_A_OK",
                            "status": "completed",
                            "task": "Reply with EXACT_A_OK",
                            "task_id": "subagent-run-1"
                        }
                    ],
                    "status": "completed"
                })
                .to_string(),
            },
        );
        session.complete_turn_with_assistant_message("parent-run", "done");
        storage.chat_sessions.create(&session).expect("session");

        let child_runs = service.list_child_runs("parent-run").expect("child runs");
        assert_eq!(child_runs.len(), 1);
        assert_eq!(child_runs[0].run_id.as_deref(), Some("subagent-run-1"));
        assert_eq!(child_runs[0].parent_run_id.as_deref(), Some("parent-run"));
        assert_eq!(child_runs[0].root_run_id.as_deref(), Some("parent-run"));
        assert_eq!(child_runs[0].container_id, session_id);

        let thread = service
            .get_execution_run_thread("subagent-run-1")
            .expect("subagent thread");
        assert_eq!(thread.focus.run_id.as_deref(), Some("subagent-run-1"));
        assert_eq!(thread.focus.parent_run_id.as_deref(), Some("parent-run"));
        assert_eq!(thread.focus.status, "completed");
        assert_eq!(thread.timeline.stats.lifecycle_count, 1);
        assert_eq!(thread.timeline.stats.message_count, 1);
        assert!(thread.timeline.events.iter().any(|event| {
            event
                .message
                .as_ref()
                .and_then(|message| message.content_preview.as_deref())
                == Some("EXACT_A_OK")
        }));
    }

    #[test]
    fn lists_task_runs_and_child_runs() {
        let (storage, _temp_dir) = create_storage();
        let service = ExecutionConsoleService::from_storage(&storage);

        let task = storage
            .tasks
            .create_task_from_spec(TaskSpec {
                name: "Digest".to_string(),
                description: None,
                agent_id: "agent-1".to_string(),
                chat_session_id: None,
                input: Some("digest".to_string()),
                input_template: None,
                schedule: TaskSchedule::default(),
                execution_mode: Some(ExecutionMode::default()),
                timeout_secs: None,
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
            .list_runs(&RunListQuery {
                container: ExecutionContainerRef {
                    kind: ExecutionContainerKind::Task,
                    id: task.id.clone(),
                },
            })
            .expect("task runs");
        let containers = service.list_execution_containers().expect("containers");
        let background_container = containers
            .iter()
            .find(|container| container.id == task.id)
            .expect("background container");
        assert_eq!(
            background_container.latest_run_id.as_deref(),
            Some("run-parent")
        );
        assert!(
            runs.iter()
                .any(|run| run.run_id.as_deref() == Some("run-parent"))
        );

        let child_runs = service.list_child_runs("run-parent").expect("child runs");
        assert_eq!(child_runs.len(), 1);
        assert_eq!(child_runs[0].run_id.as_deref(), Some("run-child"));
        assert_eq!(child_runs[0].container_id, task.id);
        assert_eq!(child_runs[0].parent_run_id.as_deref(), Some("run-parent"));
        assert_eq!(child_runs[0].root_run_id.as_deref(), Some("run-parent"));
    }

    #[test]
    fn lists_task_runs_from_task_run_records_when_trace_index_is_empty() {
        let (storage, _temp_dir) = create_storage();
        let service = ExecutionConsoleService::from_storage(&storage);

        let task = storage
            .tasks
            .create_task_from_spec(TaskSpec {
                name: "Digest".to_string(),
                description: None,
                agent_id: "agent-1".to_string(),
                chat_session_id: None,
                input: Some("digest".to_string()),
                input_template: None,
                schedule: TaskSchedule::default(),
                execution_mode: Some(ExecutionMode::default()),
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("task");
        storage
            .tasks
            .start_task_run(&task.id, "task-run-1", "exec-1", 100)
            .expect("task run");
        storage
            .tasks
            .mark_task_run_terminal(
                "task-run-1",
                TaskRunStatus::Completed,
                200,
                None,
                Default::default(),
            )
            .expect("terminal task run");

        let runs = service
            .list_runs(&RunListQuery {
                container: ExecutionContainerRef {
                    kind: ExecutionContainerKind::Task,
                    id: task.id.clone(),
                },
            })
            .expect("task runs");

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].run_id.as_deref(), Some("task-run-1"));
        assert_eq!(runs[0].status, "completed");
        assert_eq!(
            runs[0].session_id.as_deref(),
            Some(task.chat_session_id.as_str())
        );
        assert_eq!(runs[0].task_id.as_deref(), Some(task.id.as_str()));
    }

    #[test]
    fn opens_task_run_thread_from_task_run_record_when_trace_index_is_empty() {
        let (storage, _temp_dir) = create_storage();
        let service = ExecutionConsoleService::from_storage(&storage);

        let task = storage
            .tasks
            .create_task_from_spec(TaskSpec {
                name: "Digest".to_string(),
                description: None,
                agent_id: "agent-1".to_string(),
                chat_session_id: None,
                input: Some("digest".to_string()),
                input_template: None,
                schedule: TaskSchedule::default(),
                execution_mode: Some(ExecutionMode::default()),
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("task");
        let mut session = ChatSession::new(task.agent_id.clone(), "gpt-5".to_string());
        session.id = task.chat_session_id.clone();
        session.record_turn_user_message("turn-1", "digest");
        session.complete_turn_with_assistant_message("turn-1", "done");
        storage.chat_sessions.create(&session).expect("session");
        storage
            .tasks
            .start_task_run(&task.id, "task-run-1", "exec-1", 100)
            .expect("task run");
        storage
            .tasks
            .mark_task_run_terminal(
                "task-run-1",
                TaskRunStatus::Completed,
                200,
                None,
                Default::default(),
            )
            .expect("terminal task run");

        let thread = service
            .get_execution_run_thread("task-run-1")
            .expect("task run thread");

        assert_eq!(thread.focus.run_id.as_deref(), Some("task-run-1"));
        assert_eq!(thread.focus.status, "completed");
        assert_eq!(thread.focus.task_id.as_deref(), Some(task.id.as_str()));
        assert_eq!(thread.timeline.stats.message_count, 2);
        assert!(
            thread
                .timeline
                .events
                .iter()
                .all(|event| event.run_id.as_deref() == Some("task-run-1"))
        );

        let timeline = service
            .get_execution_run_timeline("task-run-1")
            .expect("task run timeline");
        assert_eq!(timeline.stats.message_count, 2);
        assert!(
            timeline
                .events
                .iter()
                .all(|event| event.run_id.as_deref() == Some("task-run-1"))
        );
    }

    #[test]
    fn task_run_timeline_projects_legacy_session_messages_when_turns_are_empty() {
        let (storage, _temp_dir) = create_storage();
        let service = ExecutionConsoleService::from_storage(&storage);

        let task = storage
            .tasks
            .create_task_from_spec(TaskSpec {
                name: "Digest".to_string(),
                description: None,
                agent_id: "agent-1".to_string(),
                chat_session_id: None,
                input: Some("digest".to_string()),
                input_template: None,
                schedule: TaskSchedule::default(),
                execution_mode: Some(ExecutionMode::default()),
                timeout_secs: None,
                resource_limits: None,
                prerequisites: Vec::new(),
                continuation: None,
            })
            .expect("task");
        let mut session = ChatSession::new(task.agent_id.clone(), "gpt-5".to_string());
        session.id = task.chat_session_id.clone();
        session.add_message(ChatMessage::user("digest"));
        session.add_message(ChatMessage::assistant("done"));
        storage.chat_sessions.create(&session).expect("session");
        storage
            .tasks
            .start_task_run(&task.id, "task-run-1", "exec-1", 100)
            .expect("task run");
        storage
            .tasks
            .mark_task_run_terminal(
                "task-run-1",
                TaskRunStatus::Completed,
                200,
                None,
                Default::default(),
            )
            .expect("terminal task run");

        let timeline = service
            .get_execution_run_timeline("task-run-1")
            .expect("task run timeline");

        assert_eq!(timeline.stats.message_count, 2);
        assert!(timeline.events.iter().any(|event| {
            event
                .message
                .as_ref()
                .and_then(|message| message.content_preview.as_deref())
                == Some("done")
        }));
    }

    #[test]
    fn child_runs_include_terminal_events_without_parent_run_id() {
        let (storage, _temp_dir) = create_storage();
        let service = ExecutionConsoleService::from_storage(&storage);

        let task = storage
            .tasks
            .create_task_from_spec(TaskSpec {
                name: "Digest".to_string(),
                description: None,
                agent_id: "agent-1".to_string(),
                chat_session_id: None,
                input: Some("digest".to_string()),
                input_template: None,
                schedule: TaskSchedule::default(),
                execution_mode: Some(ExecutionMode::default()),
                timeout_secs: None,
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

        let child_trace = ai::telemetry::RestflowTrace::new(
            "run-child",
            task.chat_session_id.clone(),
            task.id.clone(),
            "agent-1",
        )
        .with_parent_run_id(Some("run-parent".to_string()));
        let child_start = execution_trace_builders::with_trace_context(
            execution_trace_builders::lifecycle(
                &task.id,
                "agent-1",
                LifecycleTrace {
                    status: "running".to_string(),
                    message: Some("started".to_string()),
                    error: None,
                    ai_duration_ms: None,
                },
            ),
            &child_trace,
        );
        let child_terminal_trace = ai::telemetry::RestflowTrace::new(
            "run-child",
            task.chat_session_id.clone(),
            task.id.clone(),
            "agent-1",
        );
        let child_end = execution_trace_builders::with_trace_context(
            execution_trace_builders::lifecycle(
                &task.id,
                "agent-1",
                LifecycleTrace {
                    status: "completed".to_string(),
                    message: Some("done".to_string()),
                    error: None,
                    ai_duration_ms: Some(1200),
                },
            ),
            &child_terminal_trace,
        );
        storage
            .execution_traces
            .store(&child_start)
            .expect("child start");
        storage
            .execution_traces
            .store(&child_end)
            .expect("child end");

        let child_runs = service.list_child_runs("run-parent").expect("child runs");
        assert_eq!(child_runs.len(), 1);
        assert_eq!(child_runs[0].run_id.as_deref(), Some("run-child"));
        assert_eq!(child_runs[0].status, "completed");
        assert!(child_runs[0].ended_at.is_some());
    }

    #[test]
    fn workspace_container_exposes_latest_run_and_child_runs_via_relation_query() {
        let (storage, _temp_dir) = create_storage();
        let service = ExecutionConsoleService::from_storage(&storage);

        let session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        let session_id = session.id.clone();
        storage.chat_sessions.create(&session).expect("session");

        store_run_events(&storage, "task-1", &session_id, "run-1", None);
        store_run_events(&storage, "task-1", &session_id, "run-2", Some("run-1"));

        let containers = service.list_execution_containers().expect("containers");
        let workspace_container = containers
            .iter()
            .find(|container| container.id == session_id)
            .expect("workspace container");
        assert_eq!(workspace_container.latest_run_id.as_deref(), Some("run-1"));

        let thread = service.get_execution_run_thread("run-1").expect("thread");
        assert_eq!(thread.focus.run_id.as_deref(), Some("run-1"));
        assert_eq!(thread.focus.root_run_id.as_deref(), Some("run-1"));
        assert!(thread.timeline.events.len() >= 2);

        let child_runs = service.list_child_runs("run-1").expect("child runs");
        assert_eq!(child_runs.len(), 1);
        assert_eq!(child_runs[0].run_id.as_deref(), Some("run-2"));
        assert_eq!(child_runs[0].container_id, session_id);
        assert_eq!(child_runs[0].root_run_id.as_deref(), Some("run-1"));

        let child_thread = service
            .get_execution_run_thread("run-2")
            .expect("child thread");
        assert_eq!(child_thread.focus.run_id.as_deref(), Some("run-2"));
        assert_eq!(child_thread.focus.parent_run_id.as_deref(), Some("run-1"));
        assert_eq!(child_thread.focus.container_id, session_id);
        assert_eq!(child_thread.focus.root_run_id.as_deref(), Some("run-1"));
    }
}
