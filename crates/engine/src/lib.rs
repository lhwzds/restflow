//! # codocia
//!
//! Engine owns core composition and command execution boundary.
//!
//! ## Owns
//! - core API entrypoint
//! - in-memory core composition
//! - shared store bundle wiring
//! - model, profile, and skill catalog helpers
//!
//! ## Must Not
//! - own product transport details
//! - own persistence
//! - duplicate module logic
//!
//! ## Inputs
//! - core modules
//! - user chat turns
//! - task run requests
//! - tool calls
//! - model/profile/skill updates
//! - core snapshots
//!
//! ## Outputs
//! - composed core outputs
//! - command responses from proto
//! - core snapshots
//!
//! ## Depends On
//! - agent
//! - auth
//! - chat
//! - event
//! - model
//! - run
//! - skill
//! - store
//! - tool
//!
//! ## Verify
//! - cargo check -p engine

use anyhow::Result;
use store::{Repository, SharedStore};

mod command;
mod session;
mod snapshot;
mod task;
mod tooling;

#[derive(Clone)]
pub struct CoreStores {
    pub skills: SharedStore<skill::Skill>,
    pub sessions: SharedStore<chat::Session>,
    pub tasks: SharedStore<run::Task>,
    pub runs: SharedStore<run::Run>,
    pub profiles: SharedStore<auth::Profile>,
}

impl CoreStores {
    pub fn memory() -> Self {
        Self {
            skills: store::memory_store(),
            sessions: store::memory_store(),
            tasks: store::memory_store(),
            runs: store::memory_store(),
            profiles: store::memory_store(),
        }
    }
}

#[derive(Clone)]
pub struct Core {
    agent: agent::Agent,
    tools: tool::Registry,
    models: model::ModelCatalog,
    skills: SharedStore<skill::Skill>,
    sessions: SharedStore<chat::Session>,
    tasks: SharedStore<run::Task>,
    runs: SharedStore<run::Run>,
    profiles: SharedStore<auth::Profile>,
}

impl Core {
    pub fn new(model: model::Model) -> Self {
        Self::with_stores(model, CoreStores::memory())
    }

    pub fn with_stores(model: model::Model, stores: CoreStores) -> Self {
        Self {
            agent: agent::Agent::new(model),
            tools: tool::Registry::new(),
            models: model::ModelCatalog::new(),
            skills: stores.skills,
            sessions: stores.sessions,
            tasks: stores.tasks,
            runs: stores.runs,
            profiles: stores.profiles,
        }
    }

    pub fn set_model(&mut self, model: model::Model) {
        self.agent.model = model;
    }

    pub fn current_model(&self) -> &model::Model {
        &self.agent.model
    }

    pub fn insert_model(&mut self, spec: model::ModelSpec) {
        self.models.insert(spec);
    }

    pub fn clear_models(&mut self) {
        self.models.clear();
    }

    pub fn model_exists(&self, provider: &str, model: &str) -> bool {
        self.models.get(provider, model).is_some()
    }

    pub fn register_tool<T>(&mut self, tool: T)
    where
        T: tool::Tool + 'static,
    {
        self.tools.insert(tool);
    }

    pub async fn save_profile(&self, profile: auth::Profile) -> Result<()> {
        auth::save_profile(&self.profiles, profile).await
    }

    pub async fn profile_exists(&self, provider: &str) -> Result<bool> {
        self.profiles.exists(provider).await
    }

    pub async fn replace_profiles(&self, profiles: Vec<auth::Profile>) -> Result<()> {
        self.profiles
            .replace_all(
                profiles
                    .into_iter()
                    .map(|profile| (profile.provider.id.clone(), profile))
                    .collect(),
            )
            .await
    }

    pub async fn save_skill(&self, skill: skill::Skill) -> Result<()> {
        let skill_id = skill.id.clone();
        self.skills.put(&skill_id, skill).await
    }

    pub async fn skill_exists(&self, id: &str) -> Result<bool> {
        self.skills.exists(id).await
    }

