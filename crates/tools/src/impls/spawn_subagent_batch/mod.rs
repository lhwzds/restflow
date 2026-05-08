//! spawn_subagent_batch tool - Batch spawn sub-agents.

mod resolve;
mod schema;
mod spawn_exec;
pub(crate) mod types;
mod validate;

#[cfg(test)]
mod tests;

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

use crate::{Result, Tool, ToolError, ToolOutput};
use ::types::AgentOperationAssessor;
use ::types::{SubagentManager, subagent::SubagentDefSummary};

use types::SpawnSubagentBatchParams as ParsedSpawnSubagentBatchParams;
pub use types::{BatchSubagentSpec, SpawnSubagentBatchOperation};

/// spawn_subagent_batch tool for shared agent execution engine.
pub struct SpawnSubagentBatchTool {
    manager: Arc<dyn SubagentManager>,
    assessor: Option<Arc<dyn AgentOperationAssessor>>,
}

impl SpawnSubagentBatchTool {
    pub fn new(manager: Arc<dyn SubagentManager>) -> Self {
        Self {
            manager,
            assessor: None,
        }
    }

    pub fn with_assessor(mut self, assessor: Arc<dyn AgentOperationAssessor>) -> Self {
        self.assessor = Some(assessor);
        self
    }

    fn available_agents(&self) -> Vec<SubagentDefSummary> {
        self.manager.list_callable()
    }
}

#[async_trait]
impl Tool for SpawnSubagentBatchTool {
    fn name(&self) -> &str {
        "spawn_subagent_batch"
    }

    fn description(&self) -> &str {
        "Batch spawn sub-agents with explicit model/count specs."
    }

    fn parameters_schema(&self) -> Value {
        schema::parameters_schema()
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: ParsedSpawnSubagentBatchParams = serde_json::from_value(input)
            .map_err(|err| ToolError::Tool(format!("Invalid parameters: {}", err)))?;

        match params.operation {
            SpawnSubagentBatchOperation::Spawn => spawn_exec::spawn_batch(self, params).await,
        }
    }
}
