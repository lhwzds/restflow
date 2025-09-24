use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use ts_rs::TS;
use crate::storage::SecretStorage;

// Compile regex once at first use, then reuse for performance
// Pattern: {{variable_name}} or {{node_id.field.subfield}}
// \{\{  - Match literal {{
// ([^}]+) - Capture group: one or more non-} characters (the variable path)
// \}\} - Match literal }}
static INTERPOLATION_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\{\{([^}]+)\}\}").expect("Invalid regex"));

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExecutionContext {
    pub workflow_id: String,
    pub execution_id: String,
    #[ts(type = "Record<string, any>")]
    pub variables: HashMap<String, Value>,
    #[ts(type = "Record<string, any>")]
    pub node_outputs: HashMap<String, Value>,
    #[ts(type = "Record<string, any>")]
    pub global_config: HashMap<String, Value>,
    #[serde(skip)]
    #[ts(skip)]
    pub secret_storage: Option<Arc<SecretStorage>>,
}

impl ExecutionContext {
    pub fn new(workflow_id: String) -> Self {
        Self {
            workflow_id,
            execution_id: uuid::Uuid::new_v4().to_string(),
            variables: HashMap::new(),
            node_outputs: HashMap::new(),
            global_config: HashMap::new(),
            secret_storage: None,
        }
    }

    pub fn with_secret_storage(mut self, storage: Arc<SecretStorage>) -> Self {
        self.secret_storage = Some(storage);
        self
    }

    pub fn set_variable(&mut self, key: String, value: Value) {
        self.variables.insert(key, value);
    }

    pub fn get_variable(&self, key: &str) -> Option<&Value> {
        self.variables.get(key)
    }

    pub fn set_node_output(&mut self, node_id: String, output: Value) {
        self.node_outputs.insert(node_id, output);
    }

    pub fn get_node_output(&self, node_id: &str) -> Option<&Value> {
        self.node_outputs.get(node_id)
    }

    pub fn interpolate_value(&self, value: &Value) -> Value {
        match value {
            Value::String(s) => {
                let mut result = s.clone();

                for cap in INTERPOLATION_REGEX.captures_iter(s) {
                    if let Some(var_path) = cap.get(1) {
                        let path = var_path.as_str();
                        if let Some(replacement) = self.resolve_path(path) {
                            result = result.replace(&cap[0], &replacement.to_string());
                        }
                    }
                }

                Value::String(result)
            }
            Value::Object(map) => {
                let mut new_map = serde_json::Map::new();
                for (k, v) in map {
                    new_map.insert(k.clone(), self.interpolate_value(v));
                }
                Value::Object(new_map)
            }
            Value::Array(arr) => {
                Value::Array(arr.iter().map(|v| self.interpolate_value(v)).collect())
            }
            _ => value.clone(),
        }
    }

    fn resolve_path(&self, path: &str) -> Option<Value> {
        let parts: Vec<&str> = path.split('.').collect();
        if parts.is_empty() {
            return None;
        }

        // TODO: Improve with explicit namespaces to avoid conflicts
        // Current: {{node_id}} - ambiguous, could be node output or variable
        // Better: {{node.http1.body}}, {{var.counter}}, {{config.api_key}}
        // This would prevent naming conflicts and make data sources explicit

        // Try node_outputs first, then variables, then global_config
        let (root, start_idx) = if let Some(output) = self.node_outputs.get(parts[0]) {
            (output, 1)
        } else if let Some(var) = self.variables.get(parts[0]) {
            (var, 1)
        } else if parts[0] == "config" && parts.len() > 1 {
            // Handle config.key pattern
            if let Some(config_val) = self.global_config.get(parts[1]) {
                (config_val, 2)
            } else {
                return None;
            }
        } else {
            return None;
        };

        // Return root if no nested path
        if parts.len() == start_idx {
            return Some(root.clone());
        }

        // Navigate nested fields
        let mut current = root;
        for part in &parts[start_idx..] {
            match current {
                Value::Object(map) => {
                    current = map.get(*part)?;
                }
                _ => return None,
            }
        }
        Some(current.clone())
    }
}
