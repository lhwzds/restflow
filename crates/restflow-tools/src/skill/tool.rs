//! SkillAsTool: wraps a Skill as a dynamic Tool.

use async_trait::async_trait;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::Result;
use crate::security::{SecurityGate, ToolAction};
use crate::skill::{SkillInfo, SkillProvider};
use crate::{Tool, ToolOutput, check_security};

/// Wraps a Skill as a Tool so it appears in the LLM's flat tool list.
///
/// When the AI calls this tool, it receives the skill's content, which
/// it can then follow as instructions.
pub struct SkillAsTool {
    info: SkillInfo,
    provider: Arc<dyn SkillProvider>,
    security_gate: Option<Arc<dyn SecurityGate>>,
    agent_id: Option<String>,
    task_id: Option<String>,
}

impl SkillAsTool {
    pub fn new(info: SkillInfo, provider: Arc<dyn SkillProvider>) -> Self {
        Self {
            info,
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
impl Tool for SkillAsTool {
    #[allow(clippy::misnamed_getters)]
    fn name(&self) -> &str {
        &self.info.id
    }

    fn description(&self) -> &str {
        self.info.description.as_deref().unwrap_or(&self.info.name)
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "input": {
                    "type": "string",
                    "description": "Optional context input for the skill"
                }
            }
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let user_input = input.get("input").and_then(|v| v.as_str()).unwrap_or("");

        let action = ToolAction {
            tool_name: self.info.id.clone(),
            operation: "load".to_string(),
            target: self.info.id.clone(),
            summary: format!("Load skill '{}'", self.info.name),
        };

        if let Some(message) = check_security(
            self.security_gate.as_deref(),
            action,
            self.agent_id.as_deref(),
            self.task_id.as_deref(),
        )
        .await?
        {
            return Ok(ToolOutput::error(message));
        }

        match self.provider.get_skill(&self.info.id) {
            Some(content) => Ok(ToolOutput::success(json!({
                "skill_id": content.id,
                "name": content.name,
                "content": content.content,
                "input": user_input,
            }))),
            None => Ok(ToolOutput::error(format!(
                "Skill '{}' not found",
                self.info.id
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skill::*;

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

        fn create_skill(&self, _skill: SkillRecord) -> std::result::Result<SkillRecord, String> {
            Err("not implemented".to_string())
        }
        fn update_skill(
            &self,
            _id: &str,
            _update: SkillUpdate,
        ) -> std::result::Result<SkillRecord, String> {
            Err("not implemented".to_string())
        }
        fn delete_skill(&self, _id: &str) -> std::result::Result<bool, String> {
            Err("not implemented".to_string())
        }
        fn export_skill(&self, _id: &str) -> std::result::Result<String, String> {
            Err("not implemented".to_string())
        }
        fn import_skill(
            &self,
            _id: &str,
            _markdown: &str,
            _overwrite: bool,
        ) -> std::result::Result<SkillRecord, String> {
            Err("not implemented".to_string())
        }
    }

    #[test]
    fn test_skill_as_tool_name() {
        let provider = Arc::new(MockProvider);
        let info = provider.list_skills().into_iter().next().unwrap();
        let tool = SkillAsTool::new(info, provider);
        assert_eq!(tool.name(), "test-skill");
        assert_eq!(tool.description(), "A test skill");
    }

    #[tokio::test]
    async fn test_skill_as_tool_execute() {
        let provider = Arc::new(MockProvider);
        let info = provider.list_skills().into_iter().next().unwrap();
        let tool = SkillAsTool::new(info, provider);

        let result = tool.execute(json!({"input": "hello"})).await.unwrap();
        assert!(result.success);
        assert_eq!(result.result["skill_id"], "test-skill");
        assert_eq!(result.result["input"], "hello");
    }

    #[tokio::test]
    async fn test_skill_as_tool_not_found() {
        let provider = Arc::new(MockProvider);
        let info = SkillInfo {
            id: "nonexistent".to_string(),
            name: "Nope".to_string(),
            description: None,
            tags: None,
        };
        let tool = SkillAsTool::new(info, provider);

        let result = tool.execute(json!({})).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("not found"));
    }
}