    pub async fn replace_skills(&self, skills: Vec<skill::Skill>) -> Result<()> {
        self.skills
            .replace_all(
                skills
                    .into_iter()
                    .map(|skill| (skill.id.clone(), skill))
                    .collect(),
            )
            .await
    }

    pub async fn skill_catalog(&self) -> Result<skill::Catalog> {
        skill::Catalog::from_repository(&self.skills).await
    }

    pub async fn save_session(&self, session: chat::Session) -> Result<()> {
        chat::save_session(&self.sessions, session).await
    }

    pub async fn session(&self, id: &str) -> Result<Option<chat::Session>> {
        self.sessions.get(id).await
    }

    pub async fn session_exists(&self, id: &str) -> Result<bool> {
        self.sessions.exists(id).await
    }

    pub async fn replace_sessions(&self, sessions: Vec<chat::Session>) -> Result<()> {
        self.sessions
            .replace_all(
                sessions
                    .into_iter()
                    .map(|session| (session.id.clone(), session))
                    .collect(),
            )
            .await
    }

    pub async fn save_task(&self, task: run::Task) -> Result<()> {
        run::save_task(&self.tasks, task).await
    }

    pub async fn task_exists(&self, id: &str) -> Result<bool> {
        self.tasks.exists(id).await
    }

    pub async fn replace_tasks(&self, tasks: Vec<run::Task>) -> Result<()> {
        self.tasks
            .replace_all(
                tasks
                    .into_iter()
                    .map(|task| (task.id.clone(), task))
                    .collect(),
            )
            .await
    }

    pub async fn save_run(&self, run: run::Run) -> Result<()> {
        run::save_run(&self.runs, run).await
    }

    pub async fn run(&self, id: &str) -> Result<Option<run::Run>> {
        self.runs.get(id).await
    }

    pub async fn run_exists(&self, id: &str) -> Result<bool> {
        self.runs.exists(id).await
    }

