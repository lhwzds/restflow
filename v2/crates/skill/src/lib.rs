//! # codocia
//!
//! Skill owns skill metadata and AI-facing context resolution.
//!
//! ## Owns
//! - skill catalog
//! - skill repository loading
//! - skill source metadata
//! - skill source references
//! - @skill mention parsing
//! - SkillContext resolution
//! - suggested tool metadata
//!
//! ## Must Not
//! - render UI overlays
//! - write session history
//! - decide tool permissions
//! - execute tools directly
//! - own durable Task or Run state
//!
//! ## Inputs
//! - user message text
//! - assigned skill IDs
//! - skill catalog
//! - skill repository
//!
//! ## Outputs
//! - SkillContext
//! - assigned skill summaries
//! - mentioned skill content
//! - context issues
//!
//! ## Depends On
//! - store
//!
//! ## Used By
//! - chat
//! - run
//!
//! ## Verify
//! - cargo check -p skill

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use store::Repository;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    System,
    User,
    External,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub source: Source,
    pub source_ref: Option<String>,
    pub read_only: bool,
    pub description: Option<String>,
    pub content: String,
    pub suggested_tools: Vec<String>,
}

impl Skill {
    pub fn new(id: impl Into<String>, name: impl Into<String>, source: Source) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            source,
            source_ref: None,
            read_only: false,
            description: None,
            content: String::new(),
            suggested_tools: Vec::new(),
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_content(mut self, content: impl Into<String>) -> Self {
        self.content = content.into();
        self
    }

    pub fn with_source_ref(mut self, source_ref: impl Into<String>) -> Self {
        self.source_ref = Some(source_ref.into());
        self
    }

