//! # codocia
//!
//! Migrate owns one-way import from bridge DTOs into an in-memory V2 Core.
//!
//! ## Owns
//! - bridge snapshot import
//! - import report counts
//! - migration consistency checks
//! - safe Core construction from bridge input
//!
//! ## Must Not
//! - depend on legacy RestFlow crates
//! - read or write production databases
//! - execute tools
//! - hide lossy migration warnings
//!
//! ## Inputs
//! - BridgeSnapshot
//! - existing Core
//!
//! ## Outputs
//! - populated Core
//! - MigrationReport
//! - MigrationIssue list
//!
//! ## Depends On
//! - bridge
//! - chat
//! - model
//! - run
//! - store
//!
//! ## Verify
//! - cargo check -p restflow-v2

use crate::{Core, bridge::BridgeSnapshot, chat, model, run};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use store::Repository;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportMode {
    CreateOnly,
    ReplaceExisting,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationReport {
    pub applied: bool,
    pub models: usize,
    pub skills: usize,
    pub sessions: usize,
    pub messages: usize,
    pub tasks: usize,
    pub runs: usize,
    pub profiles: usize,
    pub tool_specs: usize,
    pub issues: Vec<MigrationIssue>,
}

impl MigrationReport {
    pub fn is_clean(&self) -> bool {
        self.issues.is_empty()
    }

    pub fn has_blocking_issues(&self) -> bool {
        self.issues.iter().any(MigrationIssue::is_blocking)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationIssue {
    pub kind: MigrationIssueKind,
    pub message: String,
}

impl MigrationIssue {
    fn new(kind: MigrationIssueKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn is_blocking(&self) -> bool {
        matches!(
            self.kind,
            MigrationIssueKind::CurrentModelMissingFromCatalog
                | MigrationIssueKind::RunReferencesMissingTask
                | MigrationIssueKind::RunReferencesMissingSession
                | MigrationIssueKind::DuplicateId
                | MigrationIssueKind::ExistingRecord
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationIssueKind {
    CurrentModelMissingFromCatalog,
    RunReferencesMissingTask,
    RunReferencesMissingSession,
    DuplicateId,
    ExistingRecord,
    ToolSpecsAreInformational,
}

pub async fn core_from_bridge_snapshot(
    snapshot: BridgeSnapshot,
) -> Result<(Core, MigrationReport)> {
    let current_model = snapshot.current_model.clone().into();
    let mut core = Core::new(current_model);
    let report = import_bridge_snapshot(&mut core, snapshot).await?;
    Ok((core, report))
}

pub async fn import_bridge_snapshot(
    core: &mut Core,
    snapshot: BridgeSnapshot,
) -> Result<MigrationReport> {
    import_bridge_snapshot_with_mode(core, snapshot, ImportMode::CreateOnly).await
}

pub async fn replace_bridge_snapshot(
    core: &mut Core,
    snapshot: BridgeSnapshot,
) -> Result<MigrationReport> {
    import_bridge_snapshot_with_mode(core, snapshot, ImportMode::ReplaceExisting).await
}

pub async fn import_bridge_snapshot_with_mode(
    core: &mut Core,
    snapshot: BridgeSnapshot,
    mode: ImportMode,
) -> Result<MigrationReport> {
    let mut report = inspect_bridge_snapshot(&snapshot);
    if mode == ImportMode::CreateOnly {
        report
            .issues
            .extend(existing_record_issues(core, &snapshot).await?);
    }

    if report.has_blocking_issues() {
        return Ok(report);
    }

    core.set_model(snapshot.current_model.into());
    for spec in snapshot.models {
        core.insert_model(spec.into());
    }
    for skill in snapshot.skills {
        core.save_skill(skill.into()).await?;
    }
    for session in snapshot.sessions {
        let session: chat::Session = session.into();
        chat::save_session(&core.sessions, session).await?;
    }
    for task in snapshot.tasks {
        run::save_task(&core.tasks, task.into()).await?;
    }
    for run in snapshot.runs {
        run::save_run(&core.runs, run.into()).await?;
    }
    for profile in snapshot.profiles {
        core.save_profile(profile.into()).await?;
    }

    report.applied = true;
    Ok(report)
}

pub fn inspect_bridge_snapshot(snapshot: &BridgeSnapshot) -> MigrationReport {
    let model = model::Model::from(snapshot.current_model.clone());
    let model_exists = snapshot
        .models
        .iter()
        .any(|spec| spec.provider == model.provider.id && spec.model == model.id);
    let task_ids = snapshot
        .tasks
        .iter()
        .map(|task| task.id.as_str())
        .collect::<BTreeSet<_>>();
    let session_ids = snapshot
        .sessions
        .iter()
        .map(|session| session.id.as_str())
        .collect::<BTreeSet<_>>();

    let mut issues = Vec::new();
    record_duplicate_ids(
        snapshot
            .models
            .iter()
            .map(|spec| format!("{}:{}", spec.provider, spec.model)),
        "model",
        &mut issues,
    );
    record_duplicate_ids(
        snapshot.skills.iter().map(|skill| skill.id.clone()),
        "skill",
        &mut issues,
    );
    record_duplicate_ids(
        snapshot.sessions.iter().map(|session| session.id.clone()),
        "session",
        &mut issues,
    );
    record_duplicate_ids(
        snapshot.tasks.iter().map(|task| task.id.clone()),
        "task",
        &mut issues,
    );
    record_duplicate_ids(
        snapshot.runs.iter().map(|run| run.id.clone()),
        "run",
        &mut issues,
    );
    record_duplicate_ids(
        snapshot
            .profiles
            .iter()
            .map(|profile| profile.provider.clone()),
        "profile",
        &mut issues,
    );

    if !model_exists {
        issues.push(MigrationIssue::new(
            MigrationIssueKind::CurrentModelMissingFromCatalog,
            format!(
                "current model is not listed in the model catalog: {}:{}",
                model.provider.id, model.id
            ),
        ));
    }

    for run in &snapshot.runs {
        if !task_ids.contains(run.task_id.as_str()) {
            issues.push(MigrationIssue::new(
                MigrationIssueKind::RunReferencesMissingTask,
                format!(
                    "run references a task that is not present in the snapshot: {} -> {}",
                    run.id, run.task_id
                ),
            ));
        }
        if let Some(session_id) = &run.session_id
            && !session_ids.contains(session_id.as_str())
        {
            issues.push(MigrationIssue::new(
                MigrationIssueKind::RunReferencesMissingSession,
                format!(
                    "run references a session that is not present in the snapshot: {} -> {}",
                    run.id, session_id
                ),
            ));
        }
    }

    if !snapshot.observed_tool_specs.is_empty() {
        issues.push(MigrationIssue::new(
            MigrationIssueKind::ToolSpecsAreInformational,
            "tool specs were recorded for reporting but concrete tool implementations are not imported",
        ));
    }

    MigrationReport {
        applied: false,
        models: snapshot.models.len(),
        skills: snapshot.skills.len(),
        sessions: snapshot.sessions.len(),
        messages: snapshot
            .sessions
            .iter()
            .map(|session| session.messages.len())
            .sum(),
        tasks: snapshot.tasks.len(),
        runs: snapshot.runs.len(),
        profiles: snapshot.profiles.len(),
        tool_specs: snapshot.observed_tool_specs.len(),
        issues,
    }
}

async fn existing_record_issues(
    core: &Core,
    snapshot: &BridgeSnapshot,
) -> Result<Vec<MigrationIssue>> {
    let mut issues = Vec::new();

    for spec in &snapshot.models {
        if core.models.get(&spec.provider, &spec.model).is_some() {
            issues.push(existing_record(
                "model",
                format!("{}:{}", spec.provider, spec.model),
            ));
        }
    }
    for skill in &snapshot.skills {
        if core.skills.exists(&skill.id).await? {
            issues.push(existing_record("skill", skill.id.clone()));
        }
    }
    for session in &snapshot.sessions {
        if core.sessions.exists(&session.id).await? {
            issues.push(existing_record("session", session.id.clone()));
        }
    }
    for task in &snapshot.tasks {
        if core.tasks.exists(&task.id).await? {
            issues.push(existing_record("task", task.id.clone()));
        }
    }
    for run in &snapshot.runs {
        if core.runs.exists(&run.id).await? {
            issues.push(existing_record("run", run.id.clone()));
        }
    }
    for profile in &snapshot.profiles {
        if core.profiles.exists(&profile.provider).await? {
            issues.push(existing_record("profile", profile.provider.clone()));
        }
    }

    Ok(issues)
}

fn existing_record(domain: &str, id: String) -> MigrationIssue {
    MigrationIssue::new(
        MigrationIssueKind::ExistingRecord,
        format!("{domain} already exists and create-only import will not overwrite it: {id}"),
    )
}

fn record_duplicate_ids(
    ids: impl IntoIterator<Item = String>,
    domain: &str,
    issues: &mut Vec<MigrationIssue>,
) {
    let mut seen = BTreeSet::new();
    let mut reported = BTreeSet::new();
    for id in ids {
        if !seen.insert(id.clone()) && reported.insert(id.clone()) {
            issues.push(MigrationIssue::new(
                MigrationIssueKind::DuplicateId,
                format!("snapshot contains duplicate {domain} id: {id}"),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::{
        BridgeMessage, BridgeModelRef, BridgeModelSpec, BridgeProfile, BridgeRole, BridgeRun,
        BridgeSession, BridgeSkill, BridgeSkillSource, BridgeSnapshot, BridgeStatus, BridgeTask,
        BridgeToolSpec,
    };
    use crate::skill;
    use store::Repository;

    #[test]
    fn bridge_snapshot_import_populates_core_and_reports_counts() {
        block_on_once(async {
            let (core, report) = core_from_bridge_snapshot(sample_snapshot()).await.unwrap();
            let snapshot = core.snapshot().await.unwrap();

            assert_eq!(report.models, 1);
            assert_eq!(report.skills, 1);
            assert_eq!(report.sessions, 1);
            assert_eq!(report.messages, 1);
            assert_eq!(report.tasks, 1);
            assert_eq!(report.runs, 1);
            assert_eq!(report.profiles, 1);
            assert_eq!(report.tool_specs, 0);
            assert!(report.applied);
            assert!(report.is_clean());
            assert_eq!(snapshot.current_model.id, "gpt-5.5");
            assert_eq!(snapshot.sessions[0].messages[0].text, "hello");
            assert_eq!(snapshot.sessions[0].source.as_deref(), Some("workspace"));
            assert_eq!(snapshot.skills[0].source, skill::Source::System);
            assert_eq!(
                snapshot.skills[0].source_ref.as_deref(),
                Some("system://team")
            );
        });
    }

    #[test]
    fn bridge_snapshot_import_updates_existing_core() {
        block_on_once(async {
            let mut core = Core::new(model::Model::new("openai", "gpt-5.4"));

            let report = import_bridge_snapshot(&mut core, sample_snapshot())
                .await
                .unwrap();

            assert!(report.applied);
            assert!(report.is_clean());
            assert_eq!(core.agent.model.id, "gpt-5.5");
            assert!(core.models.get("openai", "gpt-5.5").is_some());
            assert!(core.skills.exists("team").await.unwrap());
            assert!(core.sessions.exists("session-1").await.unwrap());
            assert!(core.tasks.exists("task-1").await.unwrap());
            assert!(core.runs.exists("run-1").await.unwrap());
            assert!(core.profiles.exists("openai").await.unwrap());
        });
    }

    #[test]
    fn bridge_snapshot_import_reports_lossy_and_inconsistent_data() {
        block_on_once(async {
            let mut snapshot = sample_snapshot();
            snapshot.models.clear();
            snapshot.runs[0].task_id = "missing-task".to_string();
            snapshot.runs[0].session_id = Some("missing-session".to_string());
            snapshot.observed_tool_specs.push(BridgeToolSpec {
                name: "echo".to_string(),
                description: None,
                input_schema: serde_json::json!({ "type": "object" }),
            });

            let (core, report) = core_from_bridge_snapshot(snapshot).await.unwrap();

            assert_eq!(report.issues.len(), 4);
            assert!(!report.applied);
            assert!(report.has_blocking_issues());
            assert!(core.sessions.get("session-1").await.unwrap().is_none());
            assert!(
                report.issues.iter().any(|issue| {
                    issue.kind == MigrationIssueKind::CurrentModelMissingFromCatalog
                })
            );
            assert!(
                report
                    .issues
                    .iter()
                    .any(|issue| { issue.kind == MigrationIssueKind::RunReferencesMissingTask })
            );
            assert!(
                report
                    .issues
                    .iter()
                    .any(|issue| { issue.kind == MigrationIssueKind::RunReferencesMissingSession })
            );
            assert!(
                report
                    .issues
                    .iter()
                    .any(|issue| { issue.kind == MigrationIssueKind::ToolSpecsAreInformational })
            );
        });
    }

    #[test]
    fn create_only_import_reports_existing_records_without_overwriting() {
        block_on_once(async {
            let mut core = Core::new(model::Model::new("openai", "gpt-5.4"));
            import_bridge_snapshot(&mut core, sample_snapshot())
                .await
                .unwrap();

            let mut next = sample_snapshot();
            next.sessions[0].messages[0].text = "replacement".to_string();
            let report = import_bridge_snapshot(&mut core, next).await.unwrap();

            assert!(!report.applied);
            assert!(report.has_blocking_issues());
            assert!(report.issues.iter().any(|issue| {
                issue.kind == MigrationIssueKind::ExistingRecord
                    && issue.message.contains("session-1")
            }));
            let session = core.sessions.get("session-1").await.unwrap().unwrap();
            assert_eq!(session.messages[0].text, "hello");
        });
    }

    #[test]
    fn replace_import_explicitly_allows_existing_records() {
        block_on_once(async {
            let mut core = Core::new(model::Model::new("openai", "gpt-5.4"));
            import_bridge_snapshot(&mut core, sample_snapshot())
                .await
                .unwrap();

            let mut next = sample_snapshot();
            next.sessions[0].messages[0].text = "replacement".to_string();
            let report = replace_bridge_snapshot(&mut core, next).await.unwrap();

            assert!(report.applied);
            assert!(report.is_clean());
            let session = core.sessions.get("session-1").await.unwrap().unwrap();
            assert_eq!(session.messages[0].text, "replacement");
        });
    }

    #[test]
    fn import_reports_duplicate_ids_and_skips_apply() {
        block_on_once(async {
            let mut snapshot = sample_snapshot();
            snapshot.sessions.push(snapshot.sessions[0].clone());

            let (core, report) = core_from_bridge_snapshot(snapshot).await.unwrap();

            assert!(!report.applied);
            assert!(report.has_blocking_issues());
            assert!(report.issues.iter().any(|issue| {
                issue.kind == MigrationIssueKind::DuplicateId && issue.message.contains("session-1")
            }));
            assert!(core.sessions.get("session-1").await.unwrap().is_none());
        });
    }

    fn sample_snapshot() -> BridgeSnapshot {
        BridgeSnapshot {
            current_model: BridgeModelRef::new("openai", "gpt-5.5"),
            models: vec![BridgeModelSpec {
                provider: "openai".to_string(),
                model: "gpt-5.5".to_string(),
                name: "GPT-5.5".to_string(),
                description: None,
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
            observed_tool_specs: Vec::new(),
        }
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
}
