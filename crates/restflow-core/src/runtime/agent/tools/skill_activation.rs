//! Skill-aware tool allowlist activation helpers.

use crate::models::{AgentNode, Skill};
use crate::services::skill_mentions::parse_skill_mentions;
use anyhow::{Result, bail};
use std::collections::{HashMap, HashSet};

use super::effective_main_agent_tool_names;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillActivationPolicy {
    Strict,
    IgnoreInvalid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillActivationResult {
    pub tool_names: Vec<String>,
    pub activated_skill_ids: Vec<String>,
    pub issues: Vec<SkillActivationIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillActivationIssue {
    pub category: SkillActivationIssueCategory,
    pub skill_id: String,
    pub message: String,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillActivationIssueCategory {
    MissingSkill,
    UnauthorizedSkill,
}

impl SkillActivationIssueCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MissingSkill => "missing_skill",
            Self::UnauthorizedSkill => "unauthorized_skill",
        }
    }
}

pub fn effective_tool_allowlist_for_turn(
    agent_node: &AgentNode,
    user_input: Option<&str>,
    skill_catalog: &[Skill],
    policy: SkillActivationPolicy,
) -> Result<SkillActivationResult> {
    let base_tools = effective_main_agent_tool_names(agent_node.tools.as_deref());
    resolve_skill_activated_tool_allowlist(
        &base_tools,
        agent_node.skills.as_deref(),
        user_input,
        skill_catalog,
        policy,
    )
}

pub fn resolve_skill_activated_tool_allowlist(
    base_tool_names: &[String],
    assigned_skill_ids: Option<&[String]>,
    user_input: Option<&str>,
    skill_catalog: &[Skill],
    policy: SkillActivationPolicy,
) -> Result<SkillActivationResult> {
    let mut result = SkillActivationResult {
        tool_names: dedupe_strings(base_tool_names.iter().cloned()),
        activated_skill_ids: Vec::new(),
        issues: Vec::new(),
    };
    let mut tool_set: HashSet<String> = result.tool_names.iter().cloned().collect();
    let mut activated_set = HashSet::new();
    let skill_by_id = build_effective_skill_index(skill_catalog);
    let assigned_ids = assigned_skill_ids.unwrap_or_default();

    for skill_id in assigned_ids {
        activate_assigned_skill(
            skill_id,
            &skill_by_id,
            &mut result,
            &mut tool_set,
            &mut activated_set,
        );
    }

    let mentioned_ids = user_input.map(parse_skill_mentions).unwrap_or_default();
    for skill_id in mentioned_ids {
        activate_mentioned_skill(
            &skill_id,
            &skill_by_id,
            &mut result,
            &mut tool_set,
            &mut activated_set,
        );
    }

    if policy == SkillActivationPolicy::Strict && !result.issues.is_empty() {
        bail!(format_skill_activation_issues(&result.issues));
    }

    Ok(result)
}

fn activate_assigned_skill(
    skill_id: &str,
    skill_by_id: &HashMap<&str, &Skill>,
    result: &mut SkillActivationResult,
    tool_set: &mut HashSet<String>,
    activated_set: &mut HashSet<String>,
) {
    let Some(skill) = skill_by_id.get(skill_id).copied() else {
        result.issues.push(SkillActivationIssue {
            category: SkillActivationIssueCategory::MissingSkill,
            skill_id: skill_id.to_string(),
            message: format!("Assigned skill '{}' was not found", skill_id),
            suggestion: Some("Remove the skill from agent.skills or install it".to_string()),
        });
        return;
    };

    add_skill_suggested_tools(skill, result, tool_set, activated_set);
}

fn activate_mentioned_skill(
    skill_id: &str,
    skill_by_id: &HashMap<&str, &Skill>,
    result: &mut SkillActivationResult,
    tool_set: &mut HashSet<String>,
    activated_set: &mut HashSet<String>,
) {
    add_tool("load_skill", result, tool_set);

    let Some(skill) = skill_by_id.get(skill_id).copied() else {
        result.issues.push(SkillActivationIssue {
            category: SkillActivationIssueCategory::MissingSkill,
            skill_id: skill_id.to_string(),
            message: format!("Mentioned skill '{}' was not found", skill_id),
            suggestion: Some("Install the skill or remove the mention".to_string()),
        });
        return;
    };

    add_skill_suggested_tools(skill, result, tool_set, activated_set);
}

fn add_skill_suggested_tools(
    skill: &Skill,
    result: &mut SkillActivationResult,
    tool_set: &mut HashSet<String>,
    activated_set: &mut HashSet<String>,
) {
    if activated_set.insert(skill.id.clone()) {
        result.activated_skill_ids.push(skill.id.clone());
    }
    for tool_name in &skill.suggested_tools {
        add_tool(tool_name, result, tool_set);
    }
}

fn add_tool(tool_name: &str, result: &mut SkillActivationResult, tool_set: &mut HashSet<String>) {
    if tool_set.insert(tool_name.to_string()) {
        result.tool_names.push(tool_name.to_string());
    }
}

fn build_effective_skill_index(skills: &[Skill]) -> HashMap<&str, &Skill> {
    let mut skill_by_id = HashMap::new();
    for skill in skills {
        skill_by_id.entry(skill.id.as_str()).or_insert(skill);
    }
    skill_by_id
}

fn dedupe_strings(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for value in values {
        if seen.insert(value.clone()) {
            deduped.push(value);
        }
    }
    deduped
}

