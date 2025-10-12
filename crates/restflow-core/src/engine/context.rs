use crate::storage::{SecretStorage, Storage};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use ts_rs::TS;

/// ExecutionContext namespace constants
/// Provides type-safe key builders to avoid hardcoded strings
pub mod namespace {
    /// Trigger-related keys
    pub mod trigger {
        /// Trigger input data (webhook payload, manual input, schedule time)
        pub const PAYLOAD: &str = "trigger.payload";
    }

    /// Builds node output key: node.{id}
    pub fn node(id: &str) -> String {
        format!("node.{}", id)
    }

    /// Builds variable key: var.{name}
    pub fn var(name: &str) -> String {
        format!("var.{}", name)
    }

    /// Builds config key: config.{name}
    pub fn config(name: &str) -> String {
        format!("config.{}", name)
    }
}

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
    /// Unified data storage with namespace prefixes:
    /// - trigger.* : Trigger outputs
    /// - node.*    : Node outputs
    /// - var.*     : User variables
    /// - config.*  : Global configuration
    #[ts(type = "Record<string, any>")]
    pub data: HashMap<String, Value>,
    #[serde(skip)]
    #[ts(skip)]
    pub secret_storage: Option<Arc<SecretStorage>>,
}

impl ExecutionContext {
    pub fn new(workflow_id: String) -> Self {
        Self {
            workflow_id,
            execution_id: uuid::Uuid::new_v4().to_string(),
            data: HashMap::new(),
            secret_storage: None,
        }
    }

    pub fn with_secret_storage(mut self, storage: Arc<SecretStorage>) -> Self {
        self.secret_storage = Some(storage);
        self
    }

    pub fn ensure_secret_storage(&mut self, storage: &Storage) {
        if self.secret_storage.is_none() {
            self.secret_storage = Some(Arc::new(storage.secrets.clone()));
        }
    }

    /// Set data directly (recommended, key must include namespace)
    pub fn set(&mut self, key: &str, value: Value) {
        self.data.insert(key.to_string(), value);
    }

    /// Get data directly (recommended, key must include namespace)
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.data.get(key)
    }

    /// Set variable (convenience method, automatically adds var. prefix)
    pub fn set_var(&mut self, name: &str, value: Value) {
        self.set(&namespace::var(name), value);
    }

    /// Get variable (convenience method, automatically adds var. prefix)
    pub fn get_var(&self, name: &str) -> Option<&Value> {
        self.get(&namespace::var(name))
    }

    /// Set node output (convenience method, automatically adds node. prefix)
    pub fn set_node(&mut self, id: &str, output: Value) {
        self.set(&namespace::node(id), output);
    }

    /// Get node output (convenience method, automatically adds node. prefix)
    pub fn get_node(&self, id: &str) -> Option<&Value> {
        self.get(&namespace::node(id))
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

        // New logic: explicit namespaces
        // Supported formats:
        // - trigger.payload.body → data["trigger.payload"], then navigate to .body
        // - node.http1.status → data["node.http1"], then navigate to .status
        // - var.counter → data["var.counter"]
        // - config.api_key → data["config.api_key"]

        // Try two-level key (namespace.name)
        if parts.len() >= 2 {
            let two_level_key = format!("{}.{}", parts[0], parts[1]);
            if let Some(root) = self.data.get(&two_level_key) {
                // If there's a deeper path, continue navigation
                if parts.len() > 2 {
                    return Self::navigate_nested(root, &parts[2..]);
                } else {
                    return Some(root.clone());
                }
            }
        }

        // Try single-level key (direct lookup of full path)
        self.data.get(path).cloned()
    }

    /// Navigate nested fields in Value object
    fn navigate_nested(mut current: &Value, parts: &[&str]) -> Option<Value> {
        for part in parts {
            current = current.as_object()?.get(*part)?;
        }
        Some(current.clone())
    }
}
