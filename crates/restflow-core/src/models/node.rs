use super::trigger::{AuthConfig, TriggerConfig};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Node {
    pub id: String,
    pub node_type: NodeType,
    #[ts(type = "any")]
    pub config: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
}

impl Node {
    /// Check if this node is a trigger node
    pub fn is_trigger(&self) -> bool {
        matches!(
            self.node_type,
            NodeType::ManualTrigger | NodeType::WebhookTrigger | NodeType::ScheduleTrigger
        )
    }

    /// Extract trigger configuration from node
    pub fn extract_trigger_config(&self) -> Option<TriggerConfig> {
        match self.node_type {
            NodeType::ManualTrigger => {
                // Manual trigger is a simple webhook with auto-generated path
                Some(TriggerConfig::Webhook {
                    path: format!("/manual/{}", self.id),
                    method: "POST".to_string(),
                    auth: None,
                })
            }
            NodeType::WebhookTrigger => {
                // Extract webhook configuration from node config
                let path = self
                    .config
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&format!("/webhook/{}", self.id))
                    .to_string();

                let method = self
                    .config
                    .get("method")
                    .and_then(|v| v.as_str())
                    .unwrap_or("POST")
                    .to_string();

                // Extract auth config if present
                let auth = self.config.get("auth").and_then(|auth| {
                    let auth_type = auth.get("type")?.as_str()?;
                    match auth_type {
                        "api_key" => {
                            let key = auth.get("key")?.as_str()?.to_string();
                            let header_name = auth
                                .get("header_name")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());
                            Some(AuthConfig::ApiKey { key, header_name })
                        }
                        "basic" => {
                            let username = auth.get("username")?.as_str()?.to_string();
                            let password = auth.get("password")?.as_str()?.to_string();
                            Some(AuthConfig::Basic { username, password })
                        }
                        _ => None,
                    }
                });

                Some(TriggerConfig::Webhook { path, method, auth })
            }
            NodeType::ScheduleTrigger => {
                // Extract schedule configuration from node config
                let cron = self
                    .config
                    .get("cron")
                    .and_then(|v| v.as_str())
                    .unwrap_or("0 * * * *")
                    .to_string();

                let timezone = self
                    .config
                    .get("timezone")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let payload = self.config.get("payload").cloned();

                Some(TriggerConfig::Schedule {
                    cron,
                    timezone,
                    payload,
                })
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[ts(export)]
pub enum NodeType {
    ManualTrigger,
    WebhookTrigger,
    ScheduleTrigger,
    Agent,
    HttpRequest,
    Print,
    DataTransform,
    Python,
}