fn format_skill_activation_issues(issues: &[SkillActivationIssue]) -> String {
    let messages = issues
        .iter()
        .map(|issue| issue.message.as_str())
        .collect::<Vec<_>>()
        .join("; ");
    format!("Skill activation failed: {}", messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::SkillSource;

    fn skill(id: &str, suggested_tools: &[&str]) -> Skill {
        let mut skill = Skill::new(
            id.to_string(),
            id.to_string(),
            None,
            None,
            "content".to_string(),
        );
        skill.suggested_tools = suggested_tools
            .iter()
            .map(|tool| (*tool).to_string())
            .collect();
        skill
    }

    #[test]
    fn assigned_skill_adds_suggested_tools() {
        let base_tools = vec!["bash".to_string()];
        let assigned = vec!["review".to_string()];
        let catalog = vec![skill("review", &["grep", "file"])];

        let result = resolve_skill_activated_tool_allowlist(
            &base_tools,
            Some(&assigned),
            None,
            &catalog,
            SkillActivationPolicy::Strict,
        )
        .expect("activation should succeed");

        assert_eq!(
            result.tool_names,
            vec!["bash".to_string(), "grep".to_string(), "file".to_string()]
        );
        assert_eq!(result.activated_skill_ids, vec!["review".to_string()]);
        assert!(result.issues.is_empty());
    }

    #[test]
    fn explicit_mention_adds_load_skill_and_suggested_tools() {
        let base_tools = vec!["bash".to_string()];
        let assigned = vec!["team".to_string()];
        let catalog = vec![skill("team", &["spawn_subagent_batch"])];

        let result = resolve_skill_activated_tool_allowlist(
            &base_tools,
            Some(&assigned),
            Some("Use @team for this task"),
            &catalog,
            SkillActivationPolicy::Strict,
        )
        .expect("activation should succeed");

        assert!(result.tool_names.contains(&"load_skill".to_string()));
        assert!(
            result
                .tool_names
                .contains(&"spawn_subagent_batch".to_string())
        );
        assert_eq!(result.activated_skill_ids, vec!["team".to_string()]);
    }

    #[test]
    fn known_mention_activates_suggested_tools_without_assignment() {
        let base_tools = vec!["bash".to_string()];
        let assigned = vec!["regular".to_string()];
        let catalog = vec![skill("admin", &["manage_secrets"]), skill("regular", &[])];

        let result = resolve_skill_activated_tool_allowlist(
            &base_tools,
            Some(&assigned),
            Some("@admin rotate credentials"),
            &catalog,
            SkillActivationPolicy::IgnoreInvalid,
        )
        .expect("known mention should activate");

        assert!(result.tool_names.contains(&"load_skill".to_string()));
        assert!(result.tool_names.contains(&"manage_secrets".to_string()));
        assert!(result.issues.is_empty());
    }

    #[test]
    fn missing_mention_reports_issue_without_adding_tools() {
        let base_tools = vec!["bash".to_string()];
        let catalog = vec![skill("regular", &[])];

        let result = resolve_skill_activated_tool_allowlist(
            &base_tools,
            None,
            Some("@missing do work"),
            &catalog,
            SkillActivationPolicy::IgnoreInvalid,
        )
        .expect("ignore invalid policy should return issues");

        assert!(result.tool_names.contains(&"load_skill".to_string()));
        assert!(!result.tool_names.contains(&"manage_secrets".to_string()));
        assert_eq!(result.issues.len(), 1);
        assert_eq!(
            result.issues[0].category,
            SkillActivationIssueCategory::MissingSkill
        );
    }

    #[test]
    fn strict_policy_rejects_missing_assigned_skill() {
        let base_tools = vec!["bash".to_string()];
        let assigned = vec!["missing".to_string()];

        let error = resolve_skill_activated_tool_allowlist(
            &base_tools,
            Some(&assigned),
            None,
            &[],
            SkillActivationPolicy::Strict,
        )
        .expect_err("strict policy should fail");

        assert!(
            error
                .to_string()
                .contains("Assigned skill 'missing' was not found")
        );
    }

    #[test]
    fn first_catalog_entry_wins_for_shadowed_systemskill() {
        let mut system_skill = skill("team", &["spawn_subagent_batch"]);
        system_skill.source = SkillSource::System;
        let mut storage_skill = skill("team", &["manage_secrets"]);
        storage_skill.source = SkillSource::User;
        let base_tools = vec!["bash".to_string()];
        let assigned = vec!["team".to_string()];

        let result = resolve_skill_activated_tool_allowlist(
            &base_tools,
            Some(&assigned),
            None,
            &[system_skill, storage_skill],
            SkillActivationPolicy::Strict,
        )
        .expect("system skill should win");

        assert!(
            result
                .tool_names
                .contains(&"spawn_subagent_batch".to_string())
        );
        assert!(!result.tool_names.contains(&"manage_secrets".to_string()));
    }

    #[test]
    fn agent_wrapper_uses_effective_main_agent_tools() {
        let agent = AgentNode::new().with_skills(vec!["review".to_string()]);
        let catalog = vec![skill("review", &["grep"])];

        let result = effective_tool_allowlist_for_turn(
            &agent,
            None,
            &catalog,
            SkillActivationPolicy::Strict,
        )
        .expect("activation should succeed");

        assert!(result.tool_names.contains(&"bash".to_string()));
        assert!(result.tool_names.contains(&"grep".to_string()));
    }
}
