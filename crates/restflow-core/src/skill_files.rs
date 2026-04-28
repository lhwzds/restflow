//! Systemskill assets bundled with RestFlow.

use anyhow::{Context, Result};
use restflow_traits::skill::SkillSource;

use crate::models::{Skill, StorageMode};

const SYSTEMSKILLS: &[(&str, &str)] = &[
    ("team", include_str!("../assets/skills/team/SKILL.md")),
    (
        "manage-subagent",
        include_str!("../assets/skills/manage-subagent/SKILL.md"),
    ),
    (
        "manage-background-agent",
        include_str!("../assets/skills/manage-background-agent/SKILL.md"),
    ),
    (
        "manage-agent",
        include_str!("../assets/skills/manage-agent/SKILL.md"),
    ),
    (
        "manage-chat-session",
        include_str!("../assets/skills/manage-chat-session/SKILL.md"),
    ),
    (
        "self-heal-ops",
        include_str!("../assets/skills/self-heal-ops/SKILL.md"),
    ),
    (
        "structured-planner",
        include_str!("../assets/skills/structured-planner/SKILL.md"),
    ),
    (
        "address-pr-feedback",
        include_str!("../assets/skills/address-pr-feedback/SKILL.md"),
    ),
    (
        "pr-context-gatherer",
        include_str!("../assets/skills/pr-context-gatherer/SKILL.md"),
    ),
];

pub fn list_systemskills() -> Result<Vec<Skill>> {
    SYSTEMSKILLS
        .iter()
        .map(|(id, content)| parse_systemskill(id, content))
        .collect()
}

pub fn get_systemskill(id: &str) -> Result<Option<Skill>> {
    SYSTEMSKILLS
        .iter()
        .find(|(skill_id, _)| *skill_id == id)
        .map(|(skill_id, content)| parse_systemskill(skill_id, content))
        .transpose()
}

pub fn systemskill_ids() -> impl Iterator<Item = &'static str> {
    SYSTEMSKILLS.iter().map(|(id, _)| *id)
}

fn parse_systemskill(id: &str, content: &str) -> Result<Skill> {
    let mut skill = Skill::from_markdown(id, content)
        .with_context(|| format!("Failed to parse systemskill '{id}'"))?;
    skill.storage_mode = StorageMode::FileSystemOnly;
    skill.is_synced = true;
    skill.source = SkillSource::System;
    skill.read_only = true;
    skill.source_ref = Some(format!("restflow://system/{id}"));
    Ok(skill)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn systemskill_content_is_valid_frontmatter() {
        for (skill_id, content) in SYSTEMSKILLS {
            assert!(
                content.starts_with("---"),
                "systemskill {} missing frontmatter",
                skill_id
            );
            assert!(
                content.contains("name:"),
                "systemskill {} missing name",
                skill_id
            );
        }
    }

    #[test]
    fn loads_team_as_read_only_systemskill() {
        let skill = get_systemskill("team").unwrap().expect("team should exist");
        assert_eq!(skill.id, "team");
        assert_eq!(skill.source, SkillSource::System);
        assert!(skill.read_only);
        assert!(skill.content.contains("spawn_subagent_batch"));
    }

    #[test]
    fn self_heal_ops_declares_manage_ops_activation() {
        let skill = get_systemskill("self-heal-ops")
            .unwrap()
            .expect("self-heal-ops should exist");
        assert_eq!(skill.source, SkillSource::System);
        assert!(skill.read_only);
        assert!(
            skill
                .suggested_tools
                .iter()
                .any(|tool| tool == "manage_ops")
        );
    }

    #[test]
    fn lists_stable_systemskill_ids() {
        let ids = systemskill_ids().collect::<Vec<_>>();
        assert!(ids.contains(&"team"));
        assert!(ids.contains(&"manage-subagent"));
    }
}