    pub fn with_tools(mut self, tools: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.suggested_tools = tools.into_iter().map(Into::into).collect();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillSummary {
    pub id: String,
    pub name: String,
    pub source: Source,
    pub source_ref: Option<String>,
    pub description: Option<String>,
    pub suggested_tools: Vec<String>,
}

impl From<&Skill> for SkillSummary {
    fn from(skill: &Skill) -> Self {
        Self {
            id: skill.id.clone(),
            name: skill.name.clone(),
            source: skill.source.clone(),
            source_ref: skill.source_ref.clone(),
            description: skill.description.clone(),
            suggested_tools: skill.suggested_tools.clone(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    skills: BTreeMap<String, Skill>,
}

impl Catalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_skills(skills: impl IntoIterator<Item = Skill>) -> Self {
        let mut catalog = Self::new();
        for skill in skills {
            catalog.insert(skill);
        }
        catalog
    }

    pub async fn from_repository<R>(repository: &R) -> Result<Self>
    where
        R: Repository<Skill> + ?Sized,
    {
        Ok(Self::from_skills(repository.list().await?))
    }

    pub async fn merge_repository<R>(&mut self, repository: &R) -> Result<()>
    where
        R: Repository<Skill> + ?Sized,
    {
        for skill in repository.list().await? {
            self.insert(skill);
        }
        Ok(())
    }

    pub fn insert(&mut self, skill: Skill) {
        let id = skill.id.clone();
        if self.skills.contains_key(&id) {
            return;
        }
        self.skills.insert(id, skill);
    }

    pub fn get(&self, id: &str) -> Option<&Skill> {
        self.skills.get(id)
    }

    pub fn list(&self) -> Vec<&Skill> {
        self.skills.values().collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillContext {
    pub assigned: Vec<SkillSummary>,
    pub mentioned: Vec<Skill>,
    pub issues: Vec<SkillIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillIssue {
    pub kind: SkillIssueKind,
    pub skill_id: String,
    pub message: String,
}

impl SkillIssue {
    pub fn unknown_mention(skill_id: impl Into<String>) -> Self {
        let skill_id = skill_id.into();
        Self {
            kind: SkillIssueKind::UnknownMention,
            message: format!("unknown mentioned skill: {skill_id}"),
            skill_id,
        }
    }

    pub fn missing_assigned(skill_id: impl Into<String>) -> Self {
        let skill_id = skill_id.into();
        Self {
            kind: SkillIssueKind::MissingAssignedSkill,
            message: format!("assigned skill is missing: {skill_id}"),
            skill_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillIssueKind {
    UnknownMention,
    MissingAssignedSkill,
}

pub fn mentions(input: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut seen = BTreeSet::new();
    for token in input.split_whitespace() {
        let Some(id) = token.strip_prefix('@') else {
            continue;
        };
        let id = sanitize_mention(id);
        if !id.is_empty() && seen.insert(id.clone()) {
            ids.push(id);
        }
    }
    ids
}

pub fn resolve_context(catalog: &Catalog, assigned: &[String], input: &str) -> SkillContext {
    let mut context = SkillContext::default();
    let mut assigned_seen = BTreeSet::new();
    let mut mentioned_seen = BTreeSet::new();

    for id in assigned {
        if !assigned_seen.insert(id.as_str()) {
            continue;
        }
        match catalog.get(id) {
            Some(skill) => context.assigned.push(SkillSummary::from(skill)),
            None => context.issues.push(SkillIssue::missing_assigned(id)),
        }
    }

    for id in mentions(input) {
        if !mentioned_seen.insert(id.clone()) {
            continue;
        }
        match catalog.get(&id) {
            Some(skill) => context.mentioned.push(skill.clone()),
            None => context.issues.push(SkillIssue::unknown_mention(id)),
        }
    }

    context
}

fn sanitize_mention(value: &str) -> String {
    value
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};
    use store::MemoryStore;

    fn catalog() -> Catalog {
        let mut catalog = Catalog::new();
        catalog.insert(
            Skill::new("team", "Team", Source::System)
                .with_description("Coordinate subagents.")
                .with_content("Use this skill when the user asks for parallel work.")
                .with_tools(["spawn_subagent_batch"]),
        );
        catalog.insert(
            Skill::new("review", "Review", Source::User)
                .with_description("Review code.")
                .with_content("Inspect changes and report findings.")
                .with_tools(["grep", "file"]),
        );
        catalog
    }

    #[test]
    fn mentions_are_sanitized_and_deduped() {
        assert_eq!(
            mentions("use @team, then @review and @team again"),
            vec!["team", "review"]
        );
    }

    #[test]
    fn context_includes_assigned_summaries() {
        let context = resolve_context(&catalog(), &["team".to_string()], "hello");

        assert_eq!(context.assigned.len(), 1);
        assert_eq!(context.assigned[0].id, "team");
        assert_eq!(
            context.assigned[0].suggested_tools,
            vec!["spawn_subagent_batch"]
        );
        assert!(context.mentioned.is_empty());
        assert!(context.issues.is_empty());
    }

    #[test]
    fn mentioned_skill_includes_full_content_without_assignment_gate() {
        let context = resolve_context(&catalog(), &[], "please use @team");

        assert!(context.assigned.is_empty());
        assert_eq!(context.mentioned.len(), 1);
        assert_eq!(context.mentioned[0].id, "team");
        assert_eq!(
            context.mentioned[0].content,
            "Use this skill when the user asks for parallel work."
        );
        assert!(context.issues.is_empty());
    }

    #[test]
    fn unknown_mention_reports_issue() {
        let context = resolve_context(&catalog(), &[], "@missing");

        assert!(context.mentioned.is_empty());
        assert_eq!(
            context.issues,
            vec![SkillIssue {
                kind: SkillIssueKind::UnknownMention,
                skill_id: "missing".to_string(),
                message: "unknown mentioned skill: missing".to_string(),
            }]
        );
    }

    #[test]
    fn missing_assigned_skill_reports_issue() {
        let context = resolve_context(&catalog(), &["missing".to_string()], "hello");

        assert!(context.assigned.is_empty());
        assert_eq!(
            context.issues,
            vec![SkillIssue {
                kind: SkillIssueKind::MissingAssignedSkill,
                skill_id: "missing".to_string(),
                message: "assigned skill is missing: missing".to_string(),
            }]
        );
    }

    #[test]
    fn first_catalog_entry_wins() {
        let mut catalog = Catalog::new();
        catalog.insert(Skill::new("team", "System Team", Source::System).with_content("system"));
        catalog.insert(Skill::new("team", "User Team", Source::User).with_content("user"));

        let context = resolve_context(&catalog, &[], "@team");

        assert_eq!(context.mentioned[0].content, "system");
    }

    #[test]
    fn catalog_loads_from_repository() {
        block_on_once(async {
            let store = MemoryStore::new();
            store
                .put(
                    "team",
                    Skill::new("team", "Team", Source::System)
                        .with_description("Coordinate subagents.")
                        .with_content("Use workers for independent tasks."),
                )
                .await
                .unwrap();

            let catalog = Catalog::from_repository(&store).await.unwrap();
            let context = resolve_context(&catalog, &[], "use @team");

            assert_eq!(context.mentioned.len(), 1);
            assert_eq!(context.mentioned[0].id, "team");
            assert!(context.issues.is_empty());
        });
    }

    #[test]
    fn repository_merge_does_not_override_existing_system_skill() {
        block_on_once(async {
            let store = MemoryStore::new();
            store
                .put(
                    "team",
                    Skill::new("team", "User Team", Source::User).with_content("user"),
                )
                .await
                .unwrap();
            let mut catalog =
                Catalog::from_skills([
                    Skill::new("team", "System Team", Source::System).with_content("system")
                ]);

            catalog.merge_repository(&store).await.unwrap();
            let context = resolve_context(&catalog, &[], "@team");

            assert_eq!(context.mentioned[0].source, Source::System);
            assert_eq!(context.mentioned[0].content, "system");
        });
    }

    fn block_on_once<T>(future: impl Future<Output = T>) -> T {
        let waker = Waker::from(Arc::new(NoopWake));
        let mut context = Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);

        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("skill repository future unexpectedly yielded"),
        }
    }

    struct NoopWake;

    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }
}
