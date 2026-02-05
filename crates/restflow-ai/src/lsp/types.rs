//! JSON-RPC types for LSP communication.

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcMessage {
    Request(JsonRpcRequest),
    Response(JsonRpcResponse),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    jsonrpc: String,
    pub id: Option<u64>,
    pub method: String,
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    pub params: serde_json::Value,
}

impl JsonRpcRequest {
    pub fn new<P: Serialize>(id: u64, method: &str, params: P) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: Some(id),
            method: method.to_string(),
            params: serde_json::to_value(params).unwrap_or(serde_json::Value::Null),
        }
    }

    pub fn notification<P: Serialize>(method: &str, params: P) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: method.to_string(),
            params: serde_json::to_value(params).unwrap_or(serde_json::Value::Null),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: u64,
    pub result: Option<serde_json::Value>,
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    pub fn into_result<T: serde::de::DeserializeOwned>(self) -> Result<T, JsonRpcError> {
        if let Some(error) = self.error {
            return Err(error);
        }

        let result = self
            .result
            .ok_or_else(|| JsonRpcError::new(-1, "Missing response result"))?;

        serde_json::from_value(result)
            .map_err(|err| JsonRpcError::new(-2, err.to_string()))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }
}
