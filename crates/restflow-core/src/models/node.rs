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
                // Extract webhook config from {"type": "WebhookTrigger", "data": {...}}
                // Falls back to root-level config for backward compatibility
                let data = self.config.get("data").unwrap_or(&self.config);

                let path = data
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&format!("/webhook/{}", self.id))
                    .to_string();

                let method = data
                    .get("method")
                    .and_then(|v| v.as_str())
                    .unwrap_or("POST")
                    .to_string();

                // Extract auth config if present
                let auth = data.get("auth").and_then(|auth| {
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
                // Extract schedule config from {"type": "ScheduleTrigger", "data": {...}}
                // Falls back to root-level config for backward compatibility
                let data = self.config.get("data").unwrap_or(&self.config);

                let cron = data
                    .get("cron")
                    .and_then(|v| v.as_str())
                    .unwrap_or("0 * * * *")
                    .to_string();

                let timezone = data
                    .get("timezone")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let payload = data.get("payload").cloned();

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::trigger::{AuthConfig, TriggerConfig};
    use serde_json::json;

    #[test]
    fn test_extract_webhook_trigger_config() {
        // Test with new format (config wrapped in {"type": "...", "data": {...}})
        let node = Node {
            id: "webhook-1".to_string(),
            node_type: NodeType::WebhookTrigger,
            config: json!({
                "type": "WebhookTrigger",
                "data": {
                    "path": "/webhook/test",
                    "method": "POST"
                }
            }),
            position: None,
        };

        let config = node.extract_trigger_config();
        assert!(config.is_some());

        if let Some(TriggerConfig::Webhook { path, method, auth }) = config {
            assert_eq!(path, "/webhook/test");
            assert_eq!(method, "POST");
            assert!(auth.is_none());
        } else {
            panic!("Expected Webhook trigger config");
        }
    }

    #[test]
    fn test_extract_webhook_trigger_config_with_auth() {
        let node = Node {
            id: "webhook-2".to_string(),
            node_type: NodeType::WebhookTrigger,
            config: json!({
                "type": "WebhookTrigger",
                "data": {
                    "path": "/api/webhook",
                    "method": "PUT",
                    "auth": {
                        "type": "api_key",
                        "key": "secret123",
                        "header_name": "X-API-Key"
                    }
                }
            }),
            position: None,
        };

        let config = node.extract_trigger_config();
        assert!(config.is_some());

        if let Some(TriggerConfig::Webhook { path, method, auth }) = config {
            assert_eq!(path, "/api/webhook");
            assert_eq!(method, "PUT");
            assert!(auth.is_some());

            if let Some(AuthConfig::ApiKey { key, header_name }) = auth {
                assert_eq!(key, "secret123");
                assert_eq!(header_name, Some("X-API-Key".to_string()));
            } else {
                panic!("Expected ApiKey auth config");
            }
        } else {
            panic!("Expected Webhook trigger config");
        }
    }

    #[test]
    fn test_extract_schedule_trigger_config() {
        let node = Node {
            id: "schedule-1".to_string(),
            node_type: NodeType::ScheduleTrigger,
            config: json!({
                "type": "ScheduleTrigger",
                "data": {
                    "cron": "0 10 * * *",
                    "timezone": "America/New_York",
                    "payload": {"key": "value"}
                }
            }),
            position: None,
        };

        let config = node.extract_trigger_config();
        assert!(config.is_some());

        if let Some(TriggerConfig::Schedule {
            cron,
            timezone,
            payload,
        }) = config
        {
            assert_eq!(cron, "0 10 * * *");
            assert_eq!(timezone, Some("America/New_York".to_string()));
            assert_eq!(payload, Some(json!({"key": "value"})));
        } else {
            panic!("Expected Schedule trigger config");
        }
    }

    #[test]
    fn test_extract_manual_trigger_config() {
        let node = Node {
            id: "manual-1".to_string(),
            node_type: NodeType::ManualTrigger,
            config: json!({
                "type": "ManualTrigger",
                "data": {
                    "payload": null
                }
            }),
            position: None,
        };

        let config = node.extract_trigger_config();
        assert!(config.is_some());

        if let Some(TriggerConfig::Webhook { path, method, auth }) = config {
            assert_eq!(path, "/manual/manual-1");
            assert_eq!(method, "POST");
            assert!(auth.is_none());
        } else {
            panic!("Expected Webhook trigger config for ManualTrigger");
        }
    }

    #[test]
    fn test_backward_compatibility_webhook() {
        // Test old format (without "type" and "data" wrapper)
        let node = Node {
            id: "webhook-old".to_string(),
            node_type: NodeType::WebhookTrigger,
            config: json!({
                "path": "/old/webhook",
                "method": "GET"
            }),
            position: None,
        };

        let config = node.extract_trigger_config();
        assert!(config.is_some());

        if let Some(TriggerConfig::Webhook { path, method, .. }) = config {
            assert_eq!(path, "/old/webhook");
            assert_eq!(method, "GET");
        } else {
            panic!("Expected Webhook trigger config");
        }
    }

    #[test]
    fn test_backward_compatibility_schedule() {
        // Test old format (without "type" and "data" wrapper)
        let node = Node {
            id: "schedule-old".to_string(),
            node_type: NodeType::ScheduleTrigger,
            config: json!({
                "cron": "0 0 * * *",
                "timezone": "UTC"
            }),
            position: None,
        };

        let config = node.extract_trigger_config();
        assert!(config.is_some());

        if let Some(TriggerConfig::Schedule { cron, timezone, .. }) = config {
            assert_eq!(cron, "0 0 * * *");
            assert_eq!(timezone, Some("UTC".to_string()));
        } else {
            panic!("Expected Schedule trigger config");
        }
    }
}
