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
            skill.content = Self::apply_variables(&skill.content, variables);
            for (name, value) in variables {
                skill.variables.push((name.clone(), value.clone()));
            }
        }
        Ok(skills)
    }

    fn apply_variables(content: &str, variables: &HashMap<String, String>) -> String {
        if variables.is_empty() {
            return content.to_string();
        }
        let pattern_map: HashMap<String, &str> = variables
            .iter()
            .map(|(name, value)| (format!("{{{{{}}}}}", name), value.as_str()))
            .collect();
        let replacements: HashMap<&str, &str> = pattern_map
            .iter()
            .map(|(pattern, value)| (pattern.as_str(), *value))
            .collect();
        crate::utils::template::render_template_single_pass(content, &replacements)
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

#[cfg(test)]
mod tests {
    use super::SkillLoader;
    use std::collections::HashMap;

    #[test]
    fn apply_variables_prevents_double_substitution() {
        let vars = HashMap::from([
            ("output".to_string(), "raw {{task_id}}".to_string()),
            ("task_id".to_string(), "task-1".to_string()),
        ]);
        let rendered = SkillLoader::apply_variables("Result: {{output}}", &vars);
        assert_eq!(rendered, "Result: raw {{task_id}}");
    }
}
