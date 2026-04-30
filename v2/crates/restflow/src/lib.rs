//! # codocia
//!
//! Facade module that exposes the V2 kernel as one public Rust API.
//!
//! ## Owns
//! - public module re-exports
//! - kernel API entrypoint
//! - in-memory kernel composition
//! - adapter-friendly command boundary
//! - migration snapshot boundary
//! - stable import shape for examples
//!
//! ## Must Not
//! - own runtime behavior
//! - own persistence
//! - duplicate module logic
//!
//! ## Inputs
//! - kernel modules
//! - user chat turns
//! - task run requests
//! - tool calls
//! - model/profile/skill updates
//! - migration snapshots
//!
//! ## Outputs
//! - unified Rust API surface
//! - composed kernel outputs
//! - command responses
//! - kernel snapshots
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
//! - cargo check -p restflow-v2

use anyhow::Result;
use serde::{Deserialize, Serialize};
use store::{MemoryStore, Repository};

pub use agent;
pub use auth;
pub use chat;
pub use event;
pub use model;
pub use run;
pub use skill;
pub use store;
pub use tool;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum KernelCommand {
    SaveSkill {
        skill: skill::Skill,
    },
    SaveProfile {
        profile: auth::Profile,
    },
    SwitchModel {
        model: model::Model,
    },
    ChatTurn {
        session_id: String,
        message: String,
        assigned_skills: Vec<String>,
    },
    StartRun {
        task: run::Task,
        run_id: String,
        session_id: String,
    },
    RunTask {
        run_id: String,
        task: run::Task,
        message: String,
        assigned_skills: Vec<String>,
    },
    CallTool {
        call: tool::ToolCall,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum KernelResponse {
    Saved,
    ModelSwitched { model: model::Model },
    ChatTurn { events: Vec<event::Event> },
    RunStarted { run: run::Run },
    RunTask { events: Vec<event::Event> },
    ToolEvents { events: Vec<event::Event> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KernelSnapshot {
    pub current_model: model::Model,
    pub models: Vec<model::ModelSpec>,
    pub skills: Vec<skill::Skill>,
    pub sessions: Vec<chat::Session>,
    pub tasks: Vec<run::Task>,
    pub runs: Vec<run::Run>,
    pub profiles: Vec<auth::Profile>,
    pub tool_specs: Vec<tool::ToolSpec>,
}

#[derive(Clone)]
pub struct Kernel {
    pub agent: agent::Agent,
    pub tools: tool::Registry,
    pub models: model::ModelCatalog,
    pub skills: MemoryStore<skill::Skill>,
    pub sessions: MemoryStore<chat::Session>,
    pub tasks: MemoryStore<run::Task>,
    pub runs: MemoryStore<run::Run>,
    pub profiles: MemoryStore<auth::Profile>,
}

impl Kernel {
    pub fn new(model: model::Model) -> Self {
        Self {
            agent: agent::Agent::new(model),
            tools: tool::Registry::new(),
            models: model::ModelCatalog::new(),
            skills: MemoryStore::new(),
            sessions: MemoryStore::new(),
            tasks: MemoryStore::new(),
            runs: MemoryStore::new(),
            profiles: MemoryStore::new(),
        }
    }

    pub fn set_model(&mut self, model: model::Model) {
        self.agent.model = model;
    }

    pub fn insert_model(&mut self, spec: model::ModelSpec) {
        self.models.insert(spec);
    }

    pub async fn from_snapshot(snapshot: KernelSnapshot) -> Result<Self> {
        let mut kernel = Self::new(snapshot.current_model);
        for spec in snapshot.models {
            kernel.insert_model(spec);
        }
        for skill in snapshot.skills {
            kernel.save_skill(skill).await?;
        }
        for session in snapshot.sessions {
            chat::save_session(&kernel.sessions, session).await?;
        }
        for task in snapshot.tasks {
            run::save_task(&kernel.tasks, task).await?;
        }
        for run in snapshot.runs {
            run::save_run(&kernel.runs, run).await?;
        }
        for profile in snapshot.profiles {
            kernel.save_profile(profile).await?;
        }
        Ok(kernel)
    }

    pub async fn snapshot(&self) -> Result<KernelSnapshot> {
        Ok(KernelSnapshot {
            current_model: self.agent.model.clone(),
            models: self.models.list().into_iter().cloned().collect(),
            skills: self.skills.list().await?,
            sessions: self.sessions.list().await?,
            tasks: self.tasks.list().await?,
            runs: self.runs.list().await?,
            profiles: self.profiles.list().await?,
            tool_specs: self.tools.specs(),
        })
    }

    pub async fn save_profile(&self, profile: auth::Profile) -> Result<()> {
        auth::save_profile(&self.profiles, profile).await
    }

    pub async fn save_skill(&self, skill: skill::Skill) -> Result<()> {
        let skill_id = skill.id.clone();
        self.skills.put(&skill_id, skill).await
    }

    pub async fn skill_catalog(&self) -> Result<skill::Catalog> {
        skill::Catalog::from_repository(&self.skills).await
    }

    pub async fn chat_turn(
        &self,
        session_id: &str,
        request: chat::TurnRequest,
    ) -> Result<agent::RunOutput> {
        let catalog = self.skill_catalog().await?;
        let input = chat::build_agent_input(&catalog, &request);
        chat::append_message(
            &self.sessions,
            session_id,
            chat::Message {
                role: chat::Role::User,
                text: request.message,
            },
        )
        .await?;

        let output = agent::Exec::new(self.agent.clone(), self.tools.clone()).dry_run(input);
        persist_assistant_text(&self.sessions, session_id, &output.events).await?;
        Ok(output)
    }

    pub async fn start_run(
        &self,
        task: run::Task,
        run_id: impl Into<String>,
        session_id: impl Into<String>,
    ) -> Result<run::Run> {
        let run = run::Run::new(run_id, task.id.clone())
            .with_session(session_id)
            .with_status(run::Status::Running);
        run::save_task(&self.tasks, task).await?;
        run::save_run(&self.runs, run.clone()).await?;
        Ok(run)
    }

    pub async fn run_task(
        &self,
        run_id: &str,
        request: run::TaskRequest,
    ) -> Result<agent::RunOutput> {
        run::save_task(&self.tasks, request.task.clone()).await?;
        let run = self
            .runs
            .get(run_id)
            .await?
            .unwrap_or_else(|| run::Run::new(run_id, request.task.id.clone()));
        let catalog = self.skill_catalog().await?;
        let input = run::build_agent_input(&catalog, &request);
        let output = agent::Exec::new(self.agent.clone(), self.tools.clone()).dry_run(input);
        run::save_run(&self.runs, run.with_status(run::Status::Done)).await?;
        Ok(output)
    }

    pub async fn call_tool_events(&self, call: tool::ToolCall) -> Vec<event::Event> {
        let mut events = vec![event::Event::tool_call(
            call.id.clone(),
            call.name.clone(),
            call.input.clone(),
        )];

        match self.tools.call(call).await {
            Ok(output) => events.push(event::Event::tool_result(output.call_id, output.value)),
            Err(error) => events.push(event::Event::error(error.to_string())),
        }

        events
    }

    pub async fn handle(&mut self, command: KernelCommand) -> Result<KernelResponse> {
        match command {
            KernelCommand::SaveSkill { skill } => {
                self.save_skill(skill).await?;
                Ok(KernelResponse::Saved)
            }
            KernelCommand::SaveProfile { profile } => {
                self.save_profile(profile).await?;
                Ok(KernelResponse::Saved)
            }
            KernelCommand::SwitchModel { model } => {
                self.set_model(model.clone());
                Ok(KernelResponse::ModelSwitched { model })
            }
            KernelCommand::ChatTurn {
                session_id,
                message,
                assigned_skills,
            } => {
                let request = chat::TurnRequest::new(message).with_assigned_skills(assigned_skills);
                let output = self.chat_turn(&session_id, request).await?;
                Ok(KernelResponse::ChatTurn {
                    events: output.events,
                })
            }
            KernelCommand::StartRun {
                task,
                run_id,
                session_id,
            } => {
                let run = self.start_run(task, run_id, session_id).await?;
                Ok(KernelResponse::RunStarted { run })
            }
            KernelCommand::RunTask {
                run_id,
                task,
                message,
                assigned_skills,
            } => {
                let request =
                    run::TaskRequest::new(task, message).with_assigned_skills(assigned_skills);
                let output = self.run_task(&run_id, request).await?;
                Ok(KernelResponse::RunTask {
                    events: output.events,
                })
            }
            KernelCommand::CallTool { call } => Ok(KernelResponse::ToolEvents {
                events: self.call_tool_events(call).await,
            }),
        }
    }
}

async fn persist_assistant_text(
    sessions: &MemoryStore<chat::Session>,
    session_id: &str,
    events: &[event::Event],
) -> Result<()> {
    let text = events
        .iter()
        .filter_map(|event| match event {
            event::Event::Text { value } => Some(value.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    if !text.is_empty() {
        chat::append_message(
            sessions,
            session_id,
            chat::Message {
                role: chat::Role::Assistant,
                text,
            },
        )
        .await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    #[test]
    fn kernel_chat_turn_resolves_skill_and_persists_messages() {
        block_on_once(async {
            let kernel = Kernel::new(model::Model::new("openai", "gpt-5.5"));
            kernel
                .save_skill(
                    skill::Skill::new("team", "Team", skill::Source::System)
                        .with_content("Use parallel workers for independent work."),
                )
                .await
                .unwrap();

            let output = kernel
                .chat_turn("session-1", chat::TurnRequest::new("use @team"))
                .await
                .unwrap();

            assert_eq!(output.events.len(), 1);
            let session = kernel.sessions.get("session-1").await.unwrap().unwrap();
            assert_eq!(session.messages.len(), 2);
            assert_eq!(session.messages[0].role, chat::Role::User);
            assert!(session.messages[1].text.contains("Mentioned skill: @team"));
        });
    }

    #[test]
    fn kernel_run_task_marks_run_done() {
        block_on_once(async {
            let kernel = Kernel::new(model::Model::new("openai", "gpt-5.5"));
            let task = run::Task::new("task-1", "Review branch");
            kernel
                .start_run(task.clone(), "run-1", "session-1")
                .await
                .unwrap();

            kernel
                .run_task("run-1", run::TaskRequest::new(task, "summarize"))
                .await
                .unwrap();

            let run = kernel.runs.get("run-1").await.unwrap().unwrap();
            assert_eq!(run.status, run::Status::Done);
        });
    }

    #[test]
    fn kernel_tool_calls_emit_call_and_result_events() {
        block_on_once(async {
            let mut kernel = Kernel::new(model::Model::new("openai", "gpt-5.5"));
            kernel.tools.insert(EchoTool);

            let events = kernel
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
    fn kernel_missing_tool_emits_error_event() {
        block_on_once(async {
            let kernel = Kernel::new(model::Model::new("openai", "gpt-5.5"));

            let events = kernel
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
    fn kernel_handle_routes_chat_commands() {
        block_on_once(async {
            let mut kernel = Kernel::new(model::Model::new("openai", "gpt-5.5"));
            kernel
                .handle(KernelCommand::SaveSkill {
                    skill: skill::Skill::new("team", "Team", skill::Source::System)
                        .with_content("Use workers."),
                })
                .await
                .unwrap();

            let response = kernel
                .handle(KernelCommand::ChatTurn {
                    session_id: "session-1".to_string(),
                    message: "use @team".to_string(),
                    assigned_skills: Vec::new(),
                })
                .await
                .unwrap();

            match response {
                KernelResponse::ChatTurn { events } => {
                    assert_eq!(events.len(), 1);
                    assert!(matches!(events[0], event::Event::Text { .. }));
                }
                other => panic!("unexpected response: {other:?}"),
            }
        });
    }

    #[test]
    fn kernel_handle_routes_model_and_run_commands() {
        block_on_once(async {
            let mut kernel = Kernel::new(model::Model::new("openai", "gpt-5.4"));

            let response = kernel
                .handle(KernelCommand::SwitchModel {
                    model: model::Model::new("openai", "gpt-5.5"),
                })
                .await
                .unwrap();

            assert!(matches!(response, KernelResponse::ModelSwitched { .. }));
            assert_eq!(kernel.agent.model.id, "gpt-5.5");

            let task = run::Task::new("task-1", "Review branch");
            let response = kernel
                .handle(KernelCommand::StartRun {
                    task: task.clone(),
                    run_id: "run-1".to_string(),
                    session_id: "session-1".to_string(),
                })
                .await
                .unwrap();

            assert!(matches!(response, KernelResponse::RunStarted { .. }));

            let response = kernel
                .handle(KernelCommand::RunTask {
                    run_id: "run-1".to_string(),
                    task,
                    message: "summarize".to_string(),
                    assigned_skills: Vec::new(),
                })
                .await
                .unwrap();

            assert!(matches!(response, KernelResponse::RunTask { .. }));
            assert_eq!(
                kernel.runs.get("run-1").await.unwrap().unwrap().status,
                run::Status::Done
            );
        });
    }

    #[test]
    fn kernel_command_round_trips_through_json() {
        let command = KernelCommand::ChatTurn {
            session_id: "session-1".to_string(),
            message: "hello".to_string(),
            assigned_skills: vec!["team".to_string()],
        };

        let encoded = serde_json::to_string(&command).unwrap();
        let decoded: KernelCommand = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, command);
    }

    #[test]
    fn kernel_snapshot_exports_and_imports_state() {
        block_on_once(async {
            let mut kernel = Kernel::new(model::Model::new("openai", "gpt-5.5"));
            kernel.insert_model(model::ModelSpec::new("openai", "gpt-5.5", "GPT-5.5"));
            kernel.tools.insert(EchoTool);
            kernel
                .save_skill(skill::Skill::new("team", "Team", skill::Source::System))
                .await
                .unwrap();
            kernel
                .save_profile(auth::Profile::new("openai", "OPENAI_API_KEY"))
                .await
                .unwrap();
            kernel
                .chat_turn("session-1", chat::TurnRequest::new("hello"))
                .await
                .unwrap();
            kernel
                .start_run(
                    run::Task::new("task-1", "Review branch"),
                    "run-1",
                    "session-1",
                )
                .await
                .unwrap();

            let snapshot = kernel.snapshot().await.unwrap();
            let restored = Kernel::from_snapshot(snapshot.clone()).await.unwrap();
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
    fn kernel_snapshot_round_trips_through_json() {
        block_on_once(async {
            let kernel = Kernel::new(model::Model::new("openai", "gpt-5.5"));
            let snapshot = kernel.snapshot().await.unwrap();

            let encoded = serde_json::to_string(&snapshot).unwrap();
            let decoded: KernelSnapshot = serde_json::from_str(&encoded).unwrap();

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

    fn block_on_once<T>(future: impl Future<Output = T>) -> T {
        let waker = Waker::from(Arc::new(NoopWake));
        let mut context = Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);

        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("kernel future unexpectedly yielded"),
        }
    }

    struct NoopWake;

    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }
}
