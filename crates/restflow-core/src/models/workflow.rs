use super::node::Node;
use super::trigger::TriggerConfig;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    // trigger_config removed - now stored in trigger nodes
}

impl Workflow {
    /// Extract trigger configurations from workflow nodes
    /// Returns all trigger configurations found in the workflow
    pub fn extract_trigger_configs(&self) -> Vec<(String, TriggerConfig)> {
        let mut configs = Vec::new();

        for node in &self.nodes {
            if let Some(config) = node.extract_trigger_config() {
                configs.push((node.id.clone(), config));
            }
        }

        configs
    }

    /// Get all trigger nodes
    pub fn get_trigger_nodes(&self) -> Vec<&Node> {
        self.nodes.iter().filter(|n| n.is_trigger()).collect()
    }

    /// Check if workflow has a trigger node
    pub fn has_trigger_node(&self) -> bool {
        self.nodes.iter().any(|node| node.is_trigger())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Edge {
    pub from: String,
    pub to: String,
}
