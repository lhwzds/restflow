use serde::{Deserialize, Serialize};

/// A message injected into a running agent's ReAct loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteerMessage {
    pub instruction: String,
    pub source: SteerSource,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SteerSource {
    /// Direct from UI or CLI.
    User,
    /// From Telegram channel.
    Telegram,
    /// From a hook or automation.
    Hook,
    /// From REST/WebSocket API.
    Api,
}
