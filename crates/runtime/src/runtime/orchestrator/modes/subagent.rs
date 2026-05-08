use crate::runtime::orchestrator::kernel::{ExecutionKernel, map_anyhow_error};
use types::{ExecutionOutcome, ExecutionPlan};

pub async fn run_plan(
    kernel: &ExecutionKernel,
    plan: ExecutionPlan,
) -> std::result::Result<ExecutionOutcome, types::ToolError> {
    kernel
        .backend()
        .execute_subagent_plan(plan)
        .await
        .map_err(map_anyhow_error)
}
