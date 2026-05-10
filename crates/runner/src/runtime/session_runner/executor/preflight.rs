use super::*;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::sync::RwLock;
use types::SkillPreflightPolicyMode;

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
    InvalidConfig,
}

impl PreflightCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MissingTool => "missing_tool",
            Self::MissingSecret => "missing_secret",
            Self::UnsetVariable => "unset_variable",
            Self::InvalidConfig => "invalid_config",
        }
    }
}

pub fn run_preflight(
    skills: &[Skill],
    available_tools: &[String],
    skill_variables: Option<&HashMap<String, String>>,
    model_configured: bool,
    policy_mode: SkillPreflightPolicyMode,
) -> PreflightResult {
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();
    let mut skill_warnings = Vec::new();

    if !model_configured {
        blockers.push(PreflightIssue {
            category: PreflightCategory::InvalidConfig,
            message: "No model configured for agent".to_string(),
            suggestion: Some(
                "Set model in agent definition or configure provider credentials".into(),
            ),
        });
    }

    if policy_mode != SkillPreflightPolicyMode::Off {
        let available_tool_set: HashSet<&str> =
            available_tools.iter().map(String::as_str).collect();
        for skill in skills {
            for tool_name in &skill.suggested_tools {
                if !available_tool_set.contains(tool_name.as_str()) {
                    skill_warnings.push(PreflightIssue {
                        category: PreflightCategory::MissingTool,
                        message: format!(
                            "Suggested tool '{}' from skill '{}' is not available",
                            tool_name, skill.id
                        ),
                        suggestion: Some(
                            "Check tool allowlist or remove from suggested_tools".into(),
                        ),
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
                    skill_warnings.push(PreflightIssue {
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
    }

    match policy_mode {
        SkillPreflightPolicyMode::Off => {}
        SkillPreflightPolicyMode::Warn => warnings.extend(skill_warnings),
        SkillPreflightPolicyMode::Enforce => {
            for issue in skill_warnings {
                if is_critical_skill_warning(issue.category) {
                    blockers.push(issue);
                } else {
                    warnings.push(issue);
                }
            }
        }
    }

    PreflightResult {
        passed: blockers.is_empty(),
        blockers,
        warnings,
    }
}

fn is_critical_skill_warning(category: PreflightCategory) -> bool {
    matches!(
        category,
        PreflightCategory::MissingTool | PreflightCategory::UnsetVariable
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct SkillSnapshotKey {
    pub agent_id: Option<String>,
    pub skill_filter_signature: String,
    pub trigger_context_signature: String,
}

impl SkillSnapshotKey {
    pub fn new(
        agent_id: Option<String>,
        skill_filter_signature: String,
        trigger_context_signature: String,
    ) -> Self {
        Self {
            agent_id,
            skill_filter_signature,
            trigger_context_signature,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct SkillSnapshotPayload {
    pub resolved_skills: Vec<Skill>,
}

#[derive(Debug, Clone)]
struct CachedSkillSnapshot {
    version_hash: String,
    payload: SkillSnapshotPayload,
}

#[derive(Debug, Clone)]
pub(super) struct SkillSnapshotLookup {
    pub payload: SkillSnapshotPayload,
    pub hit: bool,
}

#[derive(Debug, Default)]
pub(super) struct SkillSnapshotCache {
    entries: RwLock<HashMap<SkillSnapshotKey, CachedSkillSnapshot>>,
}

impl SkillSnapshotCache {
    pub fn resolve_with<F>(
        &self,
        key: SkillSnapshotKey,
        version_hash: String,
        refresh: F,
    ) -> Result<SkillSnapshotLookup>
    where
        F: FnOnce() -> Result<SkillSnapshotPayload>,
    {
        {
            let entries = self
                .entries
                .read()
                .map_err(|error| anyhow!("Skill snapshot cache lock poisoned: {error}"))?;
            if let Some(cached) = entries.get(&key)
                && cached.version_hash == version_hash
            {
                return Ok(SkillSnapshotLookup {
                    payload: cached.payload.clone(),
                    hit: true,
                });
            }
        }

        let refreshed = refresh()?;
        let cached = CachedSkillSnapshot {
            version_hash,
            payload: refreshed.clone(),
        };

        let mut entries = self
            .entries
            .write()
            .map_err(|error| anyhow!("Skill snapshot cache lock poisoned: {error}"))?;
        entries.insert(key, cached);

        Ok(SkillSnapshotLookup {
            payload: refreshed,
            hit: false,
        })
    }
}

pub(super) fn build_skill_filter_signature(skill_filter: Option<&[String]>) -> String {
    let mut ids: Vec<&str> = skill_filter
        .unwrap_or_default()
        .iter()
        .map(String::as_str)
        .collect();
    ids.sort_unstable();
    hash_text(&ids.join("|"))
}

pub(super) fn build_trigger_context_signature(trigger_context: Option<&str>) -> String {
    hash_text(trigger_context.map(str::trim).unwrap_or(""))
}

pub(super) fn build_skill_version_hash(skills: &[Skill]) -> String {
    let mut versions: Vec<String> = skills
        .iter()
        .map(|skill| {
            let fallback_content_hash = hex::encode(Sha256::digest(skill.content.as_bytes()));
            let content_version_hash = skill
                .content_hash
                .as_ref()
                .cloned()
                .unwrap_or(fallback_content_hash);
            format!("{}:{}:{}", skill.id, skill.updated_at, content_version_hash)
        })
        .collect();
    versions.sort_unstable();
    hash_text(&versions.join("\n"))
}

fn hash_text(input: &str) -> String {
    hex::encode(Sha256::digest(input.as_bytes()))
}

impl AgentRuntimeExecutor {
    pub(super) fn resolve_effective_tool_names(
        &self,
        agent_node: &AgentNode,
        _agent_id: Option<&str>,
        user_input: Option<&str>,
    ) -> Result<Vec<String>> {
        let skills = crate::services::skills::list_available_skills()?;
        let result = effective_tool_allowlist_for_turn(
            agent_node,
            user_input,
            &skills,
            SkillActivationPolicy::IgnoreInvalid,
        )?;
        Ok(result.tool_names)
    }

    pub(super) fn resolve_preflight_available_tool_names(
        &self,
        agent_node: &AgentNode,
        user_input: Option<&str>,
    ) -> Result<Vec<String>> {
        let requested_tools = self.resolve_effective_tool_names(agent_node, None, user_input)?;
        let registry = registry_from_allowlist(
            Some(&requested_tools),
            None,
            Some(secret_resolver_from_storage(&self.storage)),
            Some(self.storage.as_ref()),
            None,
            None,
            None,
        )?;
        let mut available_tools = registry
            .list()
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();

        for caller_registered in [
            "spawn_subagent",
            "spawn_subagent_batch",
            "wait_subagents",
            "list_subagents",
            "switch_model",
            "process",
            "reply",
        ] {
            if requested_tools.iter().any(|tool| tool == caller_registered)
                && !available_tools.iter().any(|tool| tool == caller_registered)
            {
                available_tools.push(caller_registered.to_string());
            }
        }

        Ok(available_tools)
    }

    pub(super) fn resolve_preflight_skills(
        &self,
        agent_node: &AgentNode,
        user_input: Option<&str>,
    ) -> Result<Vec<Skill>> {
        self.resolve_skill_snapshot(agent_node, None, user_input)
            .map(|snapshot| snapshot.resolved_skills)
    }

    pub(super) fn resolve_skill_snapshot(
        &self,
        agent_node: &AgentNode,
        agent_id: Option<&str>,
        user_input: Option<&str>,
    ) -> Result<ResolvedSkillSnapshot> {
        let normalized_input = user_input.map(str::trim).filter(|value| !value.is_empty());
        let key = SkillSnapshotKey::new(
            agent_id.map(|value| value.to_string()),
            build_skill_filter_signature(agent_node.skills.as_deref()),
            build_trigger_context_signature(normalized_input),
        );

        let all_skills = crate::services::skills::list_available_skills()?;
        let version_hash = build_skill_version_hash(&all_skills);

        let mut assigned_skill_ids = agent_node.skills.clone().unwrap_or_default();
        let allowed_skills: HashSet<String> = assigned_skill_ids.iter().cloned().collect();

        let lookup = self
            .skill_snapshot_cache
            .resolve_with(key, version_hash, move || {
                let triggered_skill_ids = normalized_input
                    .map(|input| {
                        match_triggers(input, &all_skills)
                            .into_iter()
                            .map(|matched| matched.skill_id)
                            .collect::<Vec<String>>()
                    })
                    .unwrap_or_default();

                for skill_id in &triggered_skill_ids {
                    if allowed_skills.contains(skill_id)
                        && !assigned_skill_ids
                            .iter()
                            .any(|existing| existing == skill_id)
                    {
                        assigned_skill_ids.push(skill_id.clone());
                    }
                }

                let skill_by_id: HashMap<String, Skill> = all_skills
                    .into_iter()
                    .map(|skill| (skill.id.clone(), skill))
                    .collect();
                let mut resolved_skills = Vec::new();
                for skill_id in assigned_skill_ids {
                    match skill_by_id.get(&skill_id) {
                        Some(skill) => resolved_skills.push(skill.clone()),
                        None => {
                            warn!(skill_id = %skill_id, "Skill referenced by agent not found during preflight")
                        }
                    }
                }

                Ok(SkillSnapshotPayload {
                    resolved_skills,
                })
            })?;

        if lookup.hit {
            debug!("Skill snapshot cache hit");
        } else {
            debug!("Skill snapshot cache miss");
        }

        Ok(ResolvedSkillSnapshot {
            resolved_skills: lookup.payload.resolved_skills,
        })
    }

    pub(super) async fn run_preflight_check(
        &self,
        agent_node: &AgentNode,
        primary_model: ModelId,
        primary_provider: Provider,
        user_input: Option<&str>,
    ) -> Result<()> {
        let skills = self.resolve_preflight_skills(agent_node, user_input)?;
        let available_tools =
            self.resolve_preflight_available_tool_names(agent_node, user_input)?;
        let mut preflight = run_preflight(
            &skills,
            &available_tools,
            agent_node.skill_variables.as_ref(),
            true,
            agent_node.effective_skill_preflight_policy_mode(),
        );

        if !Self::should_skip_api_key_resolution()
            && !primary_model.is_codex_cli()
            && !primary_model.is_gemini_cli()
            && let Err(error) = self
                .resolve_api_key_for_model(
                    primary_provider,
                    agent_node.api_key_config.as_ref(),
                    primary_provider,
                )
                .await
        {
            preflight.blockers.push(PreflightIssue {
                category: PreflightCategory::MissingSecret,
                message: error.to_string(),
                suggestion: Some("Configure API key via auth profile or secrets".to_string()),
            });
            preflight.passed = false;
        }

        for warning_issue in &preflight.warnings {
            warn!(
                category = warning_issue.category.as_str(),
                message = %warning_issue.message,
                suggestion = ?warning_issue.suggestion,
                "Background agent preflight warning"
            );
        }

        if !preflight.passed {
            let blocker_message = preflight
                .blockers
                .iter()
                .map(|issue| format!("- [{}] {}", issue.category.as_str(), issue.message))
                .collect::<Vec<_>>()
                .join("\n");
            return Err(anyhow!("Preflight check failed:\n{}", blocker_message));
        }

        Ok(())
    }
}
