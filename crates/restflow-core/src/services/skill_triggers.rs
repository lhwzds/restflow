use crate::models::{Skill, SkillStatus};

/// Match result for a skill trigger phrase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggerMatch {
    pub skill_id: String,
    pub skill_name: String,
    pub matched_trigger: String,
    pub confidence: TriggerConfidence,
}

/// Confidence score for a trigger match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TriggerConfidence {
    Exact,
}

/// Find active skills whose trigger phrases appear in the user message.
pub fn match_triggers(message: &str, skills: &[Skill]) -> Vec<TriggerMatch> {
    let normalized_message = message.to_lowercase();
    let mut matches = Vec::new();

    for skill in skills {
        if skill.status != SkillStatus::Active {
            continue;
        }

        for trigger in &skill.triggers {
            let normalized_trigger = trigger.trim().to_lowercase();
            if normalized_trigger.is_empty() {
                continue;
            }

            if normalized_message.contains(&normalized_trigger) {
                matches.push(TriggerMatch {
                    skill_id: skill.id.clone(),
                    skill_name: skill.name.clone(),
                    matched_trigger: trigger.clone(),
                    confidence: TriggerConfidence::Exact,
                });
                break;
            }
        }
    }

    matches.sort_by(|left, right| right.confidence.cmp(&left.confidence));
    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_skill(id: &str, name: &str, triggers: Vec<&str>) -> Skill {
        let mut skill = Skill::new(
            id.to_string(),
            name.to_string(),
            Some(format!("{} description", name)),
            None,
            format!("# {}\n", name),
        );
        skill.triggers = triggers.into_iter().map(|item| item.to_string()).collect();
        skill
    }

    #[test]
    fn test_trigger_exact_match() {
        let skills = vec![build_skill(
            "code-reviewer",
            "Code Reviewer",
            vec!["code review", "review PR"],
        )];

        let matches = match_triggers("please review PR #123", &skills);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].skill_id, "code-reviewer");
        assert_eq!(matches[0].confidence, TriggerConfidence::Exact);
    }

    #[test]
    fn test_trigger_case_insensitive() {
        let skills = vec![build_skill(
            "code-reviewer",
            "Code Reviewer",
            vec!["Code Review"],
        )];

        let matches = match_triggers("do a code review on this patch", &skills);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_trigger_no_match() {
        let skills = vec![build_skill("deployer", "Deployer", vec!["deploy release"])];

        let matches = match_triggers("fix the bug in parser", &skills);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_trigger_ignores_non_active_skills() {
        let mut archived = build_skill("archived", "Archived", vec!["code review"]);
        archived.status = SkillStatus::Archived;

        let matches = match_triggers("code review this", &[archived]);
        assert!(matches.is_empty());
    }
}
