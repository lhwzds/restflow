//! spawn_subagent_batch tool - Batch spawn sub-agents and manage reusable team presets.

mod resolve;
mod schema;
mod spawn_exec;
mod team;
mod types;
mod validate;

#[cfg(test)]
mod tests;

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

use crate::impls::operation_assessment::{enforce_confirmation_or_defer, preview_output};
use crate::{Result, Tool, ToolError, ToolOutput};
use restflow_contracts::request::SubagentSpawnRequest as ContractSubagentSpawnRequest;
use restflow_traits::{AgentOperationAssessor, normalize_legacy_approval_replay};
use restflow_traits::store::KvStore;
use restflow_traits::{SubagentManager, subagent::SubagentDefSummary};

use self::resolve::preview_request_from_spec;
use types::SpawnSubagentBatchParams as ParsedSpawnSubagentBatchParams;
pub use types::{BatchSubagentSpec, SpawnSubagentBatchOperation};

/// spawn_subagent_batch tool for shared agent execution engine.
pub struct SpawnSubagentBatchTool {
    manager: Arc<dyn SubagentManager>,
    kv_store: Option<Arc<dyn KvStore>>,
    assessor: Option<Arc<dyn AgentOperationAssessor>>,
}

impl SpawnSubagentBatchTool {
    pub fn new(manager: Arc<dyn SubagentManager>) -> Self {
        Self {
            manager,
            kv_store: None,
            assessor: None,
        }
    }

    pub fn with_kv_store(mut self, kv_store: Arc<dyn KvStore>) -> Self {
        self.kv_store = Some(kv_store);
        self
    }

    pub fn with_assessor(mut self, assessor: Arc<dyn AgentOperationAssessor>) -> Self {
        self.assessor = Some(assessor);
        self
    }

    fn available_agents(&self) -> Vec<SubagentDefSummary> {
        self.manager.list_callable()
    }
}

fn assessment_requests_for_specs(
    _tool: &SpawnSubagentBatchTool,
    specs: &[BatchSubagentSpec],
) -> Result<Vec<ContractSubagentSpawnRequest>> {
    Ok(specs.iter().map(preview_request_from_spec).collect())
}

#[async_trait]
impl Tool for SpawnSubagentBatchTool {
    fn name(&self) -> &str {
        "spawn_subagent_batch"
    }

    fn description(&self) -> &str {
        "Batch spawn sub-agents with model/count specs, and optionally save/reuse named team presets."
    }

    fn parameters_schema(&self) -> Value {
        schema::parameters_schema()
    }

    async fn execute(&self, mut input: Value) -> Result<ToolOutput> {
        normalize_legacy_approval_replay(&mut input);
        let params: ParsedSpawnSubagentBatchParams = serde_json::from_value(input)
            .map_err(|err| ToolError::Tool(format!("Invalid parameters: {}", err)))?;

        match params.operation {
            SpawnSubagentBatchOperation::Spawn => spawn_exec::spawn_batch(self, params).await,
            SpawnSubagentBatchOperation::SaveTeam => {
                let team_name = params
                    .team
                    .as_deref()
                    .ok_or_else(|| ToolError::Tool("save_team requires 'team'.".to_string()))?;
                let specs = params.specs.ok_or_else(|| {
                    ToolError::Tool("save_team requires non-empty 'specs'.".to_string())
                })?;
                validate::validate_save_team_request(
                    params.task.as_deref(),
                    params.tasks.as_deref(),
                    &specs,
                )?;
                validate::validate_structural_specs(self, &specs)?;
                if let Some(assessor) = &self.assessor {
                    let assessment = assessor
                        .assess_subagent_batch(
                            "save_team",
                            assessment_requests_for_specs(self, &specs)?,
                            true,
                        )
                        .await?;
                    if params.preview {
                        return Ok(preview_output(assessment));
                    }
                    if let Some(output) =
                        enforce_confirmation_or_defer(&assessment, params.approval_id.as_deref())?
                    {
                        return Ok(output);
                    }
                } else if params.preview {
                    return Err(ToolError::Tool(
                        "Sub-agent capability preview is unavailable in this runtime.".to_string(),
                    ));
                }
                let payload = team::save_team_specs(self, team_name, &specs)?;
                Ok(ToolOutput::success(serde_json::json!({
                    "operation": "save_team",
                    "result": payload
                })))
            }
            SpawnSubagentBatchOperation::ListTeams => team::list_teams(self),
            SpawnSubagentBatchOperation::GetTeam => {
                let team_name = params
                    .team
                    .as_deref()
                    .ok_or_else(|| ToolError::Tool("get_team requires 'team'.".to_string()))?;
                team::get_team(self, team_name)
            }
            SpawnSubagentBatchOperation::DeleteTeam => {
                let team_name = params
                    .team
                    .as_deref()
                    .ok_or_else(|| ToolError::Tool("delete_team requires 'team'.".to_string()))?;
                team::delete_team(self, team_name)
            }
        }
    }
}
