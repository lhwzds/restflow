//! Language Server Protocol (LSP) integration.

mod client;
mod manager;
mod types;

pub use client::LspClient;
pub use manager::{LspManager, LspServerConfig};
pub use types::{JsonRpcError, JsonRpcMessage, JsonRpcResponse};
