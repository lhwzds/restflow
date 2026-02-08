//! Skill loader implementation.

use super::ProcessedSkill;
use crate::storage::Storage;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::warn;

pub struct SkillLoader {
    storage: Arc<Storage>,
}

impl SkillLoader {
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }

    /// Load skills by IDs and process them
    pub fn load_skills(&self, skill_ids: &[String]) -> Result<Vec<ProcessedSkill>> {
        let mut skills = Vec::new();
        for id in skill_ids {
            match self.storage.skills.get(id)? {
                Some(skill) => skills.push(ProcessedSkill {
                    name: skill.name.clone(),
                    content: skill.content.clone(),
                    variables: Vec::new(),
                }),
                None => {
                    warn!(skill_id = %id, "Skill not found while loading agent skills");
                }
            }
        }
        Ok(skills)
    }

    /// Load skills and apply variable substitution
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
                skill.variables.push((name.clone(), value.clone()));
            }
        }
        Ok(skills)
    }

    /// Build system prompt with skills injected
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
        prompt.push_str("\n\n---\n\n# Available Skills\n\nThese are skills you can use to accomplish tasks. Read the skill content with the `skill` tool before executing.\n\n");
        for skill in skills {
            prompt.push_str(&skill.format_for_prompt());
            prompt.push_str("\n\n");
        }
        Ok(prompt)
    }
}
