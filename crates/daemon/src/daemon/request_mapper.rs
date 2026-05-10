use anyhow::Context;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::daemon::IpcResponse;
use types::{ValidationError, ValidationErrorResponse};

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
    use crate::daemon::IpcResponse;
    use serde::{Deserialize, Serialize};
    use types::{Skill, SkillSource};

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
    fn skill_contract_preserves_source_metadata() {
        let mut core_skill = Skill::new(
            "skill-1".to_string(),
            "Skill 1".to_string(),
            Some("External skill".to_string()),
            Some(vec!["external".to_string()]),
            "# Skill".to_string(),
        );
        core_skill.source = SkillSource::External;
        core_skill.read_only = false;
        core_skill.source_ref = Some("marketplace:skill-1@1.0.0".to_string());

        let contract: types::request::Skill = to_contract(core_skill.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(contract.source).unwrap(),
            serde_json::json!("external")
        );
        assert_eq!(
            contract.source_ref.as_deref(),
            Some("marketplace:skill-1@1.0.0")
        );

        let round_trip: Skill = from_contract(contract).unwrap();
        assert_eq!(round_trip.source, SkillSource::External);
        assert_eq!(
            round_trip.source_ref.as_deref(),
            Some("marketplace:skill-1@1.0.0")
        );
    }

    #[test]
    fn invalid_validation_response_encodes_structured_details() {
        let response = invalid_validation_response(vec![types::ValidationError::new(
            "model_ref.provider",
            "unknown provider 'bad'",
        )]);

        match response {
            IpcResponse::Error(error) => {
                assert_eq!(error.code, 400);
                assert_eq!(error.kind, types::ErrorKind::Validation);
                assert_eq!(error.message, "Validation failed");
                let details = error.details.expect("validation details");
                assert_eq!(details["type"], "validation_error");
                assert_eq!(details["errors"][0]["field"], "model_ref.provider");
            }
            other => panic!("expected error response, got {other:?}"),
        }
    }
}
