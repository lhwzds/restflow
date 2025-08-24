use serde::{Deserialize, Serialize};
use super::node::{Node, NodeType};
use super::trigger::TriggerConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_config: Option<TriggerConfig>,
}

impl Workflow {
    /// Extract trigger configuration from workflow nodes
    pub fn extract_trigger_config(&self) -> Option<TriggerConfig> {
        // Look for trigger nodes in the workflow
        for node in &self.nodes {
            match node.node_type {
                NodeType::ManualTrigger => {
                    // Manual is essentially a simplified webhook
                    // Auto-generate path, no auth, POST method, async response
                    return Some(TriggerConfig::Webhook {
                        path: format!("/manual/{}", self.id), // Auto-generate unique path
                        method: "POST".to_string(),
                        auth: None,
                        response_mode: super::trigger::ResponseMode::Async,
                    });
                }
                NodeType::WebhookTrigger => {
                    // Extract webhook config from node config
                    let path = node.config.get("path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("/webhook")
                        .to_string();
                    let method = node.config.get("method")
                        .and_then(|v| v.as_str())
                        .unwrap_or("POST")
                        .to_string();
                    let response_mode = if node.config.get("sync")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false) {
                        super::trigger::ResponseMode::Sync
                    } else {
                        super::trigger::ResponseMode::Async
                    };
                    
                    return Some(TriggerConfig::Webhook {
                        path,
                        method,
                        auth: None, // TODO: extract auth from node config
                        response_mode,
                    });
                }
                NodeType::ScheduleTrigger => {
                    // Extract schedule config from node config
                    let cron = node.config.get("cron")
                        .and_then(|v| v.as_str())
                        .unwrap_or("0 * * * *")
                        .to_string();
                    let timezone = node.config.get("timezone")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let payload = node.config.get("payload").cloned();
                    
                    return Some(TriggerConfig::Schedule {
                        cron,
                        timezone,
                        payload,
                    });
                }
                _ => continue,
            }
        }
        
        // If no trigger node found, return existing trigger_config if any
        self.trigger_config.clone()
    }
    
    /// Check if workflow has a trigger node
    pub fn has_trigger_node(&self) -> bool {
        self.nodes.iter().any(|node| matches!(
            node.node_type,
            NodeType::ManualTrigger | NodeType::WebhookTrigger | NodeType::ScheduleTrigger
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
}