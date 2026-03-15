use crate::daemon::IpcResponse;
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

#[cfg(test)]
mod tests {
    use super::*;
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
}
