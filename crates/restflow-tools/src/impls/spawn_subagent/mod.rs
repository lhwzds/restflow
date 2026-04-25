//! spawn_subagent tool - Spawn a sub-agent to work on a task in parallel.

mod routing;
mod schema;
mod types;

#[cfg(test)]
mod tests;

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

use crate::{Result, Tool, ToolError, ToolOutput};
use restflow_traits::store::TeamTemplateStore;
use restflow_traits::{AgentOperationAssessor, normalize_legacy_approval_replay};
use restflow_traits::{SubagentManager, subagent::SubagentDefSummary};

pub use types::SpawnSubagentParams;
use types::SpawnSubagentParams as ParsedSpawnSubagentParams;

/// spawn_subagent tool for the shared agent execution engine.
pub struct SpawnSubagentTool {
    manager: Arc<dyn SubagentManager>,
    team_template_store: Option<Arc<dyn TeamTemplateStore>>,
    assessor: Option<Arc<dyn AgentOperationAssessor>>,
}

impl SpawnSubagentTool {
    pub fn new(manager: Arc<dyn SubagentManager>) -> Self {
        Self {
            manager,
            team_template_store: None,
            assessor: None,
        }
    }

    pub fn with_team_template_store(mut self, store: Arc<dyn TeamTemplateStore>) -> Self {
        self.team_template_store = Some(store);
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

#[async_trait]
impl Tool for SpawnSubagentTool {
    fn name(&self) -> &str {
        "spawn_subagent"
    }

    fn description(&self) -> &str {
        "Spawn a specialized sub-agent to work on a task in parallel. Use wait_subagents to check completion."
    }

    fn parameters_schema(&self) -> Value {
        schema::parameters_schema(&self.available_agents())
    }

    async fn execute(&self, mut input: Value) -> Result<ToolOutput> {
        normalize_legacy_approval_replay(&mut input);
        let params: ParsedSpawnSubagentParams = serde_json::from_value(input)
            .map_err(|e| ToolError::Tool(format!("Invalid parameters: {}", e)))?;
        routing::execute(self, params).await
    }
}
