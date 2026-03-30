use std::sync::Arc;

use restflow_traits::{SubagentCompletion, SubagentManager, SubagentState};

use crate::{Result, ToolError};

#[derive(Clone)]
pub(crate) struct SubagentReadCapabilityService {
    manager: Arc<dyn SubagentManager>,
}

impl SubagentReadCapabilityService {
    pub(crate) fn new(manager: Arc<dyn SubagentManager>) -> Self {
        Self { manager }
    }

    pub(crate) fn list_running_for_parent(
        &self,
        parent_run_id: Option<&str>,
    ) -> Vec<SubagentState> {
        let Some(parent_run_id) = parent_run_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Vec::new();
        };

        self.manager.list_running_for_parent(parent_run_id)
    }

    pub(crate) fn running_count_for_parent(&self, parent_run_id: Option<&str>) -> usize {
        self.list_running_for_parent(parent_run_id).len()
    }

    pub(crate) async fn wait_for_parent_owned_task(
        &self,
        task_id: &str,
        parent_run_id: Option<&str>,
    ) -> Result<Option<SubagentCompletion>> {
        let Some(parent_run_id) = parent_run_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Err(ToolError::Tool(
                "parent_run_id is required for wait_subagents.".to_string(),
            ));
        };

        Ok(self
            .manager
            .wait_for_parent_owned_task(task_id, parent_run_id)
            .await)
    }
}
