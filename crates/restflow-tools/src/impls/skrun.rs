//! skrun executable skill runtime tool.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::task;

use crate::security::{SecurityGate, ToolAction};
use crate::{Result, Tool, ToolOutput, check_security};

#[derive(Debug, Deserialize)]
struct RunSkillInput {
    id: String,
    #[serde(default)]
    input: Option<Value>,
}

#[derive(Clone)]
pub struct RunSkillTool {
    root: Option<PathBuf>,
    timeout: Duration,
    security_gate: Option<Arc<dyn SecurityGate>>,
    agent_id: Option<String>,
    task_id: Option<String>,
}

impl Default for RunSkillTool {
    fn default() -> Self {
        Self::new()
    }
}

impl RunSkillTool {
    pub fn new() -> Self {
        Self {
            root: None,
            timeout: Duration::from_secs(60),
            security_gate: None,
            agent_id: None,
            task_id: None,
        }
    }

    pub fn with_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.root = Some(root.into());
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
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
impl Tool for RunSkillTool {
    fn name(&self) -> &str {
        "run_skill"
    }

    fn description(&self) -> &str {
        "Run an installed skrun executable skill by id. Input is passed as one JSON object."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Installed skrun skill id."
                },
                "input": {
                    "type": "object",
                    "description": "JSON object passed to the executable skill.",
                    "additionalProperties": true
                }
            },
            "required": ["id"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: RunSkillInput = serde_json::from_value(input)?;
        let skill_input = params.input.unwrap_or_else(|| json!({}));
        if !skill_input.is_object() {
            return Ok(ToolOutput::error("skrun skill input must be a JSON object"));
        }

        if let Some(message) = check_security(
            self.security_gate.as_deref(),
            ToolAction {
                tool_name: self.name().to_string(),
                operation: "run".to_string(),
                target: params.id.clone(),
                summary: format!("Run skrun skill '{}'", params.id),
            },
            self.agent_id.as_deref(),
            self.task_id.as_deref(),
        )
        .await?
        {
            return Ok(ToolOutput::error(message));
        }

        let skill_id = params.id.clone();
        let root = self.root.clone();
        let timeout = self.timeout;
        let output = match task::spawn_blocking(move || {
            let skills_root = match root {
                Some(root) => root,
                None => skrun::default_skills_dir()?,
            };
            let options = skrun::RunOptions {
                timeout,
                ..Default::default()
            };
            skrun::run_skill(skills_root.join(&skill_id), skill_input, &options)
        })
        .await
        {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => {
                return Ok(ToolOutput::error(format!(
                    "skrun skill '{}' failed: {error:#}",
                    params.id
                )));
            }
            Err(error) => {
                return Ok(ToolOutput::error(format!(
                    "skrun skill '{}' task failed: {error}",
                    params.id
                )));
            }
        };

        Ok(ToolOutput::success(json!({
            "skill_id": params.id,
            "output": output.value,
            "stderr": output.stderr,
            "exit_code": output.exit_code,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn schema_requires_skill_id() {
        let schema = RunSkillTool::new().parameters_schema();
        assert_eq!(schema["required"][0], "id");
    }

    #[tokio::test]
    async fn missing_skill_returns_tool_error() {
        let tool = RunSkillTool::new().with_root("/path/to/missing/skills");

        let output = tool
            .execute(json!({
                "id": "missing",
                "input": {}
            }))
            .await
            .unwrap();

        assert!(!output.success);
        assert!(output.error.unwrap().contains("failed"));
    }
}
