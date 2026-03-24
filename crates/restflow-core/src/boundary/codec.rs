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
