//! # codocia
//!
//! Bridge owns adapter-safe DTOs for moving legacy boundary data into V2 Core.
//!
//! ## Owns
//! - migration DTOs
//! - boundary-safe snapshot shape
//! - legacy-to-core value conversion
//! - command conversion helpers
//!
//! ## Must Not
//! - depend on legacy RestFlow crates
//! - read or write durable storage
//! - execute runtime behavior
//! - own UI interaction state
//!
//! ## Inputs
//! - legacy-compatible model references
//! - legacy-compatible skills
//! - legacy-compatible sessions
//! - legacy-compatible tasks and runs
//! - legacy-compatible profiles
//! - legacy-compatible tool calls
//!
//! ## Outputs
//! - CoreSnapshot
//! - CoreCommand
//! - V2 domain values
//!
//! ## Depends On
//! - auth
//! - chat
//! - model
//! - run
//! - skill
//! - tool
//!
//! ## Verify
//! - cargo check -p restflow-v2

use crate::{CoreCommand, CoreSnapshot, auth, chat, model, run, skill, tool};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeModelRef {
    pub provider: String,
    pub model: String,
}

impl BridgeModelRef {
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeModelSpec {
    pub provider: String,
    pub model: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeSkillSource {
    System,
    User,
    External,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeSkill {
    pub id: String,
    pub name: String,
    pub source: BridgeSkillSource,
    pub read_only: bool,
    pub description: Option<String>,
    pub content: String,
    pub suggested_tools: Vec<String>,
    pub source_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeRole {
    User,
    Assistant,
    Tool,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeMessage {
    pub role: BridgeRole,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeSession {
    pub id: String,
    pub name: Option<String>,
    pub agent_id: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub source: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub archived_at: Option<String>,
    pub messages: Vec<BridgeMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeTask {
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
#[serde(rename_all = "snake_case")]
pub enum BridgeStatus {
    Pending,
    Running,
    Done,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeRun {
    pub id: String,
    pub task_id: String,
    pub status: BridgeStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
pub struct BridgeProfile {
    pub provider: String,
    pub secret_key: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BridgeToolSpec {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BridgeToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeChatTurn {
    pub session_id: String,
    pub message: String,
    pub assigned_skills: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeRunTask {
    pub run_id: String,
    pub task: BridgeTask,
    pub message: String,
    pub assigned_skills: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BridgeSnapshot {
    pub current_model: BridgeModelRef,
    pub models: Vec<BridgeModelSpec>,
    pub skills: Vec<BridgeSkill>,
    pub sessions: Vec<BridgeSession>,
    pub tasks: Vec<BridgeTask>,
    pub runs: Vec<BridgeRun>,
    pub profiles: Vec<BridgeProfile>,
    #[serde(default, alias = "tool_specs")]
    pub observed_tool_specs: Vec<BridgeToolSpec>,
}

impl From<BridgeModelRef> for model::Model {
    fn from(value: BridgeModelRef) -> Self {
        model::Model::new(value.provider, value.model)
    }
}

impl From<BridgeModelSpec> for model::ModelSpec {
    fn from(value: BridgeModelSpec) -> Self {
        let mut spec = model::ModelSpec::new(value.provider, value.model, value.name);
        if let Some(description) = value.description {
            spec = spec.with_description(description);
        }
        if let Some(client_model) = value.client_model {
            spec = spec.with_client_model(client_model);
        }
        if let Some(client_kind) = value.client_kind {
            spec = spec.with_client_kind(client_kind);
        }
        if let Some(base_url) = value.base_url {
            spec = spec.with_base_url(base_url);
        }
        spec
    }
}

impl From<BridgeSkillSource> for skill::Source {
    fn from(value: BridgeSkillSource) -> Self {
        match value {
            BridgeSkillSource::System => Self::System,
            BridgeSkillSource::User => Self::User,
            BridgeSkillSource::External => Self::External,
        }
    }
}

impl From<BridgeSkill> for skill::Skill {
    fn from(value: BridgeSkill) -> Self {
        skill::Skill {
            id: value.id,
            name: value.name,
            source: value.source.into(),
            source_ref: value.source_ref,
            read_only: value.read_only,
            description: value.description,
            content: value.content,
            suggested_tools: value.suggested_tools,
        }
    }
}

impl From<BridgeRole> for chat::Role {
    fn from(value: BridgeRole) -> Self {
        match value {
            BridgeRole::User => Self::User,
            BridgeRole::Assistant => Self::Assistant,
            BridgeRole::Tool => Self::Tool,
            BridgeRole::System => Self::System,
        }
    }
}

impl From<BridgeMessage> for chat::Message {
    fn from(value: BridgeMessage) -> Self {
        Self {
            role: value.role.into(),
            text: value.text,
        }
    }
}

impl From<BridgeSession> for chat::Session {
    fn from(value: BridgeSession) -> Self {
        Self {
            id: value.id,
            name: value.name,
            agent_id: value.agent_id,
            provider: value.provider,
            model: value.model,
            source: value.source,
            created_at: value.created_at,
            updated_at: value.updated_at,
            archived_at: value.archived_at,
            messages: value.messages.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<BridgeTask> for run::Task {
    fn from(value: BridgeTask) -> Self {
        Self {
            id: value.id,
            title: value.title,
            input: value.input,
            agent_id: value.agent_id,
            session_id: value.session_id,
            status: value.status,
            schedule: value.schedule,
            created_at: value.created_at,
            updated_at: value.updated_at,
            error: value.error,
        }
    }
}

impl From<BridgeStatus> for run::Status {
    fn from(value: BridgeStatus) -> Self {
        match value {
            BridgeStatus::Pending => Self::Pending,
            BridgeStatus::Running => Self::Running,
            BridgeStatus::Done => Self::Done,
            BridgeStatus::Failed => Self::Failed,
            BridgeStatus::Canceled => Self::Canceled,
        }
    }
}

impl From<BridgeRun> for run::Run {
    fn from(value: BridgeRun) -> Self {
        Self {
            id: value.id,
            task_id: value.task_id,
            status: value.status.into(),
            raw_status: value.raw_status,
            session_id: value.session_id,
            execution_id: value.execution_id,
            checkpoint_id: value.checkpoint_id,
            error: value.error,
            started_at: value.started_at,
            updated_at: value.updated_at,
            ended_at: value.ended_at,
        }
    }
}

impl From<BridgeProfile> for auth::Profile {
    fn from(value: BridgeProfile) -> Self {
        auth::Profile::new(value.provider, value.secret_key)
    }
}

impl From<BridgeToolSpec> for tool::ToolSpec {
    fn from(value: BridgeToolSpec) -> Self {
        tool::ToolSpec {
            name: value.name,
            description: value.description,
            input_schema: value.input_schema,
        }
    }
}

impl From<BridgeToolCall> for tool::ToolCall {
    fn from(value: BridgeToolCall) -> Self {
        tool::ToolCall::new(value.id, value.name, value.input)
    }
}

impl From<BridgeChatTurn> for CoreCommand {
    fn from(value: BridgeChatTurn) -> Self {
        Self::ChatTurn {
            session_id: value.session_id,
            message: value.message,
            assigned_skills: value.assigned_skills,
        }
    }
}

impl From<BridgeRunTask> for CoreCommand {
    fn from(value: BridgeRunTask) -> Self {
        Self::RunTask {
            run_id: value.run_id,
            task: value.task.into(),
            message: value.message,
            assigned_skills: value.assigned_skills,
        }
    }
}

impl From<BridgeSnapshot> for CoreSnapshot {
    fn from(value: BridgeSnapshot) -> Self {
        Self {
            current_model: value.current_model.into(),
            models: value.models.into_iter().map(Into::into).collect(),
            skills: value.skills.into_iter().map(Into::into).collect(),
            sessions: value.sessions.into_iter().map(Into::into).collect(),
            tasks: value.tasks.into_iter().map(Into::into).collect(),
            runs: value.runs.into_iter().map(Into::into).collect(),
            profiles: value.profiles.into_iter().map(Into::into).collect(),
            tool_specs: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Core;
    use std::future::Future;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    #[test]
    fn bridge_snapshot_converts_into_core_snapshot() {
        block_on_once(async {
            let bridge = sample_snapshot();
            let snapshot: CoreSnapshot = bridge.into();
            let core = Core::from_snapshot(snapshot).await.unwrap();
            let restored = core.snapshot().await.unwrap();

            assert_eq!(restored.current_model.id, "gpt-5.5");
            assert_eq!(restored.models.len(), 1);
            assert_eq!(restored.skills[0].source, skill::Source::System);
            assert_eq!(restored.sessions[0].messages[0].role, chat::Role::User);
            assert_eq!(restored.tasks[0].title, "Review branch");
            assert_eq!(restored.runs[0].status, run::Status::Done);
            assert_eq!(restored.profiles[0].secret.key, "OPENAI_API_KEY");
            assert!(restored.tool_specs.is_empty());
        });
    }

    #[test]
    fn bridge_chat_turn_converts_into_core_command() {
        let command: CoreCommand = BridgeChatTurn {
            session_id: "session-1".to_string(),
            message: "use @team".to_string(),
            assigned_skills: vec!["team".to_string()],
        }
        .into();

        assert_eq!(
            command,
            CoreCommand::ChatTurn {
                session_id: "session-1".to_string(),
                message: "use @team".to_string(),
                assigned_skills: vec!["team".to_string()],
            }
        );
    }

    #[test]
    fn bridge_run_task_converts_into_core_command() {
        let command: CoreCommand = BridgeRunTask {
            run_id: "run-1".to_string(),
            task: BridgeTask {
                id: "task-1".to_string(),
                title: "Review branch".to_string(),
                input: Some("review this branch".to_string()),
                agent_id: Some("default".to_string()),
                session_id: Some("session-1".to_string()),
                status: Some("active".to_string()),
                schedule: None,
                created_at: Some("2026-04-29T00:00:00Z".to_string()),
                updated_at: Some("2026-04-29T00:00:01Z".to_string()),
                error: None,
            },
            message: "summarize".to_string(),
            assigned_skills: vec!["review".to_string()],
        }
        .into();

        match command {
            CoreCommand::RunTask {
                run_id,
                task,
                message,
                assigned_skills,
            } => {
                assert_eq!(run_id, "run-1");
                assert_eq!(task.id, "task-1");
                assert_eq!(message, "summarize");
                assert_eq!(assigned_skills, vec!["review"]);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn bridge_snapshot_round_trips_through_json() {
        let snapshot = sample_snapshot();

        let encoded = serde_json::to_string(&snapshot).unwrap();
        let decoded: BridgeSnapshot = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, snapshot);
    }

    #[test]
    fn bridge_tool_call_converts_to_core_tool_call() {
        let call: tool::ToolCall = BridgeToolCall {
            id: "call-1".to_string(),
            name: "echo".to_string(),
            input: serde_json::json!({ "message": "hello" }),
        }
        .into();

        assert_eq!(call.id, "call-1");
        assert_eq!(call.name, "echo");
        assert_eq!(call.input, serde_json::json!({ "message": "hello" }));
    }

    fn sample_snapshot() -> BridgeSnapshot {
        BridgeSnapshot {
            current_model: BridgeModelRef::new("openai", "gpt-5.5"),
            models: vec![BridgeModelSpec {
                provider: "openai".to_string(),
                model: "gpt-5.5".to_string(),
                name: "GPT-5.5".to_string(),
                description: Some("Frontier model".to_string()),
                client_model: Some("gpt-5.5".to_string()),
                client_kind: Some("http".to_string()),
                base_url: None,
            }],
            skills: vec![BridgeSkill {
                id: "team".to_string(),
                name: "Team".to_string(),
                source: BridgeSkillSource::System,
                read_only: true,
                description: Some("Coordinate workers.".to_string()),
                content: "Use workers for independent tasks.".to_string(),
                suggested_tools: vec!["spawn_agent".to_string()],
                source_ref: Some("system://team".to_string()),
            }],
            sessions: vec![BridgeSession {
                id: "session-1".to_string(),
                name: Some("Demo session".to_string()),
                agent_id: Some("default".to_string()),
                provider: Some("openai".to_string()),
                model: Some("gpt-5.5".to_string()),
                source: Some("workspace".to_string()),
                created_at: Some("2026-04-29T00:00:00Z".to_string()),
                updated_at: Some("2026-04-29T00:00:01Z".to_string()),
                archived_at: None,
                messages: vec![BridgeMessage {
                    role: BridgeRole::User,
                    text: "hello".to_string(),
                }],
            }],
            tasks: vec![BridgeTask {
                id: "task-1".to_string(),
                title: "Review branch".to_string(),
                input: Some("review this branch".to_string()),
                agent_id: Some("default".to_string()),
                session_id: Some("session-1".to_string()),
                status: Some("active".to_string()),
                schedule: None,
                created_at: Some("2026-04-29T00:00:00Z".to_string()),
                updated_at: Some("2026-04-29T00:00:01Z".to_string()),
                error: None,
            }],
            runs: vec![BridgeRun {
                id: "run-1".to_string(),
                task_id: "task-1".to_string(),
                status: BridgeStatus::Done,
                raw_status: Some("completed".to_string()),
                session_id: Some("session-1".to_string()),
                execution_id: Some("exec-1".to_string()),
                checkpoint_id: Some("checkpoint-1".to_string()),
                error: None,
                started_at: Some("2026-04-29T00:00:00Z".to_string()),
                updated_at: Some("2026-04-29T00:00:01Z".to_string()),
                ended_at: Some("2026-04-29T00:00:02Z".to_string()),
            }],
            profiles: vec![BridgeProfile {
                provider: "openai".to_string(),
                secret_key: "OPENAI_API_KEY".to_string(),
            }],
            observed_tool_specs: vec![BridgeToolSpec {
                name: "echo".to_string(),
                description: Some("Return input.".to_string()),
                input_schema: serde_json::json!({ "type": "object" }),
            }],
        }
    }

    fn block_on_once<T>(future: impl Future<Output = T>) -> T {
        let waker = Waker::from(Arc::new(NoopWake));
        let mut context = Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);

        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("bridge future unexpectedly yielded"),
        }
    }

    struct NoopWake;

    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }
}
