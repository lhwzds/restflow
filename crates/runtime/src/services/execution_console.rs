use std::sync::Arc;

use anyhow::Result;
use thiserror::Error;

use crate::models::{
    ChatSession, ChatSessionSource, ChatTurn, ChatTurnEventKind, ChatTurnStatus,
    ExecutionContainerKind, ExecutionContainerSummary, ExecutionThread, RunKind, RunListQuery,
    RunSummary, RunTimeline, Task, TaskRun,
};
use crate::storage::Storage;

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

impl ExecutionConsoleService {
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }

    pub fn from_storage(storage: &Arc<Storage>) -> Self {
        Self::new(storage.clone())
    }

    pub fn list_execution_containers(&self) -> Result<Vec<ExecutionContainerSummary>> {
        let sessions = self.storage.chat_sessions.list_all()?;
        let tasks = self.storage.tasks.list_tasks()?;
        let mut containers = Vec::new();

        for session in sessions
            .iter()
            .filter(|session| session.source_channel == Some(ChatSessionSource::Workspace))
        {
            containers.push(ExecutionContainerSummary {
                id: session.id.clone(),
                kind: ExecutionContainerKind::Workspace,
                title: session.name.clone(),
                subtitle: Some(session.model.clone()).filter(|value| !value.is_empty()),
                updated_at: session.updated_at,
                status: latest_session_status(session),
                session_count: 1,
                latest_session_id: Some(session.id.clone()),
                latest_run_id: latest_turn(session).map(|turn| turn.id.clone()),
                agent_id: Some(session.agent_id.clone()),
                source_channel: session.source_channel,
                source_conversation_id: session.source_conversation_id.clone(),
            });
        }

        for task in tasks {
            let runs = self
                .storage
                .tasks
                .list_task_runs(&task.id)
                .unwrap_or_default();
            containers.push(ExecutionContainerSummary {
                id: task.id.clone(),
                kind: ExecutionContainerKind::Task,
                title: task.name.clone(),
                subtitle: task.description.clone(),
                updated_at: task.updated_at,
                status: Some(task.status.as_str().to_string()),
                session_count: runs.len() as u32,
                latest_session_id: Some(task.chat_session_id.clone()),
                latest_run_id: runs
                    .iter()
                    .max_by_key(|run| run.updated_at)
                    .map(|run| run.run_id.clone()),
                agent_id: Some(task.agent_id.clone()),
                source_channel: None,
                source_conversation_id: None,
            });
        }

        containers.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        Ok(containers)
    }

    pub fn list_runs(&self, query: &RunListQuery) -> Result<Vec<RunSummary>> {
        match query.container.kind {
            ExecutionContainerKind::Workspace => {
                let sessions = self.storage.chat_sessions.list_all()?;
                let mut runs = Vec::new();
                for session in sessions.into_iter().filter(|session| {
                    session.id == query.container.id
                        || (query.container.id == "workspace"
                            && session.source_channel == Some(ChatSessionSource::Workspace))
                }) {
                    runs.extend(
                        session
                            .turns
                            .iter()
                            .map(|turn| workspace_run_summary(&session, turn)),
                    );
                }
                runs.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
                Ok(runs)
            }
            ExecutionContainerKind::Task => {
                let Some(task) = self.storage.tasks.get_task(&query.container.id)? else {
                    return Ok(Vec::new());
                };
                let mut runs = self
                    .storage
                    .tasks
                    .list_task_runs(&task.id)?
                    .into_iter()
                    .map(|run| task_run_summary(&task, &run))
                    .collect::<Vec<_>>();
                runs.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
                Ok(runs)
            }
        }
    }

    pub fn get_execution_run_thread(
        &self,
        run_id: &str,
    ) -> std::result::Result<ExecutionThread, ExecutionThreadError> {
        let summary = self.find_run(run_id)?;
        let timeline = self.timeline_for_run(run_id)?;
        Ok(ExecutionThread {
            focus: summary,
            timeline,
        })
    }

    pub fn list_child_runs(&self, parent_run_id: &str) -> Result<Vec<RunSummary>> {
        let _ = parent_run_id;
        Ok(Vec::new())
    }

    pub fn get_execution_run_timeline(&self, run_id: &str) -> Result<RunTimeline> {
        let _ = self.find_run(run_id)?;
        self.timeline_for_run(run_id).map_err(Into::into)
    }

    fn find_run(&self, run_id: &str) -> std::result::Result<RunSummary, ExecutionThreadError> {
        let run_id = run_id.trim();
        if run_id.is_empty() {
            return Err(ExecutionThreadError::InvalidQuery);
        }

        for session in self.storage.chat_sessions.list_all()? {
            if let Some(turn) = session.turns.iter().find(|turn| turn.id == run_id) {
                return Ok(workspace_run_summary(&session, turn));
            }
        }

        for task in self.storage.tasks.list_tasks()? {
            if let Some(run) = self
                .storage
                .tasks
                .list_task_runs(&task.id)?
                .into_iter()
                .find(|run| run.run_id == run_id)
            {
                return Ok(task_run_summary(&task, &run));
            }
        }

        Err(ExecutionThreadError::RunNotFound(run_id.to_string()))
    }

    fn timeline_for_run(
        &self,
        run_id: &str,
    ) -> std::result::Result<RunTimeline, ExecutionThreadError> {
        let run_id = run_id.trim();
        if run_id.is_empty() {
            return Err(ExecutionThreadError::InvalidQuery);
        }

        for session in self.storage.chat_sessions.list_all()? {
            if let Some(turn) = session.turns.iter().find(|turn| turn.id == run_id) {
                return Ok(RunTimeline {
                    events: turn.events.clone(),
                });
            }
        }

        for task in self.storage.tasks.list_tasks()? {
            if self
                .storage
                .tasks
                .list_task_runs(&task.id)?
                .into_iter()
                .any(|run| run.run_id == run_id)
            {
                let Some(session) = self.storage.chat_sessions.get(&task.chat_session_id)? else {
                    return Ok(RunTimeline::default());
                };
                let events = session
                    .turns
                    .iter()
                    .find(|turn| turn.id == run_id)
                    .map(|turn| turn.events.clone())
                    .unwrap_or_default();
                return Ok(RunTimeline { events });
            }
        }

        Err(ExecutionThreadError::RunNotFound(run_id.to_string()))
    }
}

