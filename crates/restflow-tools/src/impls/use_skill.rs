//! use_skill tool - Query and load skills dynamically.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

use crate::security::{SecurityGate, ToolAction};
use crate::{Result, ToolError};
use crate::{Tool, ToolOutput, check_security};
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
    security_gate: Option<Arc<dyn SecurityGate>>,
    agent_id: Option<String>,
    task_id: Option<String>,
}

impl UseSkillTool {
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

    async fn ensure_allowed(&self, action: ToolAction) -> Result<Option<String>> {
        check_security(
            self.security_gate.as_deref(),
            action,
            self.agent_id.as_deref(),
            self.task_id.as_deref(),
        )
        .await
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
            if let Some(message) = self
                .ensure_allowed(ToolAction {
                    tool_name: "use_skill".to_string(),
                    operation: "list".to_string(),
                    target: "*".to_string(),
                    summary: "List available skills".to_string(),
                })
                .await?
            {
                return Ok(ToolOutput::error(message));
            }

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

        if let Some(message) = self
            .ensure_allowed(ToolAction {
                tool_name: "use_skill".to_string(),
                operation: "load".to_string(),
                target: skill_id.clone(),
                summary: format!("Load skill '{}'", skill_id),
            })
            .await?
        {
            return Ok(ToolOutput::error(message));
        }

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
    use crate::security::{SecurityDecision, SecurityGate, ToolAction};
    use async_trait::async_trait;
    use restflow_traits::skill::{SkillContent, SkillInfo, SkillRecord, SkillUpdate};
    use std::sync::{Arc, Mutex};

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

    struct RecordingGate {
        calls: Arc<Mutex<Vec<ToolAction>>>,
    }

    impl RecordingGate {
        fn new() -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn calls(&self) -> Arc<Mutex<Vec<ToolAction>>> {
            self.calls.clone()
        }
    }

    #[async_trait]
    impl SecurityGate for RecordingGate {
        async fn check_command(
            &self,
            _: &str,
            _: &str,
            _: &str,
            _: Option<&str>,
        ) -> crate::Result<SecurityDecision> {
            Ok(SecurityDecision::allowed(None))
        }

        async fn check_tool_action(
            &self,
            action: &ToolAction,
            _: Option<&str>,
            _: Option<&str>,
        ) -> crate::Result<SecurityDecision> {
            self.calls.lock().unwrap().push(action.clone());
            Ok(SecurityDecision::blocked(Some("blocked".into())))
        }
    }

    #[tokio::test]
    async fn test_security_gate_blocks_use_skill() {
        let gate = Arc::new(RecordingGate::new());
        let calls = gate.calls();
        let tool =
            UseSkillTool::new(Arc::new(MockProvider)).with_security(gate, "agent-1", "task-1");
        let result = tool.execute(json!({"list": true})).await.unwrap();
        assert!(
            !result.success,
            "security gate should block execution and return error"
        );
        let recorded = calls.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].operation, "list");
    }
}
