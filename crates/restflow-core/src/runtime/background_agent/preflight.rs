//! Preflight checks for background agent execution.

use crate::models::Skill;
use regex::Regex;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightResult {
    pub passed: bool,
    pub blockers: Vec<PreflightIssue>,
    pub warnings: Vec<PreflightIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightIssue {
    pub category: PreflightCategory,
    pub message: String,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreflightCategory {
    MissingTool,
    MissingSecret,
    UnsetVariable,
    MissingPrerequisite,
    InvalidConfig,
}

impl PreflightCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MissingTool => "missing_tool",
            Self::MissingSecret => "missing_secret",
            Self::UnsetVariable => "unset_variable",
            Self::MissingPrerequisite => "missing_prerequisite",
            Self::InvalidConfig => "invalid_config",
        }
    }
}

pub fn run_preflight(
    skills: &[Skill],
    available_tools: &[String],
    skill_variables: Option<&HashMap<String, String>>,
    model_configured: bool,
) -> PreflightResult {
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();

    if !model_configured {
        blockers.push(PreflightIssue {
            category: PreflightCategory::InvalidConfig,
            message: "No model configured for agent".to_string(),
            suggestion: Some(
                "Set model in agent definition or configure provider credentials".into(),
            ),
        });
    }

    let available_tool_set: HashSet<&str> = available_tools.iter().map(String::as_str).collect();
    for skill in skills {
        for tool_name in &skill.suggested_tools {
            if !available_tool_set.contains(tool_name.as_str()) {
                warnings.push(PreflightIssue {
                    category: PreflightCategory::MissingTool,
                    message: format!(
                        "Suggested tool '{}' from skill '{}' is not available",
                        tool_name, skill.id
                    ),
                    suggestion: Some("Check tool allowlist or remove from suggested_tools".into()),
                });
            }
        }
    }

    let variable_map = skill_variables.cloned().unwrap_or_default();
    let variable_regex = Regex::new(r"\{\{([a-zA-Z_][a-zA-Z0-9_]*)\}\}")
        .expect("variable placeholder regex must compile");

    let mut seen_variables: HashSet<String> = HashSet::new();
    for skill in skills {
        for captures in variable_regex.captures_iter(&skill.content) {
            let variable_name = captures[1].to_string();
            if !seen_variables.insert(variable_name.clone()) {
                continue;
            }

            let missing = variable_map
                .get(&variable_name)
                .map(|value| value.trim().is_empty())
                .unwrap_or(true);
            if missing {
                warnings.push(PreflightIssue {
                    category: PreflightCategory::UnsetVariable,
                    message: format!(
                        "Variable '{{{{{}}}}}' is used in skill content but has no value",
                        variable_name
                    ),
                    suggestion: Some("Set value in agent.skill_variables".into()),
                });
            }
        }
    }

    PreflightResult {
        passed: blockers.is_empty(),
        blockers,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill_with(content: &str, suggested_tools: &[&str]) -> Skill {
        let mut skill = Skill::new(
            "skill-1".to_string(),
            "Skill 1".to_string(),
            None,
            None,
            content.to_string(),
        );
        skill.suggested_tools = suggested_tools
            .iter()
            .map(|tool| (*tool).to_string())
            .collect();
        skill
    }

    #[test]
    fn preflight_passes_with_valid_configuration() {
        let skill = skill_with("Use {{project}}", &["bash"]);
        let available_tools = vec!["bash".to_string(), "file".to_string()];
        let vars = HashMap::from([("project".to_string(), "restflow".to_string())]);

        let result = run_preflight(&[skill], &available_tools, Some(&vars), true);

        assert!(result.passed);
        assert!(result.blockers.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn preflight_blocks_when_model_is_missing() {
        let result = run_preflight(&[], &[], None, false);

        assert!(!result.passed);
        assert_eq!(result.blockers.len(), 1);
        assert_eq!(
            result.blockers[0].category,
            PreflightCategory::InvalidConfig
        );
    }

    #[test]
    fn preflight_warns_when_suggested_tool_missing() {
        let skill = skill_with("hello", &["nonexistent_tool"]);

        let result = run_preflight(&[skill], &[], None, true);

        assert!(result.passed);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0].category, PreflightCategory::MissingTool);
    }

    #[test]
    fn preflight_warns_when_variable_is_unset() {
        let skill = skill_with("Deploy {{service_name}} now", &[]);
        let vars = HashMap::from([("other".to_string(), "value".to_string())]);

        let result = run_preflight(&[skill], &[], Some(&vars), true);

        assert!(result.passed);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(
            result.warnings[0].category,
            PreflightCategory::UnsetVariable
        );
    }
}