    pub async fn replace_runs(&self, runs: Vec<run::Run>) -> Result<()> {
        self.runs
            .replace_all(runs.into_iter().map(|run| (run.id.clone(), run)).collect())
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proto::{CoreCommand, CoreResponse, CoreSnapshot};
    use std::future::Future;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    #[test]
    fn core_chat_turn_resolves_skill_and_persists_messages() {
        block_on_once(async {
            let core = Core::new(model::Model::new("openai", "gpt-5.5"));
            core.save_skill(
                skill::Skill::new("team", "Team", skill::Source::System)
                    .with_content("Use parallel workers for independent work."),
            )
            .await
            .unwrap();

            let output = core
                .chat_turn("session-1", chat::TurnRequest::new("use @team"))
                .await
                .unwrap();

            assert_eq!(output.events.len(), 1);
            let session = core.session("session-1").await.unwrap().unwrap();
            assert_eq!(session.messages.len(), 2);
            assert_eq!(session.messages[0].role, chat::Role::User);
            assert!(session.messages[1].text.contains("Mentioned skill: @team"));
        });
    }

    #[test]
    fn core_run_task_marks_run_done() {
        block_on_once(async {
            let core = Core::new(model::Model::new("openai", "gpt-5.5"));
            let task = run::Task::new("task-1", "Review branch");
            core.start_run(task.clone(), "run-1", "session-1")
                .await
                .unwrap();

            core.run_task("run-1", run::TaskRequest::new(task, "summarize"))
                .await
                .unwrap();

            let run = core.run("run-1").await.unwrap().unwrap();
            assert_eq!(run.status, run::Status::Done);
        });
    }

    #[test]
    fn core_can_use_injected_stores() {
        block_on_once(async {
            let stores = CoreStores::memory();
            let skill_store = stores.skills.clone();
            let session_store = stores.sessions.clone();
            let core = Core::with_stores(model::Model::new("openai", "gpt-5.5"), stores);

            core.save_skill(skill::Skill::new("team", "Team", skill::Source::System))
                .await
                .unwrap();
            core.chat_turn("session-1", chat::TurnRequest::new("hello"))
                .await
                .unwrap();

            assert!(skill_store.exists("team").await.unwrap());
            assert!(session_store.exists("session-1").await.unwrap());
        });
    }

    #[test]
    fn core_can_use_redb_stores() {
        block_on_once(async {
            let path = temp_db_path("core-redb-stores");
            let core = Core::with_stores(
                model::Model::new("openai", "gpt-5.5"),
                redb_core_stores(&path),
            );

            core.save_skill(skill::Skill::new("team", "Team", skill::Source::System))
                .await
                .unwrap();
            core.save_profile(auth::Profile::new("openai", "OPENAI_API_KEY"))
                .await
                .unwrap();
            core.chat_turn("session-1", chat::TurnRequest::new("hello"))
                .await
                .unwrap();
            core.start_run(
                run::Task::new("task-1", "Review branch"),
                "run-1",
                "session-1",
            )
            .await
            .unwrap();
            drop(core);

            let restored = Core::with_stores(
                model::Model::new("openai", "gpt-5.5"),
                redb_core_stores(&path),
            );
            let snapshot = restored.snapshot().await.unwrap();

            assert_eq!(snapshot.skills.len(), 1);
            assert_eq!(snapshot.sessions.len(), 1);
            assert_eq!(snapshot.tasks.len(), 1);
            assert_eq!(snapshot.runs.len(), 1);
            assert_eq!(snapshot.profiles.len(), 1);

            let _ = std::fs::remove_file(path);
        });
    }

    #[test]
    fn core_tool_calls_emit_call_and_result_events() {
        block_on_once(async {
            let mut core = Core::new(model::Model::new("openai", "gpt-5.5"));
            core.register_tool(EchoTool);

            let events = core
                .call_tool_events(tool::ToolCall::new(
                    "call-1",
                    "echo",
                    serde_json::json!({ "message": "hello" }),
                ))
                .await;

            assert_eq!(events.len(), 2);
            assert_eq!(
                events[0],
                event::Event::tool_call(
                    "call-1",
                    "echo",
                    serde_json::json!({ "message": "hello" })
                )
            );
            assert_eq!(
                events[1],
                event::Event::tool_result("call-1", serde_json::json!({ "message": "hello" }))
            );
        });
    }

    #[test]
    fn core_missing_tool_emits_error_event() {
        block_on_once(async {
            let core = Core::new(model::Model::new("openai", "gpt-5.5"));

            let events = core
                .call_tool_events(tool::ToolCall::new(
                    "call-1",
                    "missing",
                    serde_json::json!({}),
                ))
                .await;

            assert_eq!(events.len(), 2);
            assert!(
                matches!(&events[1], event::Event::Error { message } if message.contains("tool not found: missing"))
            );
            assert!(events[1].is_terminal());
        });
    }

    #[test]
    fn core_handle_routes_chat_commands() {
        block_on_once(async {
            let mut core = Core::new(model::Model::new("openai", "gpt-5.5"));
            core.handle(CoreCommand::SaveSkill {
                skill: skill::Skill::new("team", "Team", skill::Source::System)
                    .with_content("Use workers."),
            })
            .await
            .unwrap();

            let response = core
                .handle(CoreCommand::ChatTurn {
                    session_id: "session-1".to_string(),
                    message: "use @team".to_string(),
                    assigned_skills: Vec::new(),
                })
                .await
                .unwrap();

            match response {
                CoreResponse::ChatTurn { events } => {
                    assert_eq!(events.len(), 1);
                    assert!(matches!(events[0], event::Event::Text { .. }));
                }
                other => panic!("unexpected response: {other:?}"),
            }
        });
    }

    #[test]
    fn core_handle_routes_model_and_run_commands() {
        block_on_once(async {
            let mut core = Core::new(model::Model::new("openai", "gpt-5.4"));

            let response = core
                .handle(CoreCommand::SwitchModel {
                    model: model::Model::new("openai", "gpt-5.5"),
                })
                .await
                .unwrap();

            assert!(matches!(response, CoreResponse::ModelSwitched { .. }));
            assert_eq!(core.current_model().id, "gpt-5.5");

            let task = run::Task::new("task-1", "Review branch");
            let response = core
                .handle(CoreCommand::StartRun {
                    task: task.clone(),
                    run_id: "run-1".to_string(),
                    session_id: "session-1".to_string(),
                })
                .await
                .unwrap();

            assert!(matches!(response, CoreResponse::RunStarted { .. }));

            let response = core
                .handle(CoreCommand::RunTask {
                    run_id: "run-1".to_string(),
                    task,
                    message: "summarize".to_string(),
                    assigned_skills: Vec::new(),
                })
                .await
                .unwrap();

            assert!(matches!(response, CoreResponse::RunTask { .. }));
            assert_eq!(
                core.run("run-1").await.unwrap().unwrap().status,
                run::Status::Done
            );
        });
    }

    #[test]
    fn core_command_round_trips_through_json() {
        let command = CoreCommand::ChatTurn {
            session_id: "session-1".to_string(),
            message: "hello".to_string(),
            assigned_skills: vec!["team".to_string()],
        };

        let encoded = serde_json::to_string(&command).unwrap();
        let decoded: CoreCommand = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, command);
    }

