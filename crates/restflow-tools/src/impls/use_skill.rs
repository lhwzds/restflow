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
    /// Explicit action for load-only operations.
    pub action: Option<String>,

    /// Skill ID to load.
    pub skill_id: Option<String>,

    /// Skill ID alias for compatibility with skill.read-style input.
    pub id: Option<String>,

    /// If true, list available skills instead of loading.
    #[serde(default)]
    pub list: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UseSkillAction {
    List,
    Read,
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
        "Load-only skill access tool. Supports listing skills and reading skill content. Skill execution is not supported in this tool."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "read"],
                    "description": "Load-only action. Use 'list' to list skills, or 'read' to load skill content."
                },
                "skill_id": {
                    "type": "string",
                    "description": "Legacy input for read action: the skill ID to load."
                },
                "id": {
                    "type": "string",
                    "description": "Skill ID for read action."
                },
                "list": {
                    "type": "boolean",
                    "default": false,
                    "description": "Legacy input for list action."
                }
            },
            "additionalProperties": false
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: UseSkillParams = serde_json::from_value(input)
            .map_err(|e| ToolError::Tool(format!("Invalid parameters: {}", e)))?;

        let action = match params.action.as_deref().map(str::trim) {
            Some(raw) if raw.eq_ignore_ascii_case("list") => Ok(UseSkillAction::List),
            Some(raw) if raw.eq_ignore_ascii_case("read") || raw.eq_ignore_ascii_case("load") => {
                Ok(UseSkillAction::Read)
            }
            Some(raw) if raw.eq_ignore_ascii_case("execute") || raw.eq_ignore_ascii_case("run") => {
                Err(ToolOutput::error(
                    "skill execution not supported in this tool. use_skill is load-only; use action=list/read.",
                ))
            }
            Some(raw) => Err(ToolOutput::error(format!(
                "Unsupported action '{}'. use_skill supports only load-only actions: list, read.",
                raw
            ))),
            None if params.list => Ok(UseSkillAction::List),
            None if params.skill_id.is_some() || params.id.is_some() => Ok(UseSkillAction::Read),
            None => Err(ToolOutput::error(
                "Missing action. use_skill is load-only and requires action=list/read (legacy: list=true or skill_id).",
            )),
        };

        let action = match action {
            Ok(action) => action,
            Err(output) => return Ok(output),
        };

        if action == UseSkillAction::List {
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
            .or(params.id)
            .ok_or_else(|| ToolError::Tool("Missing 'skill_id' parameter".to_string()))?;

        if let Some(message) = self
            .ensure_allowed(ToolAction {
                tool_name: "use_skill".to_string(),
                operation: "read".to_string(),
                target: skill_id.clone(),
                summary: format!("Read skill '{}'", skill_id),
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
        assert!(params.action.is_none());
        assert!(params.skill_id.is_none());
    }

    #[test]
    fn test_params_load() {
        let params: UseSkillParams =
            serde_json::from_str(r#"{"skill_id": "api-testing"}"#).unwrap();
        assert!(!params.list);
        assert!(params.action.is_none());
        assert_eq!(params.skill_id, Some("api-testing".to_string()));
    }

    #[test]
    fn test_params_action_read() {
        let params: UseSkillParams =
            serde_json::from_str(r#"{"action": "read", "id": "x"}"#).unwrap();
        assert_eq!(params.action.as_deref(), Some("read"));
        assert_eq!(params.id.as_deref(), Some("x"));
    }

    #[tokio::test]
    async fn test_list_skills() {
        let tool = UseSkillTool::new(Arc::new(MockProvider));
        let result = tool.execute(json!({"action": "list"})).await.unwrap();
        assert!(result.success);
        assert_eq!(result.result["count"], 1);
        assert_eq!(result.result["available_skills"][0]["id"], "test-skill");
    }

    #[tokio::test]
    async fn test_load_skill() {
        let tool = UseSkillTool::new(Arc::new(MockProvider));
        let result = tool
            .execute(json!({"action": "read", "id": "test-skill"}))
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
            .execute(json!({"action": "read", "skill_id": "nonexistent"}))
            .await
            .unwrap();
        assert!(!result.success);
    }

    #[tokio::test]
    async fn test_load_skill_missing_skill_id() {
        let tool = UseSkillTool::new(Arc::new(MockProvider));
        // Missing explicit action and legacy fallback fields.
        let result = tool.execute(json!({})).await;
        assert!(result.is_ok());
        assert!(!result.unwrap().success);
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
        let result = tool.execute(json!({"action": "list"})).await.unwrap();
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
        let result = tool.execute(json!({"action": "list"})).await.unwrap();
        assert!(
            !result.success,
            "security gate should block execution and return error"
        );
        let recorded = calls.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].operation, "list");
    }

    #[tokio::test]
    async fn test_legacy_list_parameter_still_supported() {
        let tool = UseSkillTool::new(Arc::new(MockProvider));
        let result = tool.execute(json!({"list": true})).await.unwrap();
        assert!(result.success);
        assert_eq!(result.result["count"], 1);
    }

    #[tokio::test]
    async fn test_legacy_skill_id_parameter_still_supported() {
        let tool = UseSkillTool::new(Arc::new(MockProvider));
        let result = tool
            .execute(json!({"skill_id": "test-skill"}))
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.result["skill_id"], "test-skill");
    }

    #[tokio::test]
    async fn test_reject_execute_action() {
        let tool = UseSkillTool::new(Arc::new(MockProvider));
        let result = tool
            .execute(json!({"action": "execute", "id": "test-skill"}))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(
            result
                .error
                .unwrap_or_default()
                .contains("skill execution not supported in this tool")
        );
    }

    #[tokio::test]
    async fn test_reject_run_action() {
        let tool = UseSkillTool::new(Arc::new(MockProvider));
        let result = tool
            .execute(json!({"action": "run", "id": "test-skill"}))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(
            result
                .error
                .unwrap_or_default()
                .contains("skill execution not supported in this tool")
        );
    }
}
