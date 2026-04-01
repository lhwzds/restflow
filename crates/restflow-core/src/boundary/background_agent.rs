use crate::boundary::codec::{from_contract, to_contract};
use crate::models::{
    BackgroundAgentControlAction, BackgroundAgentPatch, BackgroundAgentSchedule,
    BackgroundAgentSpec, DurabilityMode, ExecutionMode, MemoryConfig, MemoryScope,
    NotificationConfig, ResourceLimits,
};
use crate::services::background_agent_conversion::default_conversion_schedule;
use restflow_contracts::request::{
    BackgroundAgentConvertSessionRequest as ContractBackgroundAgentConvertSessionRequest,
    BackgroundAgentPatch as ContractBackgroundAgentPatch,
    BackgroundAgentSpec as ContractBackgroundAgentSpec, DurabilityMode as ContractDurabilityMode,
    ExecutionMode as ContractExecutionMode, MemoryConfig as ContractMemoryConfig,
    NotificationConfig as ContractNotificationConfig, ResourceLimits as ContractResourceLimits,
    TaskSchedule as ContractTaskSchedule,
};
use restflow_tools::ToolError;
use restflow_traits::store::{
    BackgroundAgentConvertSessionRequest as StoreBackgroundAgentConvertSessionRequest,
    BackgroundAgentCreateRequest, BackgroundAgentUpdateRequest,
};
use serde::Serialize;
use serde::de::DeserializeOwned;

pub(crate) struct ConvertSessionRequestOptions {
    pub(crate) name: Option<String>,
    pub(crate) schedule: BackgroundAgentSchedule,
    pub(crate) input: Option<String>,
    pub(crate) timeout_secs: Option<u64>,
    pub(crate) memory: Option<MemoryConfig>,
    pub(crate) durability_mode: Option<DurabilityMode>,
    pub(crate) resource_limits: Option<ResourceLimits>,
    pub(crate) run_now: bool,
}

pub(crate) fn contract_spec_to_core(
    spec: ContractBackgroundAgentSpec,
) -> anyhow::Result<BackgroundAgentSpec> {
    from_contract(spec)
}

pub(crate) fn core_spec_to_contract(
    spec: BackgroundAgentSpec,
) -> anyhow::Result<ContractBackgroundAgentSpec> {
    to_contract(spec)
}

pub(crate) fn contract_patch_to_core(
    patch: ContractBackgroundAgentPatch,
) -> anyhow::Result<BackgroundAgentPatch> {
    from_contract(patch)
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
    mut spec: BackgroundAgentSpec,
    mut resolve_agent_id: ResolveAgentId,
) -> Result<BackgroundAgentSpec, E>
where
    ResolveAgentId: FnMut(&str) -> Result<String, E>,
{
    spec.agent_id = resolve_agent_id(&spec.agent_id)?;
    Ok(spec)
}

#[allow(dead_code)]
pub(crate) fn resolve_patch_agent_id<E, ResolveAgentId>(
    mut patch: BackgroundAgentPatch,
    mut resolve_agent_id: ResolveAgentId,
) -> Result<BackgroundAgentPatch, E>
where
    ResolveAgentId: FnMut(&str) -> Result<String, E>,
{
    if let Some(agent_id) = patch.agent_id.as_deref() {
        patch.agent_id = Some(resolve_agent_id(agent_id)?);
    }
    Ok(patch)
}

pub(crate) fn core_patch_to_contract(
    patch: BackgroundAgentPatch,
) -> anyhow::Result<ContractBackgroundAgentPatch> {
    to_contract(patch)
}

pub(crate) fn core_spec_to_create_request(
    spec: &BackgroundAgentSpec,
) -> anyhow::Result<BackgroundAgentCreateRequest> {
    Ok(BackgroundAgentCreateRequest {
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
        confirmation_token: None,
    })
}

pub(crate) fn core_patch_to_update_request(
    id: String,
    patch: &BackgroundAgentPatch,
) -> anyhow::Result<BackgroundAgentUpdateRequest> {
    Ok(BackgroundAgentUpdateRequest {
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
        confirmation_token: None,
    })
}

pub(crate) fn create_request_to_spec(
    request: BackgroundAgentCreateRequest,
) -> Result<BackgroundAgentSpec, ToolError> {
    let schedule = decode_contract::<ContractTaskSchedule, BackgroundAgentSchedule>(
        "schedule",
        request.schedule,
    )?;

    Ok(BackgroundAgentSpec {
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

pub(crate) fn update_request_to_patch(
    request: BackgroundAgentUpdateRequest,
) -> Result<BackgroundAgentPatch, ToolError> {
    Ok(BackgroundAgentPatch {
        name: request.name,
        description: request.description,
        agent_id: request.agent_id,
        chat_session_id: request.chat_session_id,
        input: request.input,
        input_template: request.input_template,
        schedule: decode_optional_contract::<ContractTaskSchedule, BackgroundAgentSchedule>(
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

pub(crate) fn parse_control_action(
    action: &str,
) -> Result<BackgroundAgentControlAction, ToolError> {
    match action.trim().to_lowercase().as_str() {
        "start" => Ok(BackgroundAgentControlAction::Start),
        "pause" => Ok(BackgroundAgentControlAction::Pause),
        "resume" => Ok(BackgroundAgentControlAction::Resume),
        "stop" => Ok(BackgroundAgentControlAction::Stop),
        "run_now" | "run-now" | "runnow" => Ok(BackgroundAgentControlAction::RunNow),
        value => Err(ToolError::Tool(format!(
            "Unknown control action: {}",
            value
        ))),
    }
}

pub(crate) fn convert_session_request_to_options(
    request: StoreBackgroundAgentConvertSessionRequest,
) -> Result<ConvertSessionRequestOptions, ToolError> {
    Ok(ConvertSessionRequestOptions {
        name: request.name,
        schedule: decode_optional_contract::<ContractTaskSchedule, BackgroundAgentSchedule>(
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

pub(crate) fn contract_convert_request_to_store(
    request: ContractBackgroundAgentConvertSessionRequest,
) -> anyhow::Result<StoreBackgroundAgentConvertSessionRequest> {
    Ok(StoreBackgroundAgentConvertSessionRequest {
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
        confirmation_token: None,
    })
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
        Some(scope) if scope == "per_background_agent" => Ok(Some(MemoryScope::PerBackgroundAgent)),
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
        let contract: ContractBackgroundAgentSpec = serde_json::from_value(serde_json::json!({
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
            memory_scope: Some("per_background_agent".to_string()),
            resource_limits: None,
            preview: false,
            confirmation_token: None,
        })
        .expect("spec should decode");

        assert_eq!(
            spec.memory.expect("memory").memory_scope,
            MemoryScope::PerBackgroundAgent
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
            BackgroundAgentPatch {
                agent_id: Some("default".to_string()),
                ..BackgroundAgentPatch::default()
            },
            |value| Ok::<_, &'static str>(format!("resolved:{value}")),
        )
        .expect("patch should resolve alias");

        assert_eq!(patch.agent_id.as_deref(), Some("resolved:default"));
    }
}