    #[test]
    fn core_snapshot_exports_and_imports_state() {
        block_on_once(async {
            let mut core = Core::new(model::Model::new("openai", "gpt-5.5"));
            core.insert_model(model::ModelSpec::new("openai", "gpt-5.5", "GPT-5.5"));
            core.register_tool(EchoTool);
            core.save_skill(skill::Skill::new("team", "Team", skill::Source::System))
                .await
                .unwrap();
            core.save_profile(auth::Profile::new("openai", "OPENAI_API_KEY"))
                .await
                .unwrap();
            core.chat_turn("session-1", chat::TurnRequest::new("hello"))
                .await
                .unwrap();
            core.start_run(
                run::Task::new("task-1", "Review branch"),
                "run-1",
                "session-1",
            )
            .await
            .unwrap();

            let snapshot = core.snapshot().await.unwrap();
            let restored = Core::from_snapshot(snapshot.clone()).await.unwrap();
            let restored_snapshot = restored.snapshot().await.unwrap();

            assert_eq!(snapshot.current_model, restored_snapshot.current_model);
            assert_eq!(snapshot.models, restored_snapshot.models);
            assert_eq!(snapshot.skills, restored_snapshot.skills);
            assert_eq!(snapshot.sessions, restored_snapshot.sessions);
            assert_eq!(snapshot.tasks, restored_snapshot.tasks);
            assert_eq!(snapshot.runs, restored_snapshot.runs);
            assert_eq!(snapshot.profiles, restored_snapshot.profiles);
            assert!(snapshot.tool_specs.iter().any(|spec| spec.name == "echo"));
            assert!(restored_snapshot.tool_specs.is_empty());
        });
    }

    #[test]
    fn core_snapshot_round_trips_through_json() {
        block_on_once(async {
            let core = Core::new(model::Model::new("openai", "gpt-5.5"));
            let snapshot = core.snapshot().await.unwrap();

            let encoded = serde_json::to_string(&snapshot).unwrap();
            let decoded: CoreSnapshot = serde_json::from_str(&encoded).unwrap();

            assert_eq!(decoded, snapshot);
        });
    }

    struct EchoTool;

    #[async_trait::async_trait]
    impl tool::Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }

        async fn call(&self, input: serde_json::Value) -> Result<serde_json::Value> {
            Ok(input)
        }
    }

    fn redb_core_stores(path: &std::path::Path) -> CoreStores {
        let database = store::open_redb_database(path).unwrap();
        CoreStores {
            skills: store::redb_store(database.clone(), "skills").unwrap(),
            sessions: store::redb_store(database.clone(), "sessions").unwrap(),
            tasks: store::redb_store(database.clone(), "tasks").unwrap(),
            runs: store::redb_store(database.clone(), "runs").unwrap(),
            profiles: store::redb_store(database, "profiles").unwrap(),
        }
    }

    fn temp_db_path(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{name}-{nanos}.redb"))
    }

    fn block_on_once<T>(future: impl Future<Output = T>) -> T {
        let waker = Waker::from(Arc::new(NoopWake));
        let mut context = Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);

        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("core future unexpectedly yielded"),
        }
    }

    struct NoopWake;

    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }
}
