use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(tag = "type", rename_all = "lowercase")]
#[ts(export)]
pub enum TriggerConfig {
    Manual,
    Webhook {
        path: String,
        method: String, // HTTP method as string (GET, POST, etc.)
        auth: Option<AuthConfig>,
        // Webhooks use async mode only, returning execution_id
    },
    Schedule {
        cron: String,
        timezone: Option<String>,
        #[ts(type = "any")]
        payload: Option<Value>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(tag = "type")]
#[ts(export)]
pub enum AuthConfig {
    None,
    ApiKey {
        key: String,
        header_name: Option<String>, // Default X-API-Key
    },
    Basic {
        username: String,
        password: String,
    },
}

// ResponseMode removed - Webhooks use async mode only

// Store active trigger information
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ActiveTrigger {
    pub id: String,
    pub workflow_id: String,
    pub trigger_config: TriggerConfig,
    /// Unix timestamp (seconds) when the trigger was activated
    #[ts(type = "number")]
    pub activated_at: i64,
    /// Unix timestamp (seconds) of the last trigger fire
    #[ts(type = "number | null")]
    pub last_triggered_at: Option<i64>,
    #[ts(type = "number")]
    pub trigger_count: u64,
}

impl ActiveTrigger {
    pub fn new(workflow_id: String, trigger_config: TriggerConfig) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            workflow_id,
            trigger_config,
            activated_at: chrono::Utc::now().timestamp(),
            last_triggered_at: None,
            trigger_count: 0,
        }
    }

    pub fn record_trigger(&mut self) {
        self.last_triggered_at = Some(chrono::Utc::now().timestamp());
        self.trigger_count += 1;
    }
}
