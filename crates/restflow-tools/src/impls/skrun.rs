//! skrun executable skill runtime tool.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

use crate::security::{SecurityGate, ToolAction};
use crate::{Result, Tool, ToolOutput, check_security};

const RESTFLOW_SKRUN_BIN_ENV: &str = "RESTFLOW_SKRUN_BIN";

#[derive(Debug, Deserialize)]
struct RunSkillInput {
    id: String,
    #[serde(default)]
    input: Option<Value>,
}

#[derive(Clone)]
pub struct RunSkillTool {
    bin: PathBuf,
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
            bin: std::env::var_os(RESTFLOW_SKRUN_BIN_ENV)
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("skrun")),
            timeout: Duration::from_secs(60),
            security_gate: None,
            agent_id: None,
            task_id: None,
        }
    }

    pub fn with_bin(mut self, bin: impl Into<PathBuf>) -> Self {
        self.bin = bin.into();
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

        let mut command = Command::new(&self.bin);
        command
            .arg("skill")
            .arg("run")
            .arg("--id")
            .arg(&params.id)
            .arg("--input")
            .arg(serde_json::to_string(&skill_input)?);

        let output = match timeout(self.timeout, command.output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => {
                return Ok(ToolOutput::error(format!(
                    "failed to launch skrun: {error}"
                )));
            }
            Err(_) => {
                return Ok(ToolOutput::error(format!(
                    "skrun skill '{}' timed out after {}s",
                    params.id,
                    self.timeout.as_secs()
                )));
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if !output.status.success() {
            let message = if stderr.is_empty() { stdout } else { stderr };
            return Ok(ToolOutput::error(format!(
                "skrun skill '{}' failed: {}",
                params.id, message
            )));
        }

        let value = serde_json::from_str::<Value>(&stdout).unwrap_or_else(|_| json!(stdout));
        Ok(ToolOutput::success(json!({
            "skill_id": params.id,
            "output": value,
            "stderr": stderr,
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
        let tool = RunSkillTool::new().with_bin("/path/to/missing/skrun");

        let output = tool
            .execute(json!({
                "id": "missing",
                "input": {}
            }))
            .await
            .unwrap();

        assert!(!output.success);
        assert!(output.error.unwrap().contains("failed to launch skrun"));
    }
}
