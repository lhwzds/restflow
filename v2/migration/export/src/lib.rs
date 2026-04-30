//! # codocia
//!
//! Export owns one-way adapters from the current RestFlow domain into V2 bridge DTOs.
//!
//! ## Owns
//! - current RestFlow to BridgeSnapshot conversion
//! - field-level mapping for sessions, skills, tasks, runs, models, and profiles
//! - migration-only adapter helpers
//!
//! ## Must Not
//! - read production databases
//! - execute runtime behavior
//! - mutate current RestFlow records
//! - make production crates depend on V2
//! - become a normal V2 workspace member
//!
//! ## Inputs
//! - current RestFlow domain records
//! - current model reference
//!
//! ## Outputs
//! - BridgeSnapshot
//! - bridge DTO collections
//!
//! ## Depends On
//! - restflow-v2
//! - restflow-core
//! - restflow-models
//!
//! ## Verify
//! - cargo test --manifest-path v2/migration/export/Cargo.toml
//! - cargo clippy --manifest-path v2/migration/export/Cargo.toml --all-targets -- -D warnings

use bridge::{
    BridgeMessage, BridgeModelRef, BridgeModelSpec, BridgeProfile, BridgeRole, BridgeRun,
    BridgeSession, BridgeSkill, BridgeSkillSource, BridgeSnapshot, BridgeStatus, BridgeTask,
};
use restflow_core::auth::{AuthProfile, AuthProvider};
use restflow_core::models::{
    ChatMessage, ChatRole, ChatSession, ChatSessionSource, ContinuationConfig, DurabilityMode,
    ExecutionMode, MemoryConfig, ModelRef, NotificationConfig, ResourceLimits, Skill, SkillSource,
    SkillStatus, StorageMode, Task, TaskRun, TaskRunMetrics, TaskRunStatus,
};
use restflow_models::{ClientKind, ModelSpec};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub struct SnapshotParts {
    pub current_model: Option<ModelRef>,
    pub models: Vec<ModelSpec>,
    pub skills: Vec<Skill>,
    pub sessions: Vec<ChatSession>,
    pub tasks: Vec<Task>,
    pub runs: Vec<TaskRun>,
    pub profiles: Vec<AuthProfile>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExportResult {
    pub snapshot: BridgeSnapshot,
    pub report: ExportReport,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExportReport {
    pub issues: Vec<ExportIssue>,
}

impl ExportReport {
    pub fn is_clean(&self) -> bool {
        self.issues.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportIssue {
    pub kind: ExportIssueKind,
    pub record_id: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportIssueKind {
    CurrentModelMissingFromCatalog,
    SkillMetadataDropped,
    SessionMetadataDropped,
    MessageMetadataDropped,
    TaskMetadataDropped,
    RunMetricsDropped,
    ProfileMetadataDropped,
}

pub fn export_snapshot(parts: SnapshotParts) -> BridgeSnapshot {
    export_snapshot_with_report(parts).snapshot
}

pub fn export_snapshot_with_report(parts: SnapshotParts) -> ExportResult {
    let report = inspect_export_parts(&parts);
    let snapshot = build_snapshot(parts);
    ExportResult { snapshot, report }
}

pub fn inspect_export_parts(parts: &SnapshotParts) -> ExportReport {
    let mut report = ExportReport::default();

    record_current_model_loss(parts, &mut report);
    for skill in &parts.skills {
        record_skill_loss(skill, &mut report);
    }
    for session in &parts.sessions {
        record_session_loss(session, &mut report);
    }
    for task in &parts.tasks {
        record_task_loss(task, &mut report);
    }
    for run in &parts.runs {
        record_run_loss(run, &mut report);
    }
    for profile in &parts.profiles {
        record_profile_loss(profile, &mut report);
    }

    report
}

fn build_snapshot(parts: SnapshotParts) -> BridgeSnapshot {
    let task_sessions = parts
        .tasks
        .iter()
        .filter_map(|task| {
            empty_to_none(&task.chat_session_id).map(|session_id| (task.id.clone(), session_id))
        })
        .collect::<BTreeMap<_, _>>();
    let current_model = parts
        .current_model
        .map(bridge_model_ref)
        .or_else(|| parts.sessions.first().map(session_model_ref))
        .or_else(|| parts.models.first().map(model_spec_ref))
        .unwrap_or_else(|| BridgeModelRef::new("unknown", "unknown"));

    BridgeSnapshot {
        current_model,
        models: parts.models.iter().map(bridge_model_spec).collect(),
        skills: parts.skills.iter().map(bridge_skill).collect(),
        sessions: parts.sessions.iter().map(bridge_session).collect(),
        tasks: parts.tasks.iter().map(bridge_task).collect(),
        runs: parts
            .runs
            .iter()
            .map(|run| bridge_run_with_session(run, task_sessions.get(&run.task_id).cloned()))
            .collect(),
        profiles: parts.profiles.iter().map(bridge_profile).collect(),
        observed_tool_specs: Vec::new(),
    }
}

pub fn bridge_model_ref(value: ModelRef) -> BridgeModelRef {
    BridgeModelRef::new(value.provider.as_canonical_str(), value.model.as_str())
}

pub fn model_spec_ref(value: &ModelSpec) -> BridgeModelRef {
    BridgeModelRef::new(model_provider_label(value), value.name.clone())
}

pub fn bridge_model_spec(value: &ModelSpec) -> BridgeModelSpec {
    BridgeModelSpec {
        provider: model_provider_label(value).to_string(),
        model: value.name.clone(),
        name: value.name.clone(),
        description: None,
        client_model: Some(value.client_model.clone()),
        client_kind: Some(value.client_kind.as_str().to_string()),
        base_url: value.base_url.clone(),
    }
}

pub fn bridge_skill(value: &Skill) -> BridgeSkill {
    BridgeSkill {
        id: value.id.clone(),
        name: value.name.clone(),
        source: bridge_skill_source(value.source),
        read_only: value.read_only,
        description: value.description.clone(),
        content: value.content.clone(),
        suggested_tools: value.suggested_tools.clone(),
        source_ref: value.source_ref.clone(),
    }
}

pub fn bridge_session(value: &ChatSession) -> BridgeSession {
    BridgeSession {
        id: value.id.clone(),
        name: Some(value.name.clone()),
        agent_id: Some(value.agent_id.clone()),
        provider: empty_to_none(&value.provider),
        model: Some(value.model.clone()),
        source: value.source_channel.map(source_label),
        created_at: Some(value.created_at.to_string()),
        updated_at: Some(value.updated_at.to_string()),
        archived_at: value.archived_at.map(|value| value.to_string()),
        messages: value.messages.iter().map(bridge_message).collect(),
    }
}

pub fn bridge_message(value: &ChatMessage) -> BridgeMessage {
    BridgeMessage {
        role: bridge_role(&value.role),
        text: value.content.clone(),
    }
}

pub fn bridge_task(value: &Task) -> BridgeTask {
    BridgeTask {
        id: value.id.clone(),
        title: value.name.clone(),
        input: value.input.clone(),
        agent_id: Some(value.agent_id.clone()),
        session_id: empty_to_none(&value.chat_session_id),
        status: Some(value.status.as_str().to_string()),
        schedule: serde_json::to_string(&value.schedule).ok(),
        created_at: Some(value.created_at.to_string()),
        updated_at: Some(value.updated_at.to_string()),
        error: value.last_error.clone(),
    }
}

pub fn bridge_run(value: &TaskRun) -> BridgeRun {
    bridge_run_with_session(value, None)
}

pub fn bridge_run_with_session(value: &TaskRun, session_id: Option<String>) -> BridgeRun {
    BridgeRun {
        id: value.run_id.clone(),
        task_id: value.task_id.clone(),
        status: bridge_status(&value.status),
        raw_status: Some(value.status.as_str().to_string()),
        session_id,
        execution_id: Some(value.execution_id.clone()),
        checkpoint_id: value.checkpoint_id.clone(),
        error: value.error.clone(),
        started_at: Some(value.started_at.to_string()),
        updated_at: Some(value.updated_at.to_string()),
        ended_at: value.ended_at.map(|value| value.to_string()),
    }
}

pub fn bridge_profile(value: &AuthProfile) -> BridgeProfile {
    BridgeProfile {
        provider: auth_provider_label(value.provider).to_string(),
        secret_key: value.credential.primary_secret_ref().to_string(),
    }
}

fn bridge_skill_source(value: SkillSource) -> BridgeSkillSource {
    match value {
        SkillSource::System => BridgeSkillSource::System,
        SkillSource::User => BridgeSkillSource::User,
        SkillSource::External => BridgeSkillSource::External,
    }
}

fn bridge_role(value: &ChatRole) -> BridgeRole {
    match value {
        ChatRole::User => BridgeRole::User,
        ChatRole::Assistant => BridgeRole::Assistant,
        ChatRole::System => BridgeRole::System,
    }
}

fn bridge_status(value: &TaskRunStatus) -> BridgeStatus {
    match value {
        TaskRunStatus::Running => BridgeStatus::Running,
        TaskRunStatus::Completed => BridgeStatus::Done,
        TaskRunStatus::Failed | TaskRunStatus::TimedOut => BridgeStatus::Failed,
        TaskRunStatus::Interrupted => BridgeStatus::Canceled,
    }
}

fn source_label(value: ChatSessionSource) -> String {
    match value {
        ChatSessionSource::Workspace => "workspace",
        ChatSessionSource::Background => "background",
        ChatSessionSource::Telegram => "telegram",
        ChatSessionSource::Discord => "discord",
        ChatSessionSource::Slack => "slack",
    }
    .to_string()
}

fn auth_provider_label(value: AuthProvider) -> &'static str {
    match value {
        AuthProvider::Anthropic => "anthropic",
        AuthProvider::ClaudeCode => "claude-code",
        AuthProvider::OpenAI => "openai",
        AuthProvider::OpenAICodex => "codex",
        AuthProvider::Google => "google",
        AuthProvider::Other => "other",
    }
}

fn model_provider_label(value: &ModelSpec) -> &'static str {
    match value.client_kind {
        ClientKind::Http => value.provider.as_str(),
        ClientKind::CodexCli => "codex",
        ClientKind::OpenCodeCli => "opencode",
        ClientKind::GeminiCli => "gemini-cli",
        ClientKind::ClaudeCodeCli => "claude-code",
    }
}

fn session_model_ref(value: &ChatSession) -> BridgeModelRef {
    BridgeModelRef::new(
        empty_to_none(&value.provider).unwrap_or_else(|| "unknown".to_string()),
        value.model.clone(),
    )
}

fn empty_to_none(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn record_current_model_loss(parts: &SnapshotParts, report: &mut ExportReport) {
    let current_model = parts
        .current_model
        .as_ref()
        .copied()
        .map(bridge_model_ref)
        .or_else(|| parts.sessions.first().map(session_model_ref))
        .or_else(|| parts.models.first().map(model_spec_ref))
        .unwrap_or_else(|| BridgeModelRef::new("unknown", "unknown"));
    let current_model_exists = parts.models.iter().any(|spec| {
        model_provider_label(spec) == current_model.provider && spec.name == current_model.model
    });

    if !current_model_exists {
        report_loss(
            report,
            ExportIssueKind::CurrentModelMissingFromCatalog,
            "current_model",
            format!(
                "current model is not present in the exported model catalog: {}:{}",
                current_model.provider, current_model.model
            ),
        );
    }
}

fn record_skill_loss(value: &Skill, report: &mut ExportReport) {
    if value.tags.as_ref().is_some_and(|tags| !tags.is_empty())
        || !value.triggers.is_empty()
        || value.folder_path.is_some()
        || !value.scripts.is_empty()
        || !value.references.is_empty()
        || value.gating.is_some()
        || value.version.is_some()
        || value.author.is_some()
        || value.license.is_some()
        || value.content_hash.is_some()
        || value.status != SkillStatus::Active
        || value.auto_complete
        || value.storage_mode != StorageMode::DatabaseOnly
        || value.is_synced
    {
        report_loss(
            report,
            ExportIssueKind::SkillMetadataDropped,
            &value.id,
            "skill metadata outside the v2 skill core shape is not exported",
        );
    }
}

fn record_session_loss(value: &ChatSession, report: &mut ExportReport) {
    if value.skill_id.is_some()
        || value.retention.is_some()
        || value.summary_message_id.is_some()
        || value.prompt_tokens != 0
        || value.completion_tokens != 0
        || value.cost != 0.0
        || value.metadata.total_tokens != 0
        || value.metadata.message_count != 0
        || value.source_conversation_id.is_some()
    {
        report_loss(
            report,
            ExportIssueKind::SessionMetadataDropped,
            &value.id,
            "session counters, retention, summaries, and channel conversation metadata are not exported",
        );
    }

    if !value.messages.is_empty() {
        report_loss(
            report,
            ExportIssueKind::MessageMetadataDropped,
            &value.id,
            "message ids and timestamps are not represented in the v2 message shape",
        );
    }

    if value.messages.iter().any(|message| {
        message.execution.is_some() || message.media.is_some() || message.transcript.is_some()
    }) {
        report_loss(
            report,
            ExportIssueKind::MessageMetadataDropped,
            &value.id,
            "rich message execution, media, or transcript metadata is not exported",
        );
    }
}

fn record_task_loss(value: &Task, report: &mut ExportReport) {
    if value.description.is_some()
        || value.owns_chat_session
        || value.input_template.is_some()
        || value.execution_mode != ExecutionMode::default()
        || value.timeout_secs.is_some()
        || value.notification != NotificationConfig::default()
        || value.memory != MemoryConfig::default()
        || value.durability_mode != DurabilityMode::default()
        || value.resource_limits != ResourceLimits::default()
        || !value.prerequisites.is_empty()
        || value.continuation != ContinuationConfig::default()
        || value.continuation_total_iterations != 0
        || value.continuation_segments_completed != 0
        || value.last_run_at.is_some()
        || value.next_run_at.is_some()
        || value.success_count != 0
        || value.failure_count != 0
        || value.total_tokens_used != 0
        || value.total_cost_usd != 0.0
        || value.webhook.is_some()
        || value.summary_message_id.is_some()
    {
        report_loss(
            report,
            ExportIssueKind::TaskMetadataDropped,
            &value.id,
            "task execution policy, counters, dependencies, and runtime metadata are not exported",
        );
    }
}

fn record_run_loss(value: &TaskRun, report: &mut ExportReport) {
    if value.metrics != TaskRunMetrics::default() {
        report_loss(
            report,
            ExportIssueKind::RunMetricsDropped,
            &value.run_id,
            "run metrics are not represented in the v2 run shape",
        );
    }
}

fn record_profile_loss(value: &AuthProfile, report: &mut ExportReport) {
    report_loss(
        report,
        ExportIssueKind::ProfileMetadataDropped,
        &value.id,
        "auth profile identity, display metadata, health, source, and priority are not exported",
    );
}

fn report_loss(
    report: &mut ExportReport,
    kind: ExportIssueKind,
    record_id: &str,
    message: impl Into<String>,
) {
    report.issues.push(ExportIssue {
        kind,
        record_id: record_id.to_string(),
        message: message.into(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_core::auth::{CredentialSource, SecureCredential};
    use restflow_core::models::{ChatMessage, ChatSession, ModelId, SkillStatus, TaskSchedule};
    use restflow_models::LlmProvider;

    #[test]
    fn exports_sessions_skills_tasks_runs_and_profiles() {
        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5.5".to_string());
        session.id = "session-1".to_string();
        session.name = "Demo".to_string();
        session.source_channel = Some(ChatSessionSource::Workspace);
        session.add_message(ChatMessage::user("hello"));

        let mut skill = Skill::new(
            "team".to_string(),
            "Team".to_string(),
            Some("Coordinate workers.".to_string()),
            None,
            "Use workers.".to_string(),
        );
        skill.status = SkillStatus::Active;
        skill.source = SkillSource::System;
        skill.read_only = true;
        skill.source_ref = Some("restflow://system/team".to_string());
        skill.suggested_tools = vec!["spawn_agent".to_string()];

        let mut task = Task::new(
            "task-1".to_string(),
            "Review branch".to_string(),
            "agent-1".to_string(),
            TaskSchedule::Once { run_at: 123 },
        );
        task.chat_session_id = "session-1".to_string();
        task.input = Some("review".to_string());

        let mut run = TaskRun::new("run-1", "task-1", "exec-1", 100, Some("cp-1".to_string()));
        run.mark_terminal(TaskRunStatus::Completed, 200, None, Default::default());

        let profile = AuthProfile::new_with_id(
            "profile-1".to_string(),
            "OpenAI",
            SecureCredential::ApiKey {
                secret_ref: "OPENAI_API_KEY".to_string(),
                email: None,
            },
            CredentialSource::Manual,
            AuthProvider::OpenAI,
        );

        let snapshot = export_snapshot(SnapshotParts {
            current_model: Some(ModelRef::from_model(ModelId::Gpt5_5)),
            models: vec![ModelSpec::new("gpt-5.5", LlmProvider::OpenAI, "gpt-5.5")],
            skills: vec![skill],
            sessions: vec![session],
            tasks: vec![task],
            runs: vec![run],
            profiles: vec![profile],
        });

        assert_eq!(snapshot.current_model.provider, "openai");
        assert_eq!(snapshot.current_model.model, "gpt-5.5");
        assert_eq!(snapshot.models[0].provider, "openai");
        assert_eq!(snapshot.models[0].client_model.as_deref(), Some("gpt-5.5"));
        assert_eq!(snapshot.models[0].client_kind.as_deref(), Some("http"));
        assert_eq!(snapshot.skills[0].source, BridgeSkillSource::System);
        assert_eq!(
            snapshot.skills[0].source_ref.as_deref(),
            Some("restflow://system/team")
        );
        assert_eq!(snapshot.sessions[0].source.as_deref(), Some("workspace"));
        assert_eq!(snapshot.sessions[0].messages[0].role, BridgeRole::User);
        assert_eq!(snapshot.tasks[0].session_id.as_deref(), Some("session-1"));
        assert_eq!(snapshot.runs[0].session_id.as_deref(), Some("session-1"));
        assert_eq!(snapshot.runs[0].status, BridgeStatus::Done);
        assert_eq!(snapshot.runs[0].raw_status.as_deref(), Some("completed"));
        assert_eq!(snapshot.profiles[0].secret_key, "OPENAI_API_KEY");
    }

    #[test]
    fn export_report_surfaces_lossy_metadata() {
        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5.5".to_string());
        session.id = "session-1".to_string();
        session.skill_id = Some("team".to_string());
        session.prompt_tokens = 10;
        session.add_message(ChatMessage::user("hello"));

        let mut skill = Skill::new(
            "team".to_string(),
            "Team".to_string(),
            Some("Coordinate workers.".to_string()),
            Some(vec!["coordination".to_string()]),
            "Use workers.".to_string(),
        );
        skill.status = SkillStatus::Draft;

        let mut task = Task::new(
            "task-1".to_string(),
            "Review branch".to_string(),
            "agent-1".to_string(),
            TaskSchedule::Once { run_at: 123 },
        );
        task.description = Some("Review the current branch.".to_string());

        let mut run = TaskRun::new("run-1", "task-1", "exec-1", 100, Some("cp-1".to_string()));
        run.metrics.iterations = Some(3);

        let profile = AuthProfile::new_with_id(
            "profile-1".to_string(),
            "OpenAI",
            SecureCredential::ApiKey {
                secret_ref: "OPENAI_API_KEY".to_string(),
                email: None,
            },
            CredentialSource::Manual,
            AuthProvider::OpenAI,
        );

        let result = export_snapshot_with_report(SnapshotParts {
            current_model: Some(ModelRef::from_model(ModelId::Gpt5_5)),
            models: vec![ModelSpec::new("gpt-5.5", LlmProvider::OpenAI, "gpt-5.5")],
            skills: vec![skill],
            sessions: vec![session],
            tasks: vec![task],
            runs: vec![run],
            profiles: vec![profile],
        });

        assert_eq!(result.snapshot.current_model.model, "gpt-5.5");
        assert!(!result.report.is_clean());
        assert!(has_issue(
            &result.report,
            ExportIssueKind::SkillMetadataDropped
        ));
        assert!(has_issue(
            &result.report,
            ExportIssueKind::SessionMetadataDropped
        ));
        assert!(has_issue(
            &result.report,
            ExportIssueKind::MessageMetadataDropped
        ));
        assert!(has_issue(
            &result.report,
            ExportIssueKind::TaskMetadataDropped
        ));
        assert!(has_issue(
            &result.report,
            ExportIssueKind::RunMetricsDropped
        ));
        assert!(has_issue(
            &result.report,
            ExportIssueKind::ProfileMetadataDropped
        ));
    }

    #[test]
    fn export_report_is_clean_for_core_model_only_snapshot() {
        let report = inspect_export_parts(&SnapshotParts {
            current_model: Some(ModelRef::from_model(ModelId::Gpt5_5)),
            models: vec![ModelSpec::new("gpt-5.5", LlmProvider::OpenAI, "gpt-5.5")],
            ..SnapshotParts::default()
        });

        assert!(report.is_clean(), "{:?}", report.issues);
    }

    #[test]
    fn export_report_warns_when_current_model_is_missing_from_catalog() {
        let report = inspect_export_parts(&SnapshotParts {
            current_model: Some(ModelRef::from_model(ModelId::Gpt5_5)),
            ..SnapshotParts::default()
        });

        assert!(has_issue(
            &report,
            ExportIssueKind::CurrentModelMissingFromCatalog
        ));
    }

    #[test]
    fn exported_snapshot_imports_into_v2_core() {
        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5.5".to_string());
        session.id = "session-1".to_string();
        session.provider = "openai".to_string();
        session.add_message(ChatMessage::user("hello"));

        let snapshot = export_snapshot(SnapshotParts {
            current_model: Some(ModelRef::from_model(ModelId::Gpt5_5)),
            models: vec![ModelSpec::new("gpt-5.5", LlmProvider::OpenAI, "gpt-5.5")],
            sessions: vec![session],
            ..SnapshotParts::default()
        });

        let (_core, report) = block_on_once(bridge::core_from_bridge_snapshot(snapshot)).unwrap();

        assert!(report.is_clean(), "{:?}", report.issues);
        assert!(report.applied);
    }

    #[test]
    fn exports_cli_model_provider_from_client_kind() {
        let snapshot = export_snapshot(SnapshotParts {
            models: vec![ModelSpec::codex("gpt-5.5-codex", "gpt-5.5")],
            ..SnapshotParts::default()
        });

        assert_eq!(snapshot.current_model.provider, "codex");
        assert_eq!(snapshot.models[0].provider, "codex");
        assert_eq!(snapshot.models[0].client_kind.as_deref(), Some("codex-cli"));
        assert_eq!(snapshot.models[0].client_model.as_deref(), Some("gpt-5.5"));
    }

    #[test]
    fn falls_back_to_first_session_model_when_current_model_is_missing() {
        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5.5".to_string());
        session.provider = "openai".to_string();

        let snapshot = export_snapshot(SnapshotParts {
            sessions: vec![session],
            ..SnapshotParts::default()
        });

        assert_eq!(snapshot.current_model.provider, "openai");
        assert_eq!(snapshot.current_model.model, "gpt-5.5");
    }

    fn block_on_once<T>(future: impl std::future::Future<Output = T>) -> T {
        use std::sync::Arc;
        use std::task::{Context, Poll, Waker};

        let waker = Waker::from(Arc::new(NoopWake));
        let mut context = Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);

        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("migration future unexpectedly yielded"),
        }
    }

    struct NoopWake;

    impl std::task::Wake for NoopWake {
        fn wake(self: std::sync::Arc<Self>) {}
    }

    fn has_issue(report: &ExportReport, kind: ExportIssueKind) -> bool {
        report.issues.iter().any(|issue| issue.kind == kind)
    }
}
