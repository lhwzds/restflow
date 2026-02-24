//! use_skill tool - Query and load skills dynamically.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

use crate::{Result, ToolError};
use crate::{Tool, ToolOutput};
use restflow_traits::skill::SkillProvider;

/// Parameters for use_skill tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UseSkillParams {
    /// Skill ID to load.
    pub skill_id: Option<String>,

    /// If true, list available skills instead of loading.
    #[serde(default)]
    pub list: bool,
}

/// use_skill tool — lets the LLM query available skills and load their content.
pub struct UseSkillTool {
    provider: Arc<dyn SkillProvider>,
}

impl UseSkillTool {
    pub fn new(provider: Arc<dyn SkillProvider>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl Tool for UseSkillTool {
    fn name(&self) -> &str {
        "use_skill"
    }

    fn description(&self) -> &str {
        "Load a skill to gain specialized capabilities. Skills provide domain-specific instructions and may restrict available tools."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "skill_id": {
                    "type": "string",
                    "description": "The skill ID to load"
                },
                "list": {
                    "type": "boolean",
                    "default": false,
                    "description": "If true, list available skills instead of loading"
                }
            }
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: UseSkillParams = serde_json::from_value(input)
            .map_err(|e| ToolError::Tool(format!("Invalid parameters: {}", e)))?;

        if params.list {
            let skills: Vec<Value> = self
                .provider
                .list_skills()
                .into_iter()
                .map(|info| {
                    json!({
                        "id": info.id,
                        "name": info.name,
                        "description": info.description,
                        "tags": info.tags,
                    })
                })
                .collect();

            return Ok(ToolOutput::success(json!({
                "available_skills": skills,
                "count": skills.len(),
            })));
        }

        let skill_id = params
            .skill_id
            .ok_or_else(|| ToolError::Tool("Missing 'skill_id' parameter".to_string()))?;

        match self.provider.get_skill(&skill_id) {
            Some(content) => Ok(ToolOutput::success(json!({
                "loaded": true,
                "skill_id": content.id,
                "name": content.name,
                "content": content.content,
            }))),
            None => Ok(ToolOutput::error(format!("Skill '{}' not found", skill_id))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_traits::skill::{SkillContent, SkillInfo, SkillRecord, SkillUpdate};

    struct MockProvider;

    impl SkillProvider for MockProvider {
        fn list_skills(&self) -> Vec<SkillInfo> {
            vec![SkillInfo {
                id: "test-skill".to_string(),
                name: "Test Skill".to_string(),
                description: Some("A test skill".to_string()),
                tags: None,
            }]
        }

        fn get_skill(&self, id: &str) -> Option<SkillContent> {
            if id == "test-skill" {
                Some(SkillContent {
                    id: "test-skill".to_string(),
                    name: "Test Skill".to_string(),
                    content: "# Test Skill\nDo something useful.".to_string(),
                })
            } else {
                None
            }
        }

        fn create_skill(&self, _: SkillRecord) -> std::result::Result<SkillRecord, String> {
            Err("not implemented".to_string())
        }
        fn update_skill(
            &self,
            _: &str,
            _: SkillUpdate,
        ) -> std::result::Result<SkillRecord, String> {
            Err("not implemented".to_string())
        }
        fn delete_skill(&self, _: &str) -> std::result::Result<bool, String> {
            Err("not implemented".to_string())
        }
        fn export_skill(&self, _: &str) -> std::result::Result<String, String> {
            Err("not implemented".to_string())
        }
        fn import_skill(
            &self,
            _: &str,
            _: &str,
            _: bool,
        ) -> std::result::Result<SkillRecord, String> {
            Err("not implemented".to_string())
        }
    }

    #[test]
    fn test_params_list() {
        let params: UseSkillParams = serde_json::from_str(r#"{"list": true}"#).unwrap();
        assert!(params.list);
        assert!(params.skill_id.is_none());
    }

    #[test]
    fn test_params_load() {
        let params: UseSkillParams =
            serde_json::from_str(r#"{"skill_id": "api-testing"}"#).unwrap();
        assert!(!params.list);
        assert_eq!(params.skill_id, Some("api-testing".to_string()));
    }

    #[tokio::test]
    async fn test_list_skills() {
        let tool = UseSkillTool::new(Arc::new(MockProvider));
        let result = tool.execute(json!({"list": true})).await.unwrap();
        assert!(result.success);
        assert_eq!(result.result["count"], 1);
        assert_eq!(result.result["available_skills"][0]["id"], "test-skill");
    }

    #[tokio::test]
    async fn test_load_skill() {
        let tool = UseSkillTool::new(Arc::new(MockProvider));
        let result = tool
            .execute(json!({"skill_id": "test-skill"}))
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.result["loaded"], true);
        assert_eq!(result.result["skill_id"], "test-skill");
        assert!(
            result.result["content"]
                .as_str()
                .unwrap()
                .contains("Do something useful")
        );
    }

    #[tokio::test]
    async fn test_load_skill_not_found() {
        let tool = UseSkillTool::new(Arc::new(MockProvider));
        let result = tool
            .execute(json!({"skill_id": "nonexistent"}))
            .await
            .unwrap();
        assert!(!result.success);
    }

    #[tokio::test]
    async fn test_load_skill_missing_skill_id() {
        let tool = UseSkillTool::new(Arc::new(MockProvider));
        // Neither list nor skill_id — should require skill_id
        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
    }

    struct EmptyProvider;

    impl SkillProvider for EmptyProvider {
        fn list_skills(&self) -> Vec<SkillInfo> {
            vec![]
        }
        fn get_skill(&self, _: &str) -> Option<SkillContent> {
            None
        }
        fn create_skill(&self, _: SkillRecord) -> std::result::Result<SkillRecord, String> {
            Err("not implemented".to_string())
        }
        fn update_skill(
            &self,
            _: &str,
            _: SkillUpdate,
        ) -> std::result::Result<SkillRecord, String> {
            Err("not implemented".to_string())
        }
        fn delete_skill(&self, _: &str) -> std::result::Result<bool, String> {
            Err("not implemented".to_string())
        }
        fn export_skill(&self, _: &str) -> std::result::Result<String, String> {
            Err("not implemented".to_string())
        }
        fn import_skill(
            &self,
            _: &str,
            _: &str,
            _: bool,
        ) -> std::result::Result<SkillRecord, String> {
            Err("not implemented".to_string())
        }
    }

    #[tokio::test]
    async fn test_list_skills_empty() {
        let tool = UseSkillTool::new(Arc::new(EmptyProvider));
        let result = tool.execute(json!({"list": true})).await.unwrap();
        assert!(result.success);
        assert_eq!(result.result["count"], 0);
        assert_eq!(
            result.result["available_skills"].as_array().unwrap().len(),
            0
        );
    }
}
