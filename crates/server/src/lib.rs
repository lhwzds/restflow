//! # codocia
//!
//! Server owns the product ingress boundary.
//!
//! ## Owns
//! - CoreCommand transport contract
//! - JSON command decoding and response encoding
//! - local in-process core endpoint
//! - shell/API/MCP-facing dispatch shape
//!
//! ## Must Not
//! - render UI
//! - own durable storage tables
//! - implement provider/model logic
//! - bypass CoreCommand/CoreResponse
//!
//! ## Inputs
//! - CoreCommand values
//! - CoreCommand JSON
//!
//! ## Outputs
//! - CoreResponse values
//! - CoreResponse JSON
//!
//! ## Depends On
//! - engine
//! - proto
//!
//! ## Used By
//! - CLI/TUI/Web/MCP adapters
//! - Python native bridge
//!
//! ## Verify
//! - cargo check -p server

use anyhow::{Context, Result};
use async_trait::async_trait;
use engine::Core;
use proto::{CoreCommand, CoreResponse};

#[async_trait]
pub trait CommandTransport {
    async fn send(&mut self, command: CoreCommand) -> Result<CoreResponse>;
}

#[async_trait]
pub trait JsonTransport {
    async fn send_json(&mut self, command_json: &str) -> Result<String>;
}

pub struct CoreEndpoint {
    core: Core,
}

impl CoreEndpoint {
    pub fn new(core: Core) -> Self {
        Self { core }
    }

    pub fn core(&self) -> &Core {
        &self.core
    }

    pub fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    pub fn into_core(self) -> Core {
        self.core
    }
}

#[async_trait]
impl CommandTransport for CoreEndpoint {
    async fn send(&mut self, command: CoreCommand) -> Result<CoreResponse> {
        dispatch(&mut self.core, command).await
    }
}

#[async_trait]
impl<T> JsonTransport for T
where
    T: CommandTransport + Send,
{
    async fn send_json(&mut self, command_json: &str) -> Result<String> {
        let command = decode_command(command_json)?;
        let response = self.send(command).await?;
        encode_response(&response)
    }
}

pub async fn dispatch(core: &mut Core, command: CoreCommand) -> Result<CoreResponse> {
    core.handle(command).await
}

pub fn decode_command(command_json: &str) -> Result<CoreCommand> {
    serde_json::from_str(command_json).context("decode CoreCommand JSON")
}

pub fn encode_response(response: &CoreResponse) -> Result<String> {
    serde_json::to_string(response).context("encode CoreResponse JSON")
}

pub async fn dispatch_json(core: &mut Core, command_json: &str) -> Result<String> {
    let command = decode_command(command_json)?;
    let response = dispatch(core, command).await?;
    encode_response(&response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    #[test]
    fn endpoint_routes_command_to_core() {
        block_on_once(async {
            let core = Core::new(model::Model::new("openai", "gpt-5.4"));
            let mut endpoint = CoreEndpoint::new(core);

            let response = endpoint
                .send(CoreCommand::SwitchModel {
                    model: model::Model::new("openai", "gpt-5.5"),
                })
                .await
                .unwrap();

            assert_eq!(
                response,
                CoreResponse::ModelSwitched {
                    model: model::Model::new("openai", "gpt-5.5")
                }
            );
            assert_eq!(endpoint.core().current_model().id, "gpt-5.5");
        });
    }

    #[test]
    fn endpoint_round_trips_json_command() {
        block_on_once(async {
            let core = Core::new(model::Model::new("openai", "gpt-5.4"));
            let mut endpoint = CoreEndpoint::new(core);

            let response = endpoint
                .send_json(
                    r#"{"type":"switch_model","model":{"provider":{"id":"openai"},"id":"gpt-5.5"}}"#,
                )
                .await
                .unwrap();

            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&response).unwrap(),
                serde_json::json!({
                    "type": "model_switched",
                    "model": {
                        "provider": { "id": "openai" },
                        "id": "gpt-5.5"
                    }
                })
            );
        });
    }

    #[test]
    fn dispatch_json_reports_decode_errors() {
        block_on_once(async {
            let mut core = Core::new(model::Model::new("openai", "gpt-5.4"));
            let error = dispatch_json(&mut core, r#"{"model":{}}"#)
                .await
                .unwrap_err();

            assert!(error.to_string().contains("decode CoreCommand JSON"));
        });
    }

    fn block_on_once<T>(future: impl Future<Output = T>) -> T {
        let waker = Waker::from(Arc::new(NoopWake));
        let mut context = Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);

        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("server future unexpectedly yielded"),
        }
    }

    struct NoopWake;

    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }
}
