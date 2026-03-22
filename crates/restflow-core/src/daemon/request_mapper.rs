use crate::daemon::IpcResponse;
use crate::models::{ValidationError, ValidationErrorResponse};
use anyhow::Context;
use serde::Serialize;
use serde::de::DeserializeOwned;

pub fn to_contract<T, U>(value: T) -> anyhow::Result<U>
where
    T: Serialize,
    U: DeserializeOwned,
{
    let encoded =
        serde_json::to_value(value).context("failed to serialize core request payload")?;
    serde_json::from_value(encoded).context("failed to decode contract request payload")
}

pub fn from_contract<T, U>(value: T) -> anyhow::Result<U>
where
    T: Serialize,
    U: DeserializeOwned,
{
    let encoded =
        serde_json::to_value(value).context("failed to serialize contract request payload")?;
    serde_json::from_value(encoded).context("failed to decode core request payload")
}

pub(crate) fn invalid_request_response(error: anyhow::Error) -> IpcResponse {
    IpcResponse::error(400, format!("Invalid request payload: {error:#}"))
}

pub(crate) fn invalid_validation_response(errors: Vec<ValidationError>) -> IpcResponse {
    let details = serde_json::to_value(ValidationErrorResponse::new(errors))
        .expect("validation error response should serialize");
    IpcResponse::error_with_details(400, "Validation failed", Some(details))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        BackgroundAgentSpec as CoreBackgroundAgentSpec, ExecutionMode as CoreExecutionMode,
        ValidationError,
    };
    use restflow_contracts::request::{
        BackgroundAgentSpec as ContractBackgroundAgentSpec, ExecutionMode as ContractExecutionMode,
        TaskSchedule,
    };
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct CorePayload {
        id: String,
        enabled: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct ContractPayload {
        id: String,
        enabled: bool,
    }

    #[test]
    fn to_contract_round_trips_same_shape() {
        let core = CorePayload {
            id: "a".to_string(),
            enabled: true,
        };

        let contract: ContractPayload = to_contract(core).unwrap();
        assert_eq!(
            contract,
            ContractPayload {
                id: "a".to_string(),
                enabled: true,
            }
        );
    }

    #[test]
    fn from_contract_round_trips_same_shape() {
        let contract = ContractPayload {
            id: "a".to_string(),
            enabled: false,
        };

        let core: CorePayload = from_contract(contract).unwrap();
        assert_eq!(
            core,
            CorePayload {
                id: "a".to_string(),
                enabled: false,
            }
        );
    }

    #[test]
    fn from_contract_preserves_background_agent_defaults() {
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
            "resource_limits": {}
        }))
        .expect("contract background spec");

        let core: CoreBackgroundAgentSpec = from_contract(contract).expect("core background spec");

        match core.execution_mode {
            Some(CoreExecutionMode::Cli(config)) => {
                assert_eq!(
                    config.timeout_secs,
                    crate::models::CliExecutionConfig::default().timeout_secs
                );
            }
            other => panic!("expected cli execution mode, got {other:?}"),
        }

        let memory = core.memory.expect("memory");
        assert_eq!(memory, crate::models::MemoryConfig::default());

        let limits = core.resource_limits.expect("resource limits");
        assert_eq!(limits, crate::models::ResourceLimits::default());
    }

    #[test]
    fn to_contract_preserves_background_agent_shape() {
        let core = CoreBackgroundAgentSpec {
            name: "nightly".to_string(),
            agent_id: "agent-1".to_string(),
            chat_session_id: None,
            description: None,
            input: Some("run".to_string()),
            input_template: None,
            schedule: crate::models::BackgroundAgentSchedule::default(),
            notification: None,
            execution_mode: Some(CoreExecutionMode::Cli(
                crate::models::CliExecutionConfig::default(),
            )),
            timeout_secs: None,
            memory: Some(crate::models::MemoryConfig::default()),
            durability_mode: None,
            resource_limits: Some(crate::models::ResourceLimits::default()),
            prerequisites: Vec::new(),
            continuation: None,
        };

        let contract: ContractBackgroundAgentSpec =
            to_contract(core).expect("contract background spec");
        assert_eq!(contract.schedule, TaskSchedule::default());
        match contract.execution_mode {
            Some(ContractExecutionMode::Cli(config)) => {
                assert_eq!(
                    config.timeout_secs,
                    crate::models::CliExecutionConfig::default().timeout_secs
                );
            }
            other => panic!("expected cli execution mode, got {other:?}"),
        }
    }

    #[test]
    fn invalid_validation_response_encodes_structured_details() {
        let response = invalid_validation_response(vec![ValidationError::new(
            "model_ref.provider",
            "unknown provider 'bad'",
        )]);

        match response {
            IpcResponse::Error(error) => {
                assert_eq!(error.code, 400);
                assert_eq!(error.kind, restflow_contracts::ErrorKind::Validation);
                assert_eq!(error.message, "Validation failed");
                let details = error.details.expect("validation details");
                assert_eq!(details["type"], "validation_error");
                assert_eq!(details["errors"][0]["field"], "model_ref.provider");
            }
            other => panic!("expected error response, got {other:?}"),
        }
    }
}
