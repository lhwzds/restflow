//! Skill tool for listing and reading skills

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::error::Result;
use crate::tools::traits::{SkillProvider, Tool, ToolOutput};

#[derive(Debug, Deserialize)]
struct SkillInput {
    action: String, // "list" or "read"
    id: Option<String>, // Required for "read" action
}

/// Skill tool for listing and reading skills
pub struct SkillTool {
    provider: Arc<dyn SkillProvider>,
}

impl SkillTool {
    /// Create a new skill tool with the given provider
    pub fn new(provider: Arc<dyn SkillProvider>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl Tool for SkillTool {
    fn name(&self) -> &str {
        "skill"
    }

    fn description(&self) -> &str {
        "Access skills (reusable AI prompt templates). Use 'list' action to see all available skills, or 'read' action with an id to get skill content."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "read"],
                    "description": "Action to perform: 'list' to see all skills, 'read' to get a specific skill's content"
                },
                "id": {
                    "type": "string",
                    "description": "Skill ID (required for 'read' action)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: SkillInput = serde_json::from_value(input)?;

        match params.action.as_str() {
            "list" => {
                let skills = self.provider.list_skills();
                Ok(ToolOutput::success(json!({
                    "skills": skills
                })))
            }
            "read" => {
                let id = match params.id {
                    Some(id) => id,
                    None => return Ok(ToolOutput::error("Missing 'id' parameter for read action")),
                };

                match self.provider.get_skill(&id) {
                    Some(skill) => Ok(ToolOutput::success(json!(skill))),
                    None => Ok(ToolOutput::error(format!("Skill '{}' not found", id))),
                }
            }
            _ => Ok(ToolOutput::error(format!(
                "Unknown action: '{}'. Use 'list' or 'read'",
                params.action
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::traits::{SkillContent, SkillInfo};

    struct MockSkillProvider {
        skills: Vec<(SkillInfo, String)>,
    }

    impl SkillProvider for MockSkillProvider {
        fn list_skills(&self) -> Vec<SkillInfo> {
            self.skills.iter().map(|(info, _)| info.clone()).collect()
        }

        fn get_skill(&self, id: &str) -> Option<SkillContent> {
            self.skills.iter().find(|(info, _)| info.id == id).map(|(info, content)| {
                SkillContent {
                    id: info.id.clone(),
                    name: info.name.clone(),
                    content: content.clone(),
                }
            })
        }
    }

    fn create_mock_provider() -> Arc<dyn SkillProvider> {
        Arc::new(MockSkillProvider {
            skills: vec![
                (
                    SkillInfo {
                        id: "test-skill".to_string(),
                        name: "Test Skill".to_string(),
                        description: Some("A test skill".to_string()),
                        tags: Some(vec!["test".to_string()]),
                    },
                    "# Test Skill Content\n\nThis is a test.".to_string(),
                ),
            ],
        })
    }

    #[test]
    fn test_skill_tool_schema() {
        let tool = SkillTool::new(create_mock_provider());
        assert_eq!(tool.name(), "skill");
        assert!(!tool.description().is_empty());

        let schema = tool.parameters_schema();
        assert!(schema.get("properties").is_some());
    }

    #[tokio::test]
    async fn test_list_skills() {
        let tool = SkillTool::new(create_mock_provider());
        let result = tool.execute(json!({ "action": "list" })).await.unwrap();

        assert!(result.success);
        let skills = result.result.get("skills").unwrap().as_array().unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0]["id"], "test-skill");
    }

    #[tokio::test]
    async fn test_read_skill() {
        let tool = SkillTool::new(create_mock_provider());
        let result = tool.execute(json!({ "action": "read", "id": "test-skill" })).await.unwrap();

        assert!(result.success);
        assert_eq!(result.result["id"], "test-skill");
        assert!(result.result["content"].as_str().unwrap().contains("Test Skill Content"));
    }

    #[tokio::test]
    async fn test_read_skill_not_found() {
        let tool = SkillTool::new(create_mock_provider());
        let result = tool.execute(json!({ "action": "read", "id": "nonexistent" })).await.unwrap();

        assert!(!result.success);
        assert!(result.error.unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_read_skill_missing_id() {
        let tool = SkillTool::new(create_mock_provider());
        let result = tool.execute(json!({ "action": "read" })).await.unwrap();

        assert!(!result.success);
        assert!(result.error.unwrap().contains("Missing 'id'"));
    }
}