fn latest_turn(session: &ChatSession) -> Option<&ChatTurn> {
    session.turns.iter().max_by_key(|turn| turn.updated_at)
}

fn latest_session_status(session: &ChatSession) -> Option<String> {
    latest_turn(session).map(|turn| turn_status(turn.status).to_string())
}

fn workspace_run_summary(session: &ChatSession, turn: &ChatTurn) -> RunSummary {
    RunSummary {
        id: turn.id.clone(),
        kind: RunKind::WorkspaceRun,
        container_id: session.id.clone(),
        root_run_id: Some(turn.id.clone()),
        title: turn_title(turn).unwrap_or_else(|| session.name.clone()),
        subtitle: Some(session.model.clone()).filter(|value| !value.is_empty()),
        status: turn_status(turn.status).to_string(),
        updated_at: turn.updated_at,
        started_at: Some(turn.started_at),
        ended_at: turn.completed_at,
        session_id: Some(session.id.clone()),
        run_id: Some(turn.id.clone()),
        task_id: None,
        parent_run_id: None,
        agent_id: Some(session.agent_id.clone()),
        source_channel: session.source_channel,
        source_conversation_id: session.source_conversation_id.clone(),
        effective_model: Some(session.model.clone()).filter(|value| !value.is_empty()),
        provider: Some(session.provider.clone()).filter(|value| !value.is_empty()),
        event_count: turn.events.len() as u64,
    }
}

fn task_run_summary(task: &Task, run: &TaskRun) -> RunSummary {
    RunSummary {
        id: run.run_id.clone(),
        kind: RunKind::TaskRun,
        container_id: task.id.clone(),
        root_run_id: Some(run.run_id.clone()),
        title: task.name.clone(),
        subtitle: run.error.clone().or_else(|| task.description.clone()),
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
        effective_model: run
            .metrics
            .final_model
            .clone()
            .or(run.metrics.active_model.clone()),
        provider: None,
        event_count: run.metrics.message_count.unwrap_or_default() as u64,
    }
}

fn turn_status(status: ChatTurnStatus) -> &'static str {
    match status {
        ChatTurnStatus::Running => "running",
        ChatTurnStatus::Completed => "completed",
        ChatTurnStatus::Canceled => "interrupted",
        ChatTurnStatus::Failed => "failed",
    }
}

fn turn_title(turn: &ChatTurn) -> Option<String> {
    turn.events.iter().find_map(|event| match &event.kind {
        ChatTurnEventKind::UserMessage { content } => Some(trim_title(content)),
        ChatTurnEventKind::AssistantMessage { content } => Some(trim_title(content)),
        ChatTurnEventKind::ToolCall { name, .. } => Some(format!("Tool: {name}")),
        ChatTurnEventKind::ToolResult { call_id, .. } => Some(format!("Tool result: {call_id}")),
        ChatTurnEventKind::Error { message } => Some(trim_title(message)),
        ChatTurnEventKind::Canceled => Some("Canceled turn".to_string()),
    })
}

fn trim_title(value: &str) -> String {
    let value = value.trim();
    if value.chars().count() > 80 {
        format!("{}...", value.chars().take(77).collect::<String>())
    } else if value.is_empty() {
        "Untitled run".to_string()
    } else {
        value.to_string()
    }
}
