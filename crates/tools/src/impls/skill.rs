//! Skill tool for listing and reading skills

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::Result;
use crate::{SecurityGate, ToolAction};
use crate::{Tool, ToolOutput, check_security};
use types::skill::SkillProvider;

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum SkillInput {
    List,
    Read { id: String },
    Export { id: String },
}

/// Skill tool for managing skills
pub struct SkillTool {
    provider: Arc<dyn SkillProvider>,
    security_gate: Option<Arc<dyn SecurityGate>>,
    agent_id: Option<String>,
    task_id: Option<String>,
}

impl SkillTool {
    /// Create a new skill tool with the given provider
    pub fn new(provider: Arc<dyn SkillProvider>) -> Self {
        Self {
            provider,
            security_gate: None,
            agent_id: None,
            task_id: None,
        }
    }

    pub fn with_security(
        mut self,
        security_gate: Arc<dyn SecurityGate>,
        agent_id: impl Into<String>,
        task_id: impl Into<String>,
    ) -> Self {
        self.security_gate = Some(security_gate);
        self.agent_id = Some(agent_id.into());
        self.task_id = Some(task_id.into());
        self
    }
}

#[async_trait]
impl Tool for SkillTool {
    fn name(&self) -> &str {
        "skill"
    }

    fn description(&self) -> &str {
        "List, read, and export reusable skill definitions from the skrun-managed catalog."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "read", "export"],
                    "description": "Action to perform"
                },
                "id": {
                    "type": "string",
                    "description": "Skill ID (required for read/export)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: SkillInput = serde_json::from_value(input)?;

        match params {
            SkillInput::List => {
                if let Some(message) = check_security(
                    self.security_gate.as_deref(),
                    ToolAction {
                        tool_name: "skill".to_string(),
                        operation: "list".to_string(),
                        target: "*".to_string(),
                        summary: "List skills".to_string(),
                    },
                    self.agent_id.as_deref(),
                    self.task_id.as_deref(),
                )
                .await?
                {
                    return Ok(ToolOutput::error(message));
                }
                let skills = self.provider.list_skills();
                Ok(ToolOutput::success(json!({
                    "skills": skills
                })))
            }
            SkillInput::Read { id } => {
                if let Some(message) = check_security(
                    self.security_gate.as_deref(),
                    ToolAction {
                        tool_name: "skill".to_string(),
                        operation: "read".to_string(),
                        target: id.clone(),
                        summary: format!("Read skill '{}'", id),
                    },
                    self.agent_id.as_deref(),
                    self.task_id.as_deref(),
                )
                .await?
                {
                    return Ok(ToolOutput::error(message));
                }
                match self.provider.get_skill(&id) {
                    Some(skill) => Ok(ToolOutput::success(json!(skill))),
                    None => Ok(ToolOutput::error(format!("Skill '{}' not found", id))),
                }
            }
            SkillInput::Export { id } => match self.provider.export_skill(&id) {
                Ok(markdown) => {
                    if let Some(message) = check_security(
                        self.security_gate.as_deref(),
                        ToolAction {
                            tool_name: "skill".to_string(),
                            operation: "export".to_string(),
                            target: id.clone(),
                            summary: format!("Export skill '{}'", id),
                        },
                        self.agent_id.as_deref(),
                        self.task_id.as_deref(),
                    )
                    .await?
                    {
                        return Ok(ToolOutput::error(message));
                    }
                    Ok(ToolOutput::success(json!({
                        "id": id,
                        "markdown": markdown
                    })))
                }
                Err(err) => Ok(ToolOutput::error(format!("Skill operation failed: {err}"))),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::skill::{SkillContent, SkillInfo, SkillSource};

    #[derive(Clone)]
    struct TestSkill {
        id: String,
        name: String,
        description: Option<String>,
        tags: Option<Vec<String>>,
        content: String,
        source: SkillSource,
        read_only: bool,
        source_ref: Option<String>,
    }

    struct MockSkillProvider {
        skills: Vec<TestSkill>,
    }

    impl SkillProvider for MockSkillProvider {
        fn list_skills(&self) -> Vec<SkillInfo> {
            self.skills
                .iter()
                .map(|skill| SkillInfo {
                    id: skill.id.clone(),
                    name: skill.name.clone(),
                    description: skill.description.clone(),
                    tags: skill.tags.clone(),
                    kind: None,
                    executable: false,
                    suggested_tools: Vec::new(),
                    source: skill.source,
                    read_only: skill.read_only,
                    source_ref: skill.source_ref.clone(),
                })
                .collect()
        }

        fn get_skill(&self, id: &str) -> Option<SkillContent> {
            self.skills
                .iter()
                .find(|skill| skill.id == id)
                .map(|skill| SkillContent {
                    id: skill.id.clone(),
                    name: skill.name.clone(),
                    content: skill.content.clone(),
                    kind: None,
                    executable: false,
                    suggested_tools: Vec::new(),
                    source: skill.source,
                    read_only: skill.read_only,
                    source_ref: skill.source_ref.clone(),
                })
        }

        fn export_skill(&self, id: &str) -> std::result::Result<String, String> {
            let skill = self
                .skills
                .iter()
                .find(|s| s.id == id)
                .ok_or_else(|| format!("Skill {} not found", id))?;
            Ok(format!(
                "---\nname: {}\n---\n\n{}",
                skill.name, skill.content
            ))
        }
    }

    fn create_mock_provider() -> Arc<dyn SkillProvider> {
        Arc::new(MockSkillProvider {
            skills: vec![TestSkill {
                id: "test-skill".to_string(),
                name: "Test Skill".to_string(),
                description: Some("A test skill".to_string()),
                tags: Some(vec!["test".to_string()]),
                content: "# Test Skill Content\n\nThis is a test.".to_string(),
                source: SkillSource::User,
                read_only: false,
                source_ref: None,
            }],
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

    #[test]
    fn test_schema_never_exposes_write_actions() {
        let tool = SkillTool::new(create_mock_provider());
        let schema = tool.parameters_schema();
        let actions = schema["properties"]["action"]["enum"]
            .as_array()
            .expect("enum array");
        for write_action in ["create", "update", "delete", "import"] {
            assert!(
                !actions
                    .iter()
                    .any(|value| value.as_str() == Some(write_action)),
                "skill tool must not expose {write_action} action"
            );
        }
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
        let result = tool
            .execute(json!({ "action": "read", "id": "test-skill" }))
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.result["id"], "test-skill");
        assert!(
            result.result["content"]
                .as_str()
                .unwrap()
                .contains("Test Skill Content")
        );
    }

    #[tokio::test]
    async fn test_read_skill_not_found() {
        let tool = SkillTool::new(create_mock_provider());
        let result = tool
            .execute(json!({ "action": "read", "id": "nonexistent" }))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_create_action_is_not_supported() {
        let tool = SkillTool::new(create_mock_provider());
        let result = tool
            .execute(json!({
                "action": "create",
                "id": "new",
                "name": "New",
                "content": "# New"
            }))
            .await;

        let err = result.expect_err("expected unsupported action error");
        assert!(err.to_string().contains("unknown variant"));
    }

    #[tokio::test]
    async fn builder_registered_skill_tool_is_read_only() {
        let registry = crate::impls::ToolRegistryBuilder::new()
            .with_skill_tool(create_mock_provider())
            .build();

        let schema = registry
            .get("skill")
            .expect("skill tool should be registered")
            .parameters_schema();
        let actions = schema["properties"]["action"]["enum"]
            .as_array()
            .expect("action enum should be present");
        assert!(
            !actions
                .iter()
                .any(|action| action.as_str() == Some("create")),
            "builder-registered skill tool must not expose write actions"
        );

        let result = registry
            .execute_safe(
                "skill",
                json!({
                    "action": "create",
                    "id": "new",
                    "name": "New",
                    "content": "# New"
                }),
            )
            .await;

        let err = result.expect_err("expected builder-registered skill create to fail");
        assert!(err.to_string().contains("unknown variant"));
    }
}
