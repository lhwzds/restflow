use super::*;

impl AgentRuntimeExecutor {
    pub(super) fn build_background_system_prompt(
        &self,
        agent_node: &AgentNode,
        agent_id: Option<&str>,
        background_task_id: Option<&str>,
        user_input: Option<&str>,
    ) -> Result<String> {
        let mut prompt_agent = agent_node.clone();

        // SECURITY: Build allowed skill set from agent's assigned skills
        let allowed_skills: HashSet<String> = agent_node
            .skills
            .as_ref()
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default();

        if let Some(input) = user_input.map(str::trim).filter(|value| !value.is_empty()) {
            let triggered_skill_ids =
                self.resolve_triggered_skill_ids(agent_node, agent_id, input)?;

            // SECURITY: Only allow triggered skills that are in agent's skill list
            // This prevents capability scope expansion via crafted input
            let allowed_triggered: Vec<String> = triggered_skill_ids
                .into_iter()
                .filter(|skill_id| allowed_skills.contains(skill_id))
                .collect();

            if !allowed_triggered.is_empty() {
                let mut effective_skills = prompt_agent.skills.clone().unwrap_or_default();
                for skill_id in allowed_triggered {
                    if !effective_skills
                        .iter()
                        .any(|existing| existing == &skill_id)
                    {
                        effective_skills.push(skill_id);
                    }
                }
                prompt_agent.skills = Some(effective_skills);
            }
        }

        let base_prompt = build_agent_system_prompt(self.storage.clone(), &prompt_agent, agent_id)?;
        let policy_prompt = prompt_files::load_background_agent_policy(background_task_id)?;
        if policy_prompt.trim().is_empty() {
            return Ok(base_prompt);
        }
        Ok(format!("{base_prompt}\n\n{policy_prompt}"))
    }

    pub(super) fn resolve_triggered_skill_ids(
        &self,
        agent_node: &AgentNode,
        agent_id: Option<&str>,
        user_input: &str,
    ) -> Result<Vec<String>> {
        self.resolve_skill_snapshot(agent_node, agent_id, Some(user_input))
            .map(|snapshot| snapshot.triggered_skill_ids)
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

        let all_skills = self.storage.skills.list()?;
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
                    triggered_skill_ids,
                })
            })?;

        if lookup.hit {
            debug!("Skill snapshot cache hit");
        } else {
            debug!("Skill snapshot cache miss");
        }

        Ok(ResolvedSkillSnapshot {
            triggered_skill_ids: lookup.payload.triggered_skill_ids,
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
        let available_tools = effective_main_agent_tool_names(agent_node.tools.as_deref());
        let mut preflight = run_preflight(
            &skills,
            &available_tools,
            agent_node.skill_variables.as_ref(),
            true,
            agent_node.effective_skill_preflight_policy_mode(),
        );

        if !primary_model.is_codex_cli()
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
