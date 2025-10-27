use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use ts_rs::TS;

/// Unified node output enum
/// Each variant corresponds to a node type's output structure
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", content = "data")]
pub enum NodeOutput {
    Http(HttpOutput),
    Agent(AgentOutput),
    Python(PythonOutput),
    Print(PrintOutput),
    ManualTrigger(TriggerOutput),
    WebhookTrigger(TriggerOutput),
    ScheduleTrigger(ScheduleOutput),
}

/// HTTP Request output
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HttpOutput {
    pub status: u16,
    #[ts(type = "Record<string, string>")]
    pub headers: HashMap<String, String>,
    #[ts(type = "any")]
    pub body: Value,
}

/// AI Agent output
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentOutput {
    pub response: String,
}

/// Python script output
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PythonOutput {
    #[ts(type = "any")]
    pub result: Value,
}

/// Print node output
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PrintOutput {
    pub printed: String,
}

/// Trigger output (webhook/manual)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TriggerOutput {
    pub method: String,
    #[ts(type = "Record<string, string>")]
    pub headers: HashMap<String, String>,
    #[ts(type = "any")]
    pub body: Value,
    #[ts(type = "Record<string, string>")]
    pub query: HashMap<String, String>,
}

/// Schedule trigger output
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ScheduleOutput {
    pub triggered_at: i64,
    #[ts(type = "any")]
    pub payload: Value,
}

impl NodeOutput {
    /// Get type-safe access to HTTP output
    pub fn as_http(&self) -> Option<&HttpOutput> {
        match self {
            Self::Http(output) => Some(output),
            _ => None,
        }
    }

    /// Get type-safe access to Agent output
    pub fn as_agent(&self) -> Option<&AgentOutput> {
        match self {
            Self::Agent(output) => Some(output),
            _ => None,
        }
    }

    /// Get type-safe access to Python output
    pub fn as_python(&self) -> Option<&PythonOutput> {
        match self {
            Self::Python(output) => Some(output),
            _ => None,
        }
    }

    /// Get type-safe access to Print output
    pub fn as_print(&self) -> Option<&PrintOutput> {
        match self {
            Self::Print(output) => Some(output),
            _ => None,
        }
    }

    /// Get type-safe access to Trigger output
    pub fn as_trigger(&self) -> Option<&TriggerOutput> {
        match self {
            Self::ManualTrigger(output) | Self::WebhookTrigger(output) => Some(output),
            _ => None,
        }
    }

    /// Get type-safe access to Schedule output
    pub fn as_schedule(&self) -> Option<&ScheduleOutput> {
        match self {
            Self::ScheduleTrigger(output) => Some(output),
            _ => None,
        }
    }
}
