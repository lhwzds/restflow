//! Batch registration of Skills as Tools.

use std::sync::Arc;

use crate::ToolRegistry;
use crate::skill::SkillProvider;
use crate::skill::tool::SkillAsTool;

/// Register all skills from a provider as dynamic Tools in the registry.
pub fn register_skills(registry: &mut ToolRegistry, provider: Arc<dyn SkillProvider>) {
    for info in provider.list_skills() {
        let tool = SkillAsTool::new(info, provider.clone());
        registry.register(tool);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skill::*;

    struct TestProvider;

    impl SkillProvider for TestProvider {
        fn list_skills(&self) -> Vec<SkillInfo> {
            vec![
                SkillInfo {
                    id: "skill-a".to_string(),
                    name: "Skill A".to_string(),
                    description: Some("First skill".to_string()),
                    tags: None,
                    kind: None,
                    executable: false,
                    suggested_tools: Vec::new(),
                    source: SkillSource::User,
                    read_only: false,
                    source_ref: None,
                },
                SkillInfo {
                    id: "skill-b".to_string(),
                    name: "Skill B".to_string(),
                    description: Some("Second skill".to_string()),
                    tags: None,
                    kind: None,
                    executable: false,
                    suggested_tools: Vec::new(),
                    source: SkillSource::User,
                    read_only: false,
                    source_ref: None,
                },
            ]
        }

        fn get_skill(&self, id: &str) -> Option<SkillContent> {
            match id {
                "skill-a" => Some(SkillContent {
                    id: "skill-a".to_string(),
                    name: "Skill A".to_string(),
                    content: "Do A".to_string(),
                    kind: None,
                    executable: false,
                    suggested_tools: Vec::new(),
                    source: SkillSource::User,
                    read_only: false,
                    source_ref: None,
                }),
                "skill-b" => Some(SkillContent {
                    id: "skill-b".to_string(),
                    name: "Skill B".to_string(),
                    content: "Do B".to_string(),
                    kind: None,
                    executable: false,
                    suggested_tools: Vec::new(),
                    source: SkillSource::User,
                    read_only: false,
                    source_ref: None,
                }),
                _ => None,
            }
        }

        fn export_skill(&self, _id: &str) -> std::result::Result<String, String> {
            Err("not implemented".to_string())
        }
    }

    #[test]
    fn test_register_skills() {
        let mut registry = ToolRegistry::new();
        let provider = Arc::new(TestProvider);
        register_skills(&mut registry, provider);

        assert!(registry.has("skill-a"));
        assert!(registry.has("skill-b"));

        let schemas = registry.schemas();
        assert_eq!(schemas.len(), 2);
    }
}
