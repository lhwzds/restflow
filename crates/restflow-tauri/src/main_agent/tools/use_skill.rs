//! use_skill tool - Load and activate a skill.

use crate::main_agent::MainAgent;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use ts_rs::TS;

/// Parameters for use_skill tool
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UseSkillParams {
    /// Skill ID to load
    pub skill_id: Option<String>,

    /// If true, list available skills instead of loading
    #[serde(default)]
    pub list: bool,
}

/// use_skill tool for the main agent
#[allow(dead_code)]
pub struct UseSkillTool {
    main_agent: Arc<MainAgent>,
}

impl UseSkillTool {
    /// Create a new use_skill tool
    pub fn new(main_agent: Arc<MainAgent>) -> Self {
        Self { main_agent }
    }

    /// Get tool name
    pub fn name(&self) -> &str {
        "use_skill"
    }

    /// Get tool description
    pub fn description(&self) -> &str {
        "Load a skill to gain specialized capabilities. \
         Skills provide domain-specific instructions and may restrict available tools."
    }

    /// Get JSON schema for parameters
    pub fn parameters_schema(&self) -> Value {
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

    /// Execute the tool
    pub async fn execute(&self, input: Value) -> Result<Value> {
        let params: UseSkillParams =
            serde_json::from_value(input).map_err(|e| anyhow!("Invalid parameters: {}", e))?;

        if params.list {
            // List available skills
            // TODO: Implement skill registry
            return Ok(json!({
                "available_skills": [],
                "message": "Skill registry not yet implemented"
            }));
        }

        let skill_id = params
            .skill_id
            .ok_or_else(|| anyhow!("Missing 'skill_id' parameter"))?;

        // TODO: Load skill from storage
        // For now, return a placeholder response
        Ok(json!({
            "loaded": false,
            "skill_id": skill_id,
            "message": "Skill loading not yet implemented"
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_params_list() {
        let json = r#"{"list": true}"#;
        let params: UseSkillParams = serde_json::from_str(json).unwrap();
        assert!(params.list);
        assert!(params.skill_id.is_none());
    }

    #[test]
    fn test_params_load() {
        let json = r#"{"skill_id": "api-testing"}"#;
        let params: UseSkillParams = serde_json::from_str(json).unwrap();
        assert!(!params.list);
        assert_eq!(params.skill_id, Some("api-testing".to_string()));
    }
}
