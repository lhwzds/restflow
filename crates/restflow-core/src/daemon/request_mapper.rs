pub use crate::boundary::codec::{from_contract, to_contract};
pub(crate) use crate::boundary::error::{invalid_request_response, invalid_validation_response};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::IpcResponse;
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
    fn invalid_validation_response_encodes_structured_details() {
        let response = invalid_validation_response(vec![crate::models::ValidationError::new(
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
