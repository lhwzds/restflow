pub use crate::boundary::codec::{from_contract, to_contract};
pub(crate) use crate::boundary::error::{invalid_request_response, invalid_validation_response};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::IpcResponse;
    use crate::models::{Skill, SkillSource};
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
        let response = invalid_validation_response(vec![crate::models::ValidationError::new(
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
