//! Shared boundary contracts used across transport and app layers.

mod error;
mod operation;
pub mod request;
mod response;
mod stream;
mod tool;

pub use error::{ErrorKind, ErrorPayload};
pub use operation::{
    ApiKeyResponse, ApprovalHandledResponse, ArchiveResponse, CancelResponse, ClearResponse,
    DeleteResponse, DeleteWithIdResponse, IdResponse, IpcDaemonStatus, OkResponse, PromptResponse,
    SecretResponse, SteerResponse,
};
pub use request::IpcRequest;
pub use response::ResponseEnvelope;
pub use stream::StreamEnvelope;
pub use tool::{ToolDefinition, ToolErrorCategory, ToolExecutionResult};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn public_contract_exports_are_accessible() {
        let _ = ErrorPayload::new(500, "boom", None);
        let _ = ResponseEnvelope::<Value>::Pong;
        let _ = StreamEnvelope::<()>::Done { total_tokens: None };
        let _ = OkResponse { ok: true };
        let _ = IpcRequest::Ping;
        let _ = IpcDaemonStatus {
            status: "running".to_string(),
            protocol_version: "2".to_string(),
            daemon_version: "0.4.0".to_string(),
            pid: 1,
            started_at_ms: 0,
            uptime_secs: 0,
        };
    }
}
