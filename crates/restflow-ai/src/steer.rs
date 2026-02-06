use serde::{Deserialize, Serialize};

/// A message injected into a running agent's loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteerMessage {
    pub instruction: String,
    pub source: SteerSource,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SteerSource {
    User,
    Telegram,
    Hook,
    Api,
}
