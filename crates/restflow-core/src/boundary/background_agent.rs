use crate::boundary::codec::{from_contract, to_contract};
use crate::models::{
    DurabilityMode, ExecutionMode, MemoryConfig, MemoryScope, NotificationConfig, ResourceLimits,
    TaskControlAction, TaskPatch, TaskSchedule, TaskSpec,
};
use crate::services::background_agent_conversion::default_conversion_schedule;
use restflow_contracts::request::{
    DurabilityMode as ContractDurabilityMode, ExecutionMode as ContractExecutionMode,
    MemoryConfig as ContractMemoryConfig, NotificationConfig as ContractNotificationConfig,
    ResourceLimits as ContractResourceLimits,
    TaskFromSessionRequest as ContractTaskFromSessionRequest, TaskPatch as ContractTaskPatch,
    TaskSchedule as ContractTaskSchedule, TaskSpec as ContractTaskSpec,
};
use restflow_tools::ToolError;
use restflow_traits::store::{
    BackgroundAgentConvertSessionRequest, BackgroundAgentCreateRequest,
    BackgroundAgentUpdateRequest,
};
use serde::Serialize;
use serde::de::DeserializeOwned;

// Phase 2 boundary owner shift: expose task-oriented names as canonical in this file
// while keeping legacy wrappers for unchanged callers.
type CoreTaskControlAction = TaskControlAction;
type CoreTaskPatch = TaskPatch;
type CoreTaskSchedule = TaskSchedule;
type CoreTaskSpec = TaskSpec;
type LegacyStoreTaskCreateRequest = BackgroundAgentCreateRequest;
type LegacyStoreTaskFromSessionRequest = BackgroundAgentConvertSessionRequest;
type LegacyStoreTaskUpdateRequest = BackgroundAgentUpdateRequest;

pub(crate) struct ConvertSessionToTaskOptions {
    pub(crate) name: Option<String>,
    pub(crate) schedule: CoreTaskSchedule,
    pub(crate) input: Option<String>,
    pub(crate) timeout_secs: Option<u64>,
    pub(crate) memory: Option<MemoryConfig>,
    pub(crate) durability_mode: Option<DurabilityMode>,
    pub(crate) resource_limits: Option<ResourceLimits>,
    pub(crate) run_now: bool,
}

pub(crate) type ConvertSessionRequestOptions = ConvertSessionToTaskOptions;

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

