//! # codocia
//!
//! Run owns durable task and run execution concepts.
//!
//! ## Owns
//! - Task
//! - Run
//! - run status
//! - durable execution vocabulary
//! - migration-safe task and run metadata
//! - task and run repository helpers
//!
//! ## Must Not
//! - become a second agent loop
//! - own skill catalog
//! - create a separate team runtime
//!
//! ## Inputs
//! - task definitions
//! - agent execution events
//! - checkpoint state
//! - skill catalog
//! - task and run repositories
//!
//! ## Outputs
//! - run status
//! - run history
//! - run artifacts
//! - agent run input
//! - persisted task and run records
//!
//! ## Depends On
//! - agent
//! - chat
//! - event
//! - skill
//! - store
//!
//! ## Verify
//! - cargo check -p run

use agent::RunInput;
use anyhow::Result;
use chat::Session;
use serde::{Deserialize, Serialize};
use skill::{Catalog, resolve_context};
use store::Repository;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub input: Option<String>,
    pub agent_id: Option<String>,
    pub session_id: Option<String>,
    pub status: Option<String>,
    pub schedule: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub id: String,
    pub task_id: String,
    pub status: Status,
    pub raw_status: Option<String>,
    pub session_id: Option<String>,
    pub execution_id: Option<String>,
    pub checkpoint_id: Option<String>,
    pub error: Option<String>,
    pub started_at: Option<String>,
    pub updated_at: Option<String>,
    pub ended_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pending,
    Running,
    Done,
    Failed,
    Canceled,
}

impl Task {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            input: None,
            agent_id: None,
            session_id: None,
            status: None,
            schedule: None,
            created_at: None,
            updated_at: None,
            error: None,
        }
    }
}

impl Run {
    pub fn new(id: impl Into<String>, task_id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            task_id: task_id.into(),
            status: Status::Pending,
            raw_status: None,
            session_id: None,
            execution_id: None,
            checkpoint_id: None,
            error: None,
            started_at: None,
            updated_at: None,
            ended_at: None,
        }
    }

    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    pub fn with_status(mut self, status: Status) -> Self {
        self.status = status;
        self
    }
}

pub async fn load_task<R>(repository: &R, id: &str) -> Result<Option<Task>>
where
    R: Repository<Task> + ?Sized,
{
    repository.get(id).await
}

pub async fn save_task<R>(repository: &R, task: Task) -> Result<()>
where
    R: Repository<Task> + ?Sized,
{
    repository.put(&task.id.clone(), task).await
}

pub async fn load_run<R>(repository: &R, id: &str) -> Result<Option<Run>>
where
    R: Repository<Run> + ?Sized,
{
    repository.get(id).await
}

pub async fn save_run<R>(repository: &R, run: Run) -> Result<()>
where
    R: Repository<Run> + ?Sized,
{
    repository.put(&run.id.clone(), run).await
}

pub async fn list_task_runs<R>(repository: &R, task_id: &str) -> Result<Vec<Run>>
where
    R: Repository<Run> + ?Sized,
{
    Ok(repository
        .list()
        .await?
        .into_iter()
        .filter(|run| run.task_id == task_id)
        .collect())
}

pub async fn update_run_status<R>(repository: &R, run_id: &str, status: Status) -> Result<Run>
where
    R: Repository<Run> + ?Sized,
{
    let mut run = repository
        .get(run_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("run not found: {run_id}"))?;
    run.status = status;
    repository.put(run_id, run.clone()).await?;
    Ok(run)
}

