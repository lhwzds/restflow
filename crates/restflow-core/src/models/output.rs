use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use ts_rs::TS;

/// Unified node output enum. Each variant corresponds to a node type's output structure.
///
/// Note: Uses `#[serde(tag = "type", content = "data")]` for O(1) deserialization (15-20% faster) at ~20 bytes overhead per output.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", content = "data")]
pub enum NodeOutput {
    Http(HttpOutput),
    Agent(AgentOutput),
    Python(PythonOutput),
    Print(PrintOutput),
    Email(EmailOutput),
    ManualTrigger(ManualTriggerOutput),
    WebhookTrigger(WebhookTriggerOutput),
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

/// Manual trigger output
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ManualTriggerOutput {
    #[ts(type = "number")]
    pub triggered_at: i64,
    #[ts(type = "any")]
    pub payload: Value,
}

/// Webhook trigger output - received HTTP request data
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WebhookTriggerOutput {
    #[ts(type = "number")]
    pub triggered_at: i64,
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
    #[ts(type = "number")]
    pub triggered_at: i64,
    #[ts(type = "any")]
    pub payload: Value,
}

/// Email send output
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EmailOutput {
    /// Unix timestamp (milliseconds) when email was sent
    #[ts(type = "number")]
    pub sent_at: i64,
    /// Message ID from email server (if available)
    pub message_id: Option<String>,
    /// List of recipients
    pub recipients: Vec<String>,
    /// Email subject
    pub subject: String,
    /// Whether email was sent as HTML
    pub is_html: bool,
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

    /// Get type-safe access to Email output
    pub fn as_email(&self) -> Option<&EmailOutput> {
        match self {
            Self::Email(output) => Some(output),
            _ => None,
        }
    }

    /// Get type-safe access to ManualTrigger output
    pub fn as_manual_trigger(&self) -> Option<&ManualTriggerOutput> {
        match self {
            Self::ManualTrigger(output) => Some(output),
            _ => None,
        }
    }

    /// Get type-safe access to WebhookTrigger output
    pub fn as_webhook_trigger(&self) -> Option<&WebhookTriggerOutput> {
        match self {
            Self::WebhookTrigger(output) => Some(output),
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
