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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_context_with_payload(payload: Value) -> ExecutionContext {
        let mut context = ExecutionContext::new("test-exec".to_string());
        context.set(namespace::trigger::PAYLOAD, payload);
        context
    }

    #[tokio::test]
    async fn test_manual_trigger_execution() {
        let executor = TriggerExecutor;
        let node_type = NodeType::ManualTrigger;
        let config = json!({});
        let payload = json!({"message": "User triggered workflow"});
        let mut context = create_context_with_payload(payload.clone());

        let result = executor.execute(&node_type, &config, &mut context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        match output {
            NodeOutput::ManualTrigger(manual_output) => {
                assert!(manual_output.triggered_at > 0);
                assert_eq!(manual_output.payload, payload);
            }
            _ => panic!("Expected ManualTriggerOutput"),
        }
    }

    #[tokio::test]
    async fn test_webhook_trigger_execution() {
        let executor = TriggerExecutor;
        let node_type = NodeType::WebhookTrigger;
        let config = json!({});
        let payload = json!({"data": "Webhook request body"});

        let mut context = create_context_with_payload(payload.clone());
        context.set("trigger.method", json!("POST"));
        context.set("trigger.headers", json!({"content-type": "application/json"}));
        context.set("trigger.query", json!({"key": "value"}));

        let result = executor.execute(&node_type, &config, &mut context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        match output {
            NodeOutput::WebhookTrigger(webhook_output) => {
                assert!(webhook_output.triggered_at > 0);
                assert_eq!(webhook_output.method, "POST");
                assert_eq!(webhook_output.body, payload);
                assert_eq!(webhook_output.headers.get("content-type"), Some(&"application/json".to_string()));
                assert_eq!(webhook_output.query.get("key"), Some(&"value".to_string()));
            }
            _ => panic!("Expected WebhookTriggerOutput"),
        }
    }

    #[tokio::test]
    async fn test_webhook_trigger_defaults() {
        let executor = TriggerExecutor;
        let node_type = NodeType::WebhookTrigger;
        let config = json!({});
        let payload = json!({"data": "test"});
        let mut context = create_context_with_payload(payload.clone());
        // Don't set method, headers, query - test defaults

        let result = executor.execute(&node_type, &config, &mut context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        match output {
            NodeOutput::WebhookTrigger(webhook_output) => {
                assert_eq!(webhook_output.method, "POST"); // Default
                assert!(webhook_output.headers.is_empty());
                assert!(webhook_output.query.is_empty());
            }
            _ => panic!("Expected WebhookTriggerOutput"),
        }
    }

    #[tokio::test]
    async fn test_schedule_trigger_execution() {
        let executor = TriggerExecutor;
        let node_type = NodeType::ScheduleTrigger;
        let config = json!({});
        let payload = json!({"scheduled": true});
        let mut context = create_context_with_payload(payload.clone());

        let result = executor.execute(&node_type, &config, &mut context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        match output {
            NodeOutput::ScheduleTrigger(schedule_output) => {
                assert!(schedule_output.triggered_at > 0);
                assert_eq!(schedule_output.payload, payload);
            }
            _ => panic!("Expected ScheduleOutput"),
        }
    }

    #[tokio::test]
    async fn test_trigger_missing_payload() {
        let executor = TriggerExecutor;
        let node_type = NodeType::ManualTrigger;
        let config = json!({});
        let mut context = ExecutionContext::new("test-exec".to_string());
        // Don't set payload

        let result = executor.execute(&node_type, &config, &mut context).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Trigger payload not found"));
    }

    #[tokio::test]
    async fn test_trigger_with_non_trigger_node_type() {
        let executor = TriggerExecutor;
        let node_type = NodeType::Agent;
        let config = json!({});
        let mut context = create_context_with_payload(json!({}));

        let result = executor.execute(&node_type, &config, &mut context).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("non-trigger node type"));
    }
}
