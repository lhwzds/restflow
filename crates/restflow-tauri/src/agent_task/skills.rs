//! Skill loading utilities for agent prompt injection.

use anyhow::Result;
use restflow_core::storage::Storage;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::warn;

/// Processed skill ready for system prompt injection.
#[derive(Debug, Clone)]
pub struct ProcessedSkill {
    pub name: String,
    pub content: String,
    pub variables: Vec<(String, String)>,
}

impl ProcessedSkill {
    /// Format the skill for prompt injection.
    pub fn format_for_prompt(&self) -> String {
        format!("## Skill: {}\n\n{}", self.name, self.content)
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_core::models::Skill;
    use tempfile::tempdir;

    fn create_storage_with_skill(content: &str) -> (Arc<Storage>, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
        let skill = Skill::new(
            "skill-1".to_string(),
            "Test Skill".to_string(),
            None,
            None,
            content.to_string(),
        );
        storage.skills.create(&skill).unwrap();
        (Arc::new(storage), temp_dir)
    }

    #[test]
    fn test_build_system_prompt_injects_skills() {
        let (storage, _tmp_dir) = create_storage_with_skill("# Skill Content");
        let loader = SkillLoader::new(storage);
        let prompt = loader
            .build_system_prompt("Base prompt", &["skill-1".to_string()], None)
            .unwrap();

        assert!(prompt.contains("Base prompt"));
        assert!(prompt.contains("# Available Skills"));
        assert!(prompt.contains("# Skill Content"));
    }

    #[test]
    fn test_variable_substitution() {
        let (storage, _tmp_dir) = create_storage_with_skill("Hello {{name}}!");
        let loader = SkillLoader::new(storage);
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "RestFlow".to_string());

        let prompt = loader
            .build_system_prompt("Base", &["skill-1".to_string()], Some(&vars))
            .unwrap();

        assert!(prompt.contains("Hello RestFlow!"));
    }

    #[test]
    fn test_no_skills_returns_base_prompt() {
        let (storage, _tmp_dir) = create_storage_with_skill("Ignored");
        let loader = SkillLoader::new(storage);
        let prompt = loader
            .build_system_prompt("Base prompt", &[], None)
            .unwrap();

        assert_eq!(prompt, "Base prompt");
    }
}
