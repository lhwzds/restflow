use super::*;

impl RestFlowMcpServer {
    pub(crate) async fn handle_list_skills(
        &self,
        params: ListSkillsParams,
    ) -> Result<String, String> {
        let skills = self
            .backend
            .list_skills()
            .await
            .map_err(|e| format!("Failed to list skills: {}", e))?;

        let status_filter = Self::parse_skill_status(params.status)?;
        let summaries: Vec<SkillSummary> = skills
            .into_iter()
            .filter(|s| match &status_filter {
                Some(status) => &s.status == status,
                None => true,
            })
            .map(|s| SkillSummary {
                id: s.id,
                name: s.name,
                description: s.description,
                tags: s.tags,
                kind: s.kind,
                executable: s.executable,
                suggested_tools: s.suggested_tools,
                status: s.status,
            })
            .collect();

        serde_json::to_string_pretty(&summaries)
            .map_err(|e| format!("Failed to serialize skills: {}", e))
    }

    pub(crate) async fn handle_get_skill(&self, params: GetSkillParams) -> Result<String, String> {
        let skill = self
            .backend
            .get_skill(&params.id)
            .await
            .map_err(|e| format!("Failed to get skill: {}", e))?
            .ok_or_else(|| format!("Skill not found: {}", params.id))?;

        serde_json::to_string_pretty(&skill)
            .map_err(|e| format!("Failed to serialize skill: {}", e))
    }

    pub(crate) async fn handle_get_skill_reference(
        &self,
        params: GetSkillReferenceParams,
    ) -> Result<String, String> {
        let content = self
            .backend
            .get_skill_reference(&params.skill_id, &params.ref_id)
            .await
            .map_err(|e| format!("Failed to get skill reference: {}", e))?
            .ok_or_else(|| {
                format!(
                    "Reference not found: skill_id={}, ref_id={}",
                    params.skill_id, params.ref_id
                )
            })?;

        let response = serde_json::json!({
            "skill_id": params.skill_id,
            "ref_id": params.ref_id,
            "content": content,
        });

        serde_json::to_string_pretty(&response)
            .map_err(|e| format!("Failed to serialize reference response: {}", e))
    }

    pub(crate) async fn handle_get_skill_context(
        &self,
        params: GetSkillContextParams,
    ) -> Result<String, String> {
        let mut skill = self
            .backend
            .get_skill(&params.skill_id)
            .await
            .map_err(|e| format!("Failed to get skill: {}", e))?
            .ok_or_else(|| format!("Skill not found: {}", params.skill_id))?;

        if skill.auto_complete && skill.status != SkillStatus::Completed {
            skill.status = SkillStatus::Completed;
        }
        let response_status = if skill.auto_complete {
            SkillStatus::Completed
        } else {
            skill.status.clone()
        };

        let available_references: Vec<Value> = skill
            .references
            .iter()
            .map(|reference| {
                serde_json::json!({
                    "ref_id": reference.id,
                    "path": reference.path,
                    "title": reference.title.as_deref().unwrap_or(reference.id.as_str()),
                    "summary": reference.summary.as_deref().unwrap_or("No summary"),
                })
            })
            .collect();

        let response = serde_json::json!({
            "skill_id": skill.id,
            "name": skill.name,
            "content": skill.content,
            "input": params.input,
            "status": response_status,
            "available_references": available_references,
            "note_references": "Deep reference content is available on-demand via get_skill_reference.",
            "note": "Skill execution is not supported via MCP. Use the content with the input as context."
        });

        serde_json::to_string_pretty(&response)
            .map_err(|e| format!("Failed to serialize skill response: {}", e))
    }
}
