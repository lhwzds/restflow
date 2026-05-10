use std::sync::Arc;

use types::{AgentNode, ApiKeyConfig, ValidationError};

use crate::AppCore;

/// Validate agent fields that require runtime/storage lookups.
pub async fn validate_agent_node_async(
    agent: &AgentNode,
    core: &Arc<AppCore>,
) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();

    let tool_registry = match crate::services::tool_registry::create_tool_registry(
        core.storage.config.clone(),
        None,
        None,
    ) {
        Ok(registry) => registry,
        Err(err) => {
            errors.push(ValidationError::new(
                "tools",
                format!("Failed to create tool registry: {err}"),
            ));
            return Err(errors);
        }
    };

    if let Some(tools) = &agent.tools {
        for tool_name in tools {
            let normalized = tool_name.trim();
            if normalized.is_empty() {
                errors.push(ValidationError::new("tools", "tool name must not be empty"));
                continue;
            }
            if !tool_registry.has(normalized) {
                errors.push(ValidationError::new(
                    "tools",
                    format!("unknown tool: {}", normalized),
                ));
            }
        }
    }

    if let Some(skills) = &agent.skills {
        for skill_id in skills {
            let normalized = skill_id.trim();
            if normalized.is_empty() {
                errors.push(ValidationError::new("skills", "skill ID must not be empty"));
                continue;
            }
            match crate::services::skills::skill_exists_in_catalog(normalized) {
                Ok(true) => {}
                Ok(false) => errors.push(ValidationError::new(
                    "skills",
                    format!("unknown skill: {}", normalized),
                )),
                Err(err) => errors.push(ValidationError::new(
                    "skills",
                    format!("failed to verify skill '{}': {}", normalized, err),
                )),
            }
        }
    }

    if let Some(ApiKeyConfig::Secret(secret_name)) = &agent.api_key_config {
        let normalized = secret_name.trim();
        if !normalized.is_empty() {
            match core.storage.secrets.has_available_secret(normalized) {
                Ok(true) => {}
                Ok(false) => errors.push(ValidationError::new(
                    "api_key_config",
                    format!("secret not found in storage: {}", normalized),
                )),
                Err(err) => errors.push(ValidationError::new(
                    "api_key_config",
                    format!("failed to verify secret '{}': {}", normalized, err),
                )),
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::RestflowTestEnv;

    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn accepts_team_skill() {
        let env = RestflowTestEnv::new();
        let previous_skrun_root = std::env::var_os("SKRUN_SKILLS_DIR");
        let skills_root = env.root().join("skrun-skills");
        let artifact = skrun::SkillArtifact::markdown("team", "Team", "0.1.0", "# Team");
        skrun::save_artifact(skills_root.join("team"), &artifact).unwrap();
        unsafe { std::env::set_var("SKRUN_SKILLS_DIR", &skills_root) };
        let core = Arc::new(
            AppCore::new(env.db_path("agent-skill.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let node = AgentNode {
            skills: Some(vec!["team".to_string()]),
            ..AgentNode::new()
        };

        let result = validate_agent_node_async(&node, &core).await;
        unsafe {
            if let Some(value) = previous_skrun_root {
                std::env::set_var("SKRUN_SKILLS_DIR", value);
            } else {
                std::env::remove_var("SKRUN_SKILLS_DIR");
            }
        }

        assert!(result.is_ok(), "unexpected validation errors: {result:?}");
    }
}
