use crate::boundary::codec::{from_contract, to_contract};
use crate::models::{TaskPatch, TaskSpec};
use types::request::{
    TaskFromSessionRequest as ContractTaskFromSessionRequest, TaskPatch as ContractTaskPatch,
    TaskSpec as ContractTaskSpec,
};
use types::store::TaskConvertSessionRequest;

type CoreTaskPatch = TaskPatch;
type CoreTaskSpec = TaskSpec;

pub(crate) fn contract_task_spec_to_core_task_spec(
    spec: ContractTaskSpec,
) -> anyhow::Result<CoreTaskSpec> {
    from_contract(spec)
}

pub(crate) fn contract_spec_to_core(spec: ContractTaskSpec) -> anyhow::Result<CoreTaskSpec> {
    contract_task_spec_to_core_task_spec(spec)
}

pub(crate) fn core_task_spec_to_contract_task_spec(
    spec: CoreTaskSpec,
) -> anyhow::Result<ContractTaskSpec> {
    to_contract(spec)
}

pub(crate) fn core_spec_to_contract(spec: CoreTaskSpec) -> anyhow::Result<ContractTaskSpec> {
    core_task_spec_to_contract_task_spec(spec)
}

pub(crate) fn contract_task_patch_to_core_task_patch(
    patch: ContractTaskPatch,
) -> anyhow::Result<CoreTaskPatch> {
    from_contract(patch)
}

pub(crate) fn contract_patch_to_core(patch: ContractTaskPatch) -> anyhow::Result<CoreTaskPatch> {
    contract_task_patch_to_core_task_patch(patch)
}

#[allow(dead_code)]
pub(crate) fn resolve_spec_agent_id<E, ResolveAgentId>(
    mut spec: CoreTaskSpec,
    mut resolve_agent_id: ResolveAgentId,
) -> Result<CoreTaskSpec, E>
where
    ResolveAgentId: FnMut(&str) -> Result<String, E>,
{
    spec.agent_id = resolve_agent_id(&spec.agent_id)?;
    Ok(spec)
}

#[allow(dead_code)]
pub(crate) fn resolve_patch_agent_id<E, ResolveAgentId>(
    mut patch: CoreTaskPatch,
    mut resolve_agent_id: ResolveAgentId,
) -> Result<CoreTaskPatch, E>
where
    ResolveAgentId: FnMut(&str) -> Result<String, E>,
{
    if let Some(agent_id) = patch.agent_id.as_deref() {
        patch.agent_id = Some(resolve_agent_id(agent_id)?);
    }
    Ok(patch)
}

pub(crate) fn core_task_patch_to_contract_task_patch(
    patch: CoreTaskPatch,
) -> anyhow::Result<ContractTaskPatch> {
    to_contract(patch)
}

pub(crate) fn core_patch_to_contract(patch: CoreTaskPatch) -> anyhow::Result<ContractTaskPatch> {
    core_task_patch_to_contract_task_patch(patch)
}

pub(crate) fn contract_task_from_session_request_to_store(
    request: ContractTaskFromSessionRequest,
) -> anyhow::Result<TaskConvertSessionRequest> {
    Ok(TaskConvertSessionRequest {
        session_id: request.session_id,
        name: request.name,
        schedule: request.schedule,
        input: request.input,
        timeout_secs: request.timeout_secs,
        resource_limits: request.resource_limits,
        run_now: request.run_now,
        preview: false,
        approval_id: None,
    })
}

pub(crate) fn contract_convert_request_to_store(
    request: ContractTaskFromSessionRequest,
) -> anyhow::Result<TaskConvertSessionRequest> {
    contract_task_from_session_request_to_store(request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ContinuationConfig, ExecutionMode, ResourceLimits};

    #[test]
    fn contract_spec_to_core_preserves_task_defaults() {
        let contract: ContractTaskSpec = serde_json::from_value(serde_json::json!({
            "name": "nightly",
            "agent_id": "agent-1",
            "schedule": {
                "type": "interval",
                "interval_ms": 60000,
                "start_at": null
            },
            "execution_mode": {
                "type": "api"
            },
            "memory": {},
            "resource_limits": {},
            "continuation": {}
        }))
        .expect("contract background spec");

        let core = contract_spec_to_core(contract).expect("core background spec");

        assert_eq!(core.execution_mode, Some(ExecutionMode::Api));

        assert_eq!(
            core.resource_limits.expect("resource limits"),
            ResourceLimits::default()
        );
        assert_eq!(
            core.continuation.expect("continuation"),
            ContinuationConfig::default()
        );
    }

    #[test]
    fn resolve_patch_agent_id_resolves_present_id() {
        let patch = resolve_patch_agent_id(
            TaskPatch {
                agent_id: Some("agent-123".to_string()),
                ..TaskPatch::default()
            },
            |value| Ok::<_, &'static str>(format!("resolved:{value}")),
        )
        .expect("patch should resolve id");

        assert_eq!(patch.agent_id.as_deref(), Some("resolved:agent-123"));
    }
}
