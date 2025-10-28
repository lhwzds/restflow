use crate::engine::context::{ExecutionContext, namespace};
use crate::models::{NodeOutput, NodeType, ScheduleOutput, ManualTriggerOutput, WebhookTriggerOutput};
use crate::node::registry::NodeExecutor;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

/// Unified Trigger node executor
///
/// All Trigger types (Webhook, Manual, Schedule) use the same logic:
/// Read trigger.payload from context and output it for downstream nodes
pub struct TriggerExecutor;

#[async_trait]
impl NodeExecutor for TriggerExecutor {
    async fn execute(
        &self,
        node_type: &NodeType,
        _config: &Value,
        context: &mut ExecutionContext,
    ) -> Result<NodeOutput> {
        let payload = context
            .get(namespace::trigger::PAYLOAD)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Trigger payload not found in context"))?;

        match node_type {
            NodeType::ScheduleTrigger => {
                // Schedule trigger: extract triggered_at and payload
                let triggered_at = chrono::Utc::now().timestamp_millis();
                Ok(NodeOutput::ScheduleTrigger(ScheduleOutput {
                    triggered_at,
                    payload,
                }))
            }
            NodeType::ManualTrigger => {
                // Manual trigger: simple triggered_at + payload
                let triggered_at = chrono::Utc::now().timestamp_millis();
                Ok(NodeOutput::ManualTrigger(ManualTriggerOutput {
                    triggered_at,
                    payload,
                }))
            }
            NodeType::WebhookTrigger => {
                // Webhook trigger: extract HTTP request information from context
                let triggered_at = chrono::Utc::now().timestamp_millis();
                let method = context
                    .get("trigger.method")
                    .and_then(|v| v.as_str())
                    .unwrap_or("POST")
                    .to_string();

                let headers = context
                    .get("trigger.headers")
                    .and_then(|v| serde_json::from_value::<HashMap<String, String>>(v.clone()).ok())
                    .unwrap_or_default();

                let query = context
                    .get("trigger.query")
                    .and_then(|v| serde_json::from_value::<HashMap<String, String>>(v.clone()).ok())
                    .unwrap_or_default();

                Ok(NodeOutput::WebhookTrigger(WebhookTriggerOutput {
                    triggered_at,
                    method,
                    headers,
                    body: payload,
                    query,
                }))
            }
            _ => Err(anyhow::anyhow!(
                "TriggerExecutor called with non-trigger node type: {:?}",
                node_type
            )),
        }
    }
}
