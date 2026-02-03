use crate::models::{AgentNode, Skill, TaskSchedule};
use crate::storage::SystemConfig;
use serde::{Deserialize, Serialize};

/// Message frame: [4 bytes length LE][JSON payload]
pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum IpcRequest {
    Ping,
    GetStatus,
    Shutdown,

    ListAgents,
    GetAgent {
        id: String,
    },
    CreateAgent {
        name: String,
        agent: AgentNode,
    },
    UpdateAgent {
        id: String,
        name: Option<String>,
        agent: Option<AgentNode>,
    },
    DeleteAgent {
        id: String,
    },

    ListSkills,
    GetSkill {
        id: String,
    },
    CreateSkill {
        skill: Skill,
    },
    UpdateSkill {
        id: String,
        skill: Skill,
    },
    DeleteSkill {
        id: String,
    },

    ListTasks,
    GetTask {
        id: String,
    },
    CreateTask {
        name: String,
        agent_id: String,
        schedule: TaskSchedule,
    },
    RunTask {
        id: String,
    },
    StopTask {
        id: String,
    },

    ListSecrets,
    GetSecret {
        key: String,
    },
    SetSecret {
        key: String,
        value: String,
        description: Option<String>,
    },
    DeleteSecret {
        key: String,
    },

    GetConfig,
    SetConfig {
        config: SystemConfig,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum IpcResponse {
    Pong,
    Success(serde_json::Value),
    Error { code: i32, message: String },
}

impl IpcResponse {
    pub fn success<T: Serialize>(data: T) -> Self {
        match serde_json::to_value(data) {
            Ok(value) => Self::Success(value),
            Err(_) => Self::Success(serde_json::Value::Null),
        }
    }

    pub fn error(code: i32, message: impl Into<String>) -> Self {
        Self::Error {
            code,
            message: message.into(),
        }
    }

    pub fn not_found(what: &str) -> Self {
        Self::Error {
            code: 404,
            message: format!("{} not found", what),
        }
    }
}
