//! # codocia
//!
//! Agent owns execution planning and model/tool orchestration.
//!
//! ## Owns
//! - Agent
//! - execution input and output
//! - tool registry consumption
//! - event production
//!
//! ## Must Not
//! - own daemon lifecycle
//! - write durable storage directly
//! - render UI
//! - parse UI picker state
//!
//! ## Inputs
//! - Model
//! - allowed tools
//! - user message
//! - SkillContext
//!
//! ## Outputs
//! - Event stream
//! - final run output
//!
//! ## Depends On
//! - event
//! - model
//! - skill
//! - tool
//!
//! ## Used By
//! - chat
//! - run
//!
//! ## Verify
//! - cargo check -p agent

use event::Event;
use model::Model;
use serde::{Deserialize, Serialize};
use skill::{Skill, SkillContext, SkillIssue, SkillSummary, Source};
use tool::Registry;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub model: Model,
    pub skills: Vec<String>,
}

impl Agent {
    pub fn new(model: Model) -> Self {
        Self {
            model,
            skills: Vec::new(),
        }
    }

    pub fn with_skills(mut self, skills: impl IntoIterator<Item = String>) -> Self {
        self.skills = skills.into_iter().collect();
        self
    }
}

#[derive(Debug, Clone)]
pub struct RunInput {
    pub message: String,
    pub skill_context: SkillContext,
}

impl RunInput {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            skill_context: SkillContext::default(),
        }
    }

    pub fn with_skill_context(mut self, skill_context: SkillContext) -> Self {
        self.skill_context = skill_context;
        self
    }
}

#[derive(Debug, Clone)]
pub struct RunOutput {
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub blocks: Vec<PromptBlock>,
    pub user_message: String,
}

impl Prompt {
    pub fn render(&self) -> String {
        let mut output = String::new();
        for block in &self.blocks {
            if !output.is_empty() {
                output.push_str("\n\n");
            }
            output.push_str("## ");
            output.push_str(&block.title);
            output.push('\n');
            output.push_str(&block.body);
        }

        if !output.is_empty() {
            output.push_str("\n\n");
        }
        output.push_str("## User message\n");
        output.push_str(&self.user_message);
        output
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptBlock {
    pub title: String,
    pub body: String,
}

pub struct Exec {
    pub agent: Agent,
    pub tools: Registry,
}

impl Exec {
    pub fn new(agent: Agent, tools: Registry) -> Self {
        Self { agent, tools }
    }

    pub fn build_prompt(&self, input: &RunInput) -> Prompt {
        Prompt {
            blocks: skill_blocks(&input.skill_context),
            user_message: input.message.clone(),
        }
    }

    pub fn dry_run(&self, input: RunInput) -> RunOutput {
        RunOutput {
            events: vec![Event::Text {
                value: self.build_prompt(&input).render(),
            }],
        }
    }
}

fn skill_blocks(context: &SkillContext) -> Vec<PromptBlock> {
    let mut blocks = Vec::new();

    if !context.assigned.is_empty() {
        blocks.push(PromptBlock {
            title: "Assigned skills".to_string(),
            body: context
                .assigned
                .iter()
                .map(render_skill_summary)
                .collect::<Vec<_>>()
                .join("\n\n"),
        });
    }

    for skill in &context.mentioned {
        blocks.push(PromptBlock {
            title: format!("Mentioned skill: @{}", skill.id),
            body: render_mentioned_skill(skill),
        });
    }

    if !context.issues.is_empty() {
        blocks.push(PromptBlock {
            title: "Skill context issues".to_string(),
            body: context
                .issues
                .iter()
                .map(render_skill_issue)
                .collect::<Vec<_>>()
                .join("\n"),
        });
    }

    blocks
}

fn render_skill_summary(skill: &SkillSummary) -> String {
    let mut lines = vec![format!(
        "- @{}: {} [{}]",
        skill.id,
        skill.name,
        source_label(&skill.source)
    )];
    if let Some(description) = &skill.description {
        lines.push(format!("  Description: {description}"));
    }
    if !skill.suggested_tools.is_empty() {
        lines.push(format!(
            "  Suggested tools: {}",
            skill.suggested_tools.join(", ")
        ));
    }
    lines.join("\n")
}

fn render_mentioned_skill(skill: &Skill) -> String {
    let mut lines = vec![format!(
        "@{}: {} [{}]",
        skill.id,
        skill.name,
        source_label(&skill.source)
    )];
    if let Some(description) = &skill.description {
        lines.push(format!("Description: {description}"));
    }
    if !skill.suggested_tools.is_empty() {
        lines.push(format!(
            "Suggested tools: {}",
            skill.suggested_tools.join(", ")
        ));
    }
    if !skill.content.trim().is_empty() {
        lines.push("Instructions:".to_string());
        lines.push(skill.content.clone());
    }
    lines.join("\n")
}

fn render_skill_issue(issue: &SkillIssue) -> String {
    format!("- @{}: {}", issue.skill_id, issue.message)
}

fn source_label(source: &Source) -> &'static str {
    match source {
        Source::System => "system",
        Source::User => "user",
        Source::External => "external",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exec() -> Exec {
        Exec::new(Agent::new(Model::new("openai", "gpt-5.5")), Registry::new())
    }

    #[test]
    fn assigned_skills_render_as_summaries() {
        let context = SkillContext {
            assigned: vec![SkillSummary {
                id: "team".to_string(),
                name: "Team".to_string(),
                source: Source::System,
                source_ref: Some("system://team".to_string()),
                description: Some("Coordinate subagents.".to_string()),
                suggested_tools: vec!["spawn_subagent_batch".to_string()],
            }],
            mentioned: Vec::new(),
            issues: Vec::new(),
        };

        let prompt = exec().build_prompt(&RunInput::new("review this").with_skill_context(context));
        let rendered = prompt.render();

        assert!(rendered.contains("## Assigned skills"));
        assert!(rendered.contains("@team: Team [system]"));
        assert!(rendered.contains("Suggested tools: spawn_subagent_batch"));
        assert!(!rendered.contains("Instructions:"));
    }

    #[test]
    fn mentioned_skills_render_full_content() {
        let context = SkillContext {
            assigned: Vec::new(),
            mentioned: vec![
                Skill::new("team", "Team", Source::System)
                    .with_description("Coordinate subagents.")
                    .with_content("Use parallel workers for independent tasks.")
                    .with_tools(["spawn_subagent_batch"]),
            ],
            issues: Vec::new(),
        };

        let prompt = exec().build_prompt(&RunInput::new("use @team").with_skill_context(context));
        let rendered = prompt.render();

        assert!(rendered.contains("## Mentioned skill: @team"));
        assert!(rendered.contains("Instructions:"));
        assert!(rendered.contains("Use parallel workers for independent tasks."));
        assert!(rendered.contains("## User message\nuse @team"));
    }

    #[test]
    fn skill_context_issues_are_visible_to_agent() {
        let context = SkillContext {
            assigned: Vec::new(),
            mentioned: Vec::new(),
            issues: vec![skill::SkillIssue::unknown_mention("missing")],
        };

        let prompt =
            exec().build_prompt(&RunInput::new("use @missing").with_skill_context(context));
        let rendered = prompt.render();

        assert!(rendered.contains("## Skill context issues"));
        assert!(rendered.contains("- @missing: unknown mentioned skill: missing"));
    }
}
