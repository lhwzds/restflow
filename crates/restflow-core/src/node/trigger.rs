use crate::engine::context::{ExecutionContext, namespace};
use crate::node::registry::NodeExecutor;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

/// Unified Trigger node executor
///
/// All Trigger types (Webhook, Manual, Schedule) use the same logic:
/// Read trigger.payload from context and output it for downstream nodes
pub struct TriggerExecutor;

#[async_trait]
impl NodeExecutor for TriggerExecutor {
    async fn execute(&self, _config: &Value, context: &mut ExecutionContext) -> Result<Value> {
        context
            .get(namespace::trigger::PAYLOAD)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Trigger payload not found in context"))
    }
}