pub(crate) fn resolve_agent_id_alias<E, ResolveDefault, ResolveExisting>(
    id_or_alias: &str,
    resolve_default: ResolveDefault,
    resolve_existing: ResolveExisting,
) -> Result<String, E>
where
    ResolveDefault: FnOnce() -> Result<String, E>,
    ResolveExisting: FnOnce(&str) -> Result<String, E>,
{
    let trimmed = id_or_alias.trim();
    if trimmed.eq_ignore_ascii_case("default") {
        resolve_default()
    } else {
        resolve_existing(trimmed)
    }
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

pub(crate) fn core_task_spec_to_store_create_request(
    spec: &CoreTaskSpec,
) -> anyhow::Result<LegacyStoreTaskCreateRequest> {
    Ok(LegacyStoreTaskCreateRequest {
        name: spec.name.clone(),
        agent_id: spec.agent_id.clone(),
        chat_session_id: spec.chat_session_id.clone(),
        schedule: to_contract(spec.schedule.clone())?,
        input: spec.input.clone(),
        input_template: spec.input_template.clone(),
        timeout_secs: spec.timeout_secs,
        durability_mode: spec.durability_mode.clone().map(to_contract).transpose()?,
        memory: spec.memory.clone().map(to_contract).transpose()?,
        memory_scope: None,
        resource_limits: spec.resource_limits.clone().map(to_contract).transpose()?,
        preview: false,
        approval_id: None,
    })
}

pub(crate) fn core_spec_to_create_request(
    spec: &CoreTaskSpec,
) -> anyhow::Result<LegacyStoreTaskCreateRequest> {
    core_task_spec_to_store_create_request(spec)
}

pub(crate) fn core_patch_to_update_request(
    id: String,
    patch: &CoreTaskPatch,
) -> anyhow::Result<LegacyStoreTaskUpdateRequest> {
    Ok(LegacyStoreTaskUpdateRequest {
        id,
        name: patch.name.clone(),
        description: patch.description.clone(),
        agent_id: patch.agent_id.clone(),
        chat_session_id: patch.chat_session_id.clone(),
        input: patch.input.clone(),
        input_template: patch.input_template.clone(),
        schedule: patch.schedule.clone().map(to_contract).transpose()?,
        notification: patch.notification.clone().map(to_contract).transpose()?,
        execution_mode: patch.execution_mode.clone().map(to_contract).transpose()?,
        timeout_secs: patch.timeout_secs,
        durability_mode: patch.durability_mode.clone().map(to_contract).transpose()?,
        memory: patch.memory.clone().map(to_contract).transpose()?,
        memory_scope: None,
        resource_limits: patch.resource_limits.clone().map(to_contract).transpose()?,
        preview: false,
        approval_id: None,
    })
}

pub(crate) fn store_create_request_to_core_task_spec(
    request: LegacyStoreTaskCreateRequest,
) -> Result<CoreTaskSpec, ToolError> {
    let schedule =
        decode_contract::<ContractTaskSchedule, CoreTaskSchedule>("schedule", request.schedule)?;

    Ok(CoreTaskSpec {
        name: request.name,
        agent_id: request.agent_id,
        chat_session_id: request.chat_session_id,
        description: None,
        input: request.input,
        input_template: request.input_template,
        schedule,
        notification: None,
        execution_mode: None,
        timeout_secs: request.timeout_secs,
        memory: merge_memory_scope(
            decode_optional_contract::<ContractMemoryConfig, MemoryConfig>(
                "memory",
                request.memory,
            )?,
            request.memory_scope,
        )?,
        durability_mode: decode_optional_contract::<ContractDurabilityMode, DurabilityMode>(
            "durability_mode",
            request.durability_mode,
        )?,
        resource_limits: decode_optional_contract::<ContractResourceLimits, ResourceLimits>(
            "resource_limits",
            request.resource_limits,
        )?,
        prerequisites: Vec::new(),
        continuation: None,
    })
}

pub(crate) fn create_request_to_spec(
    request: LegacyStoreTaskCreateRequest,
) -> Result<CoreTaskSpec, ToolError> {
    store_create_request_to_core_task_spec(request)
}

pub(crate) fn update_request_to_patch(
    request: LegacyStoreTaskUpdateRequest,
) -> Result<CoreTaskPatch, ToolError> {
    Ok(CoreTaskPatch {
        name: request.name,
        description: request.description,
        agent_id: request.agent_id,
        chat_session_id: request.chat_session_id,
        input: request.input,
        input_template: request.input_template,
        schedule: decode_optional_contract::<ContractTaskSchedule, TaskSchedule>(
            "schedule",
            request.schedule,
        )?,
        notification: decode_optional_contract::<ContractNotificationConfig, NotificationConfig>(
            "notification",
            request.notification,
        )?,
        execution_mode: decode_optional_contract::<ContractExecutionMode, ExecutionMode>(
            "execution_mode",
            request.execution_mode,
        )?,
        timeout_secs: request.timeout_secs,
        memory: merge_memory_scope(
            decode_optional_contract::<ContractMemoryConfig, MemoryConfig>(
                "memory",
                request.memory,
            )?,
            request.memory_scope,
        )?,
        durability_mode: decode_optional_contract::<ContractDurabilityMode, DurabilityMode>(
            "durability_mode",
            request.durability_mode,
        )?,
        resource_limits: decode_optional_contract::<ContractResourceLimits, ResourceLimits>(
            "resource_limits",
            request.resource_limits,
        )?,
        prerequisites: None,
        continuation: None,
    })
}

pub(crate) fn parse_task_control_action(action: &str) -> Result<CoreTaskControlAction, ToolError> {
    match action.trim().to_lowercase().as_str() {
        "start" => Ok(CoreTaskControlAction::Start),
        "pause" => Ok(CoreTaskControlAction::Pause),
        "resume" => Ok(CoreTaskControlAction::Resume),
        "stop" => Ok(CoreTaskControlAction::Stop),
        "run_now" | "run-now" | "runnow" => Ok(CoreTaskControlAction::RunNow),
        value => Err(ToolError::Tool(format!(
            "Unknown control action: {}",
            value
        ))),
    }
}

pub(crate) fn parse_control_action(action: &str) -> Result<CoreTaskControlAction, ToolError> {
    parse_task_control_action(action)
}

pub(crate) fn task_from_session_request_to_options(
    request: LegacyStoreTaskFromSessionRequest,
) -> Result<ConvertSessionToTaskOptions, ToolError> {
    Ok(ConvertSessionToTaskOptions {
        name: request.name,
        schedule: decode_optional_contract::<ContractTaskSchedule, CoreTaskSchedule>(
            "schedule",
            request.schedule,
        )?
        .unwrap_or_else(default_conversion_schedule),
        input: request.input,
        timeout_secs: request.timeout_secs,
        memory: merge_memory_scope(
            decode_optional_contract::<ContractMemoryConfig, MemoryConfig>(
                "memory",
                request.memory,
            )?,
            request.memory_scope,
        )?,
        durability_mode: decode_optional_contract::<ContractDurabilityMode, DurabilityMode>(
            "durability_mode",
            request.durability_mode,
        )?,
        resource_limits: decode_optional_contract::<ContractResourceLimits, ResourceLimits>(
            "resource_limits",
            request.resource_limits,
        )?,
        run_now: request.run_now.unwrap_or(false),
    })
}

pub(crate) fn convert_session_request_to_options(
    request: LegacyStoreTaskFromSessionRequest,
) -> Result<ConvertSessionRequestOptions, ToolError> {
    task_from_session_request_to_options(request)
}

pub(crate) fn contract_task_from_session_request_to_store(
    request: ContractTaskFromSessionRequest,
) -> anyhow::Result<LegacyStoreTaskFromSessionRequest> {
    Ok(LegacyStoreTaskFromSessionRequest {
        session_id: request.session_id,
        name: request.name,
        schedule: request.schedule,
        input: request.input,
        timeout_secs: request.timeout_secs,
        durability_mode: request.durability_mode,
        memory: request.memory,
        memory_scope: request.memory_scope,
        resource_limits: request.resource_limits,
        run_now: request.run_now,
        preview: false,
        approval_id: None,
    })
}

pub(crate) fn contract_convert_request_to_store(
    request: ContractTaskFromSessionRequest,
) -> anyhow::Result<LegacyStoreTaskFromSessionRequest> {
    contract_task_from_session_request_to_store(request)
}

fn decode_contract<T: Serialize, U: DeserializeOwned>(
    field: &str,
    value: T,
) -> Result<U, ToolError> {
    let encoded = serde_json::to_value(value)
        .map_err(|e| ToolError::Tool(format!("Invalid {}: {}", field, e)))?;
    serde_json::from_value(encoded)
        .map_err(|e| ToolError::Tool(format!("Invalid {}: {}", field, e)))
}

fn decode_optional_contract<T: Serialize, U: DeserializeOwned>(
    field: &str,
    value: Option<T>,
) -> Result<Option<U>, ToolError> {
    value.map(|value| decode_contract(field, value)).transpose()
}

fn parse_memory_scope(value: Option<&str>) -> Result<Option<MemoryScope>, ToolError> {
    match value.map(|scope| scope.trim().to_lowercase()) {
        None => Ok(None),
        Some(scope) if scope.is_empty() => Ok(None),
        Some(scope) if scope == "shared_agent" => Ok(Some(MemoryScope::SharedAgent)),
        Some(scope) if scope == "per_task" || scope == "per_background_agent" => {
            Ok(Some(MemoryScope::PerTask))
        }
        Some(scope) => Err(ToolError::Tool(format!("Unknown memory_scope: {}", scope))),
    }
}

fn merge_memory_scope(
    memory: Option<MemoryConfig>,
    memory_scope: Option<String>,
) -> Result<Option<MemoryConfig>, ToolError> {
    let parsed_scope = parse_memory_scope(memory_scope.as_deref())?;
    match (memory, parsed_scope) {
        (Some(mut memory), Some(scope)) => {
            memory.memory_scope = scope;
            Ok(Some(memory))
        }
        (Some(memory), None) => Ok(Some(memory)),
        (None, Some(scope)) => Ok(Some(MemoryConfig {
            memory_scope: scope,
            ..MemoryConfig::default()
        })),
        (None, None) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CliExecutionConfig, ContinuationConfig};

    #[test]
    fn contract_spec_to_core_preserves_background_agent_defaults() {
        let contract: ContractTaskSpec = serde_json::from_value(serde_json::json!({
            "name": "nightly",
            "agent_id": "agent-1",
            "schedule": {
                "type": "interval",
                "interval_ms": 60000,
                "start_at": null
            },
            "execution_mode": {
                "type": "cli",
                "binary": "claude"
            },
            "memory": {},
            "resource_limits": {},
            "continuation": {}
        }))
        .expect("contract background spec");

        let core = contract_spec_to_core(contract).expect("core background spec");

        match core.execution_mode {
            Some(ExecutionMode::Cli(config)) => {
                assert_eq!(
                    config.timeout_secs,
                    CliExecutionConfig::default().timeout_secs
                );
            }
            other => panic!("expected cli execution mode, got {other:?}"),
        }

        assert_eq!(core.memory.expect("memory"), MemoryConfig::default());
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
    fn create_request_to_spec_merges_memory_scope() {
        let spec = create_request_to_spec(BackgroundAgentCreateRequest {
            name: "nightly".to_string(),
            agent_id: "agent-1".to_string(),
            chat_session_id: None,
            schedule: ContractTaskSchedule::Interval {
                interval_ms: 60_000,
                start_at: None,
            },
            input: None,
            input_template: None,
            timeout_secs: None,
            durability_mode: None,
            memory: None,
            memory_scope: Some("per_task".to_string()),
            resource_limits: None,
            preview: false,
            approval_id: None,
        })
        .expect("spec should decode");

        assert_eq!(
            spec.memory.expect("memory").memory_scope,
            MemoryScope::PerTask
        );
    }

    #[test]
    fn create_request_to_spec_accepts_legacy_memory_scope_alias() {
        let spec = create_request_to_spec(BackgroundAgentCreateRequest {
            name: "nightly".to_string(),
            agent_id: "agent-1".to_string(),
            chat_session_id: None,
            schedule: ContractTaskSchedule::Interval {
                interval_ms: 60_000,
                start_at: None,
            },
            input: None,
            input_template: None,
            timeout_secs: None,
            durability_mode: None,
            memory: None,
            memory_scope: Some("per_background_agent".to_string()),
            resource_limits: None,
            preview: false,
            approval_id: None,
        })
        .expect("legacy value should decode through raw ingress compatibility");

        assert_eq!(
            spec.memory.expect("memory").memory_scope,
            MemoryScope::PerTask
        );
    }

    #[test]
    fn resolve_agent_id_alias_accepts_default_alias() {
        let resolved = resolve_agent_id_alias(
            " default ",
            || Ok::<_, &'static str>("agent-default".to_string()),
            |_| Err("should not use explicit resolver"),
        )
        .expect("default alias should resolve");

        assert_eq!(resolved, "agent-default");
    }

    #[test]
    fn resolve_patch_agent_id_resolves_present_alias() {
        let patch = resolve_patch_agent_id(
            TaskPatch {
                agent_id: Some("default".to_string()),
                ..TaskPatch::default()
            },
            |value| Ok::<_, &'static str>(format!("resolved:{value}")),
        )
        .expect("patch should resolve alias");

        assert_eq!(patch.agent_id.as_deref(), Some("resolved:default"));
    }
}
