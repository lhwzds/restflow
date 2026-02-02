//! Skill loader implementation for the unified agent.

use anyhow::Result;
use restflow_core::storage::Storage;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::warn;

use super::ProcessedSkill;

/// Loader for agent skills stored in the database.
pub struct SkillLoader {
    storage: Arc<Storage>,
}

impl SkillLoader {
    /// Create a new loader with storage access.
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }

    /// Load skills by ID without variable substitution.
    pub fn load_skills(&self, skill_ids: &[String]) -> Result<Vec<ProcessedSkill>> {
        let mut skills = Vec::new();
        for id in skill_ids {
            match self.storage.skills.get(id)? {
                Some(skill) => skills.push(ProcessedSkill {
                    name: skill.name,
                    content: skill.content,
                    variables: Vec::new(),
                }),
                None => {
                    warn!(skill_id = %id, "Skill not found while loading agent skills");
                }
            }
        }
        Ok(skills)
    }

    /// Load skills and apply variable substitution.
    pub fn load_skills_with_vars(
        &self,
        skill_ids: &[String],
        variables: &HashMap<String, String>,
    ) -> Result<Vec<ProcessedSkill>> {
        let mut skills = self.load_skills(skill_ids)?;
        for skill in &mut skills {
            for (name, value) in variables {
                let pattern = format!("{{{{{}}}}}", name);
                skill.content = skill.content.replace(&pattern, value);
            }
            skill.variables = variables
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
        }
        Ok(skills)
    }

    /// Build a system prompt with skills injected.
    pub fn build_system_prompt(
        &self,
        base_prompt: &str,
        skill_ids: &[String],
        variables: Option<&HashMap<String, String>>,
    ) -> Result<String> {
        if skill_ids.is_empty() {
            return Ok(base_prompt.to_string());
        }

        let skills = match variables {
            Some(vars) => self.load_skills_with_vars(skill_ids, vars)?,
            None => self.load_skills(skill_ids)?,
        };

        if skills.is_empty() {
            return Ok(base_prompt.to_string());
        }

        let mut prompt = base_prompt.to_string();
        prompt.push_str("\n\n---\n\n# Available Skills\n\n");
        for skill in skills {
            prompt.push_str(&skill.format_for_prompt());
            prompt.push_str("\n\n");
        }

        Ok(prompt)
    }
}