pub async fn load_run_session<R>(repository: &R, run: &Run) -> Result<Option<Session>>
where
    R: Repository<Session> + ?Sized,
{
    let Some(session_id) = &run.session_id else {
        return Ok(None);
    };
    repository.get(session_id).await
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRequest {
    pub task: Task,
    pub message: String,
    pub assigned_skills: Vec<String>,
}

impl TaskRequest {
    pub fn new(task: Task, message: impl Into<String>) -> Self {
        Self {
            task,
            message: message.into(),
            assigned_skills: Vec::new(),
        }
    }

    pub fn with_assigned_skills(mut self, skills: impl IntoIterator<Item = String>) -> Self {
        self.assigned_skills = skills.into_iter().collect();
        self
    }

    pub fn to_agent_input(&self, catalog: &Catalog) -> RunInput {
        let message = format!("Task: {}\n\n{}", self.task.title, self.message);
        RunInput::new(message).with_skill_context(resolve_context(
            catalog,
            &self.assigned_skills,
            &self.message,
        ))
    }
}

pub fn build_agent_input(catalog: &Catalog, request: &TaskRequest) -> RunInput {
    request.to_agent_input(catalog)
}

#[cfg(test)]
mod tests {
    use super::*;
    use skill::{Skill, Source};
    use std::future::Future;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};
    use store::MemoryStore;

    #[test]
    fn task_request_resolves_skill_context_for_agent() {
        let mut catalog = Catalog::new();
        catalog.insert(
            Skill::new("review", "Review", Source::System)
                .with_description("Review code.")
                .with_content("Report findings first."),
        );
        let task = Task::new("task-1", "Review branch");
        let request =
            TaskRequest::new(task, "use @review").with_assigned_skills(["review".to_string()]);

        let input = build_agent_input(&catalog, &request);

        assert!(input.message.starts_with("Task: Review branch"));
        assert_eq!(input.skill_context.assigned.len(), 1);
        assert_eq!(input.skill_context.mentioned.len(), 1);
        assert!(input.skill_context.issues.is_empty());
    }

    #[test]
    fn task_and_run_repository_helpers_persist_records() {
        block_on_once(async {
            let task_store = MemoryStore::new();
            let run_store = MemoryStore::new();
            let task = Task::new("task-1", "Review branch");
            let run = Run::new("run-1", "task-1").with_session("session-1");

            save_task(&task_store, task.clone()).await.unwrap();
            save_run(&run_store, run.clone()).await.unwrap();

            assert_eq!(load_task(&task_store, "task-1").await.unwrap(), Some(task));
            assert_eq!(load_run(&run_store, "run-1").await.unwrap(), Some(run));
        });
    }

    #[test]
    fn list_task_runs_filters_by_task_id() {
        block_on_once(async {
            let store = MemoryStore::new();
            save_run(&store, Run::new("run-1", "task-1")).await.unwrap();
            save_run(&store, Run::new("run-2", "task-2")).await.unwrap();
            save_run(&store, Run::new("run-3", "task-1")).await.unwrap();

            let runs = list_task_runs(&store, "task-1").await.unwrap();

            assert_eq!(runs.len(), 2);
            assert!(runs.iter().all(|run| run.task_id == "task-1"));
        });
    }

    #[test]
    fn update_run_status_rewrites_existing_run() {
        block_on_once(async {
            let store = MemoryStore::new();
            save_run(&store, Run::new("run-1", "task-1")).await.unwrap();

            let run = update_run_status(&store, "run-1", Status::Running)
                .await
                .unwrap();

            assert_eq!(run.status, Status::Running);
            assert_eq!(
                load_run(&store, "run-1").await.unwrap().unwrap().status,
                Status::Running
            );
        });
    }

    #[test]
    fn load_run_session_returns_bound_session() {
        block_on_once(async {
            let session_store = MemoryStore::new();
            session_store
                .put("session-1", Session::new("session-1"))
                .await
                .unwrap();
            let run = Run::new("run-1", "task-1").with_session("session-1");

            let session = load_run_session(&session_store, &run).await.unwrap();

            assert_eq!(session.unwrap().id, "session-1");
        });
    }

    fn block_on_once<T>(future: impl Future<Output = T>) -> T {
        let waker = Waker::from(Arc::new(NoopWake));
        let mut context = Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);

        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("run repository future unexpectedly yielded"),
        }
    }

    struct NoopWake;

    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }
}
