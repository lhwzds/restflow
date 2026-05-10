//! load_skill tool - Query and load skills dynamically.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

use crate::{Result, ToolError};
use crate::{SecurityGate, ToolAction};
use crate::{Tool, ToolOutput, check_security};
use types::skill::SkillProvider;

/// Parameters for load_skill tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadSkillParams {
    /// Explicit action for load-only operations.
    pub action: Option<String>,

    /// Skill ID to load.
    pub id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoadSkillAction {
    List,
    Read,
}

/// load_skill tool — lets the LLM query available skills and load their content.
pub struct LoadSkillTool {
    provider: Arc<dyn SkillProvider>,
    security_gate: Option<Arc<dyn SecurityGate>>,
    agent_id: Option<String>,
    task_id: Option<String>,
}

impl LoadSkillTool {
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
impl Tool for LoadSkillTool {
    fn name(&self) -> &str {
        "load_skill"
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
                "id": {
                    "type": "string",
                    "description": "Skill ID for read action."
                }
            },
            "additionalProperties": false
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: LoadSkillParams = serde_json::from_value(input)
            .map_err(|e| ToolError::Tool(format!("Invalid parameters: {}", e)))?;

        let action = match params.action.as_deref().map(str::trim) {
            Some(raw) if raw.eq_ignore_ascii_case("list") => Ok(LoadSkillAction::List),
            Some(raw) if raw.eq_ignore_ascii_case("read") => Ok(LoadSkillAction::Read),
            Some(raw) if raw.eq_ignore_ascii_case("execute") || raw.eq_ignore_ascii_case("run") => {
                Err(ToolOutput::error(
                    "skill execution not supported in this tool. load_skill is load-only; use action=list/read.",
                ))
            }
            Some(raw) => Err(ToolOutput::error(format!(
                "Unsupported action '{}'. load_skill supports only load-only actions: list, read.",
                raw
            ))),
            None => Err(ToolOutput::error(
                "Missing action. load_skill is load-only and requires action=list/read.",
            )),
        };

        let action = match action {
            Ok(action) => action,
            Err(output) => return Ok(output),
        };

        if action == LoadSkillAction::List {
            if let Some(message) = self
                .ensure_allowed(ToolAction {
                    tool_name: "load_skill".to_string(),
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
                        "kind": info.kind,
                        "executable": info.executable,
                        "suggested_tools": info.suggested_tools,
                        "source": info.source,
                        "read_only": info.read_only,
                        "source_ref": info.source_ref,
                    })
                })
                .collect();

            return Ok(ToolOutput::success(json!({
                "available_skills": skills,
                "count": skills.len(),
            })));
        }

        let skill_id = params
            .id
            .ok_or_else(|| ToolError::Tool("Missing 'id' parameter".to_string()))?;

        if let Some(message) = self
            .ensure_allowed(ToolAction {
                tool_name: "load_skill".to_string(),
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
                "kind": content.kind,
                "executable": content.executable,
                "suggested_tools": content.suggested_tools,
                "source": content.source,
                "read_only": content.read_only,
                "source_ref": content.source_ref,
            }))),
            None => Ok(ToolOutput::error(format!("Skill '{}' not found", skill_id))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SecurityDecision, SecurityGate, ToolAction};
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};
    use types::skill::{SkillContent, SkillInfo, SkillSource};

    struct MockProvider;

    impl SkillProvider for MockProvider {
        fn list_skills(&self) -> Vec<SkillInfo> {
            vec![SkillInfo {
                id: "test-skill".to_string(),
                name: "Test Skill".to_string(),
                description: Some("A test skill".to_string()),
                tags: None,
                kind: None,
                executable: false,
                suggested_tools: Vec::new(),
                source: SkillSource::User,
                read_only: false,
                source_ref: None,
            }]
        }

        fn get_skill(&self, id: &str) -> Option<SkillContent> {
            if id == "test-skill" {
                Some(SkillContent {
                    id: "test-skill".to_string(),
                    name: "Test Skill".to_string(),
                    content: "# Test Skill\nDo something useful.".to_string(),
                    kind: None,
                    executable: false,
                    suggested_tools: Vec::new(),
                    source: SkillSource::User,
                    read_only: false,
                    source_ref: None,
                })
            } else {
                None
            }
        }

        fn export_skill(&self, _: &str) -> std::result::Result<String, String> {
            Err("not implemented".to_string())
        }
    }

    #[test]
    fn test_params_list() {
        let params: LoadSkillParams = serde_json::from_str(r#"{"action": "list"}"#).unwrap();
        assert_eq!(params.action.as_deref(), Some("list"));
        assert!(params.id.is_none());
    }

    #[test]
    fn test_params_read() {
        let params: LoadSkillParams =
            serde_json::from_str(r#"{"action": "read", "id": "api-testing"}"#).unwrap();
        assert_eq!(params.action.as_deref(), Some("read"));
        assert_eq!(params.id.as_deref(), Some("api-testing"));
    }

    #[tokio::test]
    async fn test_list_skills() {
        let tool = LoadSkillTool::new(Arc::new(MockProvider));
        let result = tool.execute(json!({"action": "list"})).await.unwrap();
        assert!(result.success);
        assert_eq!(result.result["count"], 1);
        assert_eq!(result.result["available_skills"][0]["id"], "test-skill");
    }

    #[tokio::test]
    async fn test_load_skill() {
        let tool = LoadSkillTool::new(Arc::new(MockProvider));
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
        let tool = LoadSkillTool::new(Arc::new(MockProvider));
        let result = tool
            .execute(json!({"action": "read", "id": "nonexistent"}))
            .await
            .unwrap();
        assert!(!result.success);
    }

    #[tokio::test]
    async fn test_load_skill_missing_id() {
        let tool = LoadSkillTool::new(Arc::new(MockProvider));
        // Missing explicit action and id field.
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
        fn export_skill(&self, _: &str) -> std::result::Result<String, String> {
            Err("not implemented".to_string())
        }
    }

    #[tokio::test]
    async fn test_list_skills_empty() {
        let tool = LoadSkillTool::new(Arc::new(EmptyProvider));
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
    async fn test_security_gate_blocks_load_skill() {
        let gate = Arc::new(RecordingGate::new());
        let calls = gate.calls();
        let tool =
            LoadSkillTool::new(Arc::new(MockProvider)).with_security(gate, "agent-1", "task-1");
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
    async fn test_reject_execute_action() {
        let tool = LoadSkillTool::new(Arc::new(MockProvider));
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
        let tool = LoadSkillTool::new(Arc::new(MockProvider));
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
