use crate::python::PythonManager;
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
    #[serde(skip)]
    #[ts(skip)]
    pub python_manager: Option<Arc<PythonManager>>,
}

impl ExecutionContext {
    pub fn new(workflow_id: String) -> Self {
        Self {
            workflow_id,
            execution_id: uuid::Uuid::new_v4().to_string(),
            data: HashMap::new(),
            secret_storage: None,
            python_manager: None,
        }
    }

    /// Create execution context with specific execution_id (for test executions)
    pub fn with_execution_id(workflow_id: String, execution_id: String) -> Self {
        Self {
            workflow_id,
            execution_id,
            data: HashMap::new(),
            secret_storage: None,
            python_manager: None,
        }
    }

    pub fn with_secret_storage(mut self, storage: Arc<SecretStorage>) -> Self {
        self.secret_storage = Some(storage);
        self
    }

    pub fn with_python_manager(mut self, manager: Arc<PythonManager>) -> Self {
        self.python_manager = Some(manager);
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
                            // For string values, extract the string content without quotes
                            // For other types (numbers, booleans, objects), use to_string()
                            let replacement_str = match &replacement {
                                Value::String(s) => s.clone(),
                                _ => replacement.to_string(),
                            };
                            result = result.replace(&cap[0], &replacement_str);
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_new_context() {
        let ctx = ExecutionContext::new("wf-001".to_string());
        assert_eq!(ctx.workflow_id, "wf-001");
        assert!(!ctx.execution_id.is_empty());
        assert!(ctx.data.is_empty());
    }

    #[test]
    fn test_with_execution_id() {
        let ctx = ExecutionContext::with_execution_id("wf-001".to_string(), "exec-001".to_string());
        assert_eq!(ctx.workflow_id, "wf-001");
        assert_eq!(ctx.execution_id, "exec-001");
    }

    #[test]
    fn test_set_get_direct() {
        let mut ctx = ExecutionContext::new("wf-001".to_string());
        ctx.set("custom.key", json!({"value": 42}));

        let result = ctx.get("custom.key");
        assert!(result.is_some());
        assert_eq!(result.unwrap()["value"], 42);
    }

    #[test]
    fn test_set_get_var() {
        let mut ctx = ExecutionContext::new("wf-001".to_string());
        ctx.set_var("counter", json!(10));

        let result = ctx.get_var("counter");
        assert_eq!(result, Some(&json!(10)));

        // Verify it's stored with var. prefix
        assert_eq!(ctx.get("var.counter"), Some(&json!(10)));
    }

    #[test]
    fn test_set_get_node() {
        let mut ctx = ExecutionContext::new("wf-001".to_string());
        ctx.set_node("http1", json!({
            "status": 200,
            "body": {"message": "success"}
        }));

        let result = ctx.get_node("http1");
        assert!(result.is_some());
        assert_eq!(result.unwrap()["status"], 200);

        // Verify it's stored with node. prefix
        assert_eq!(ctx.get("node.http1").unwrap()["status"], 200);
    }

    #[test]
    fn test_interpolate_simple_variable() {
        let mut ctx = ExecutionContext::new("wf-001".to_string());
        ctx.set_var("name", json!("Alice"));

        let input = json!("Hello {{var.name}}!");
        let result = ctx.interpolate_value(&input);

        assert_eq!(result, json!("Hello Alice!"));
    }

    #[test]
    fn test_interpolate_node_output() {
        let mut ctx = ExecutionContext::new("wf-001".to_string());
        ctx.set_node("http1", json!({"status": 200}));

        let input = json!("Status: {{node.http1.status}}");
        let result = ctx.interpolate_value(&input);

        assert_eq!(result, json!("Status: 200"));
    }

    #[test]
    fn test_interpolate_trigger_payload() {
        let mut ctx = ExecutionContext::new("wf-001".to_string());
        ctx.set(namespace::trigger::PAYLOAD, json!({
            "user": "bob",
            "action": "login"
        }));

        let input = json!("User {{trigger.payload.user}} performed {{trigger.payload.action}}");
        let result = ctx.interpolate_value(&input);

        assert_eq!(result, json!("User bob performed login"));
    }

    #[test]
    fn test_interpolate_multiple_variables() {
        let mut ctx = ExecutionContext::new("wf-001".to_string());
        ctx.set_var("first", json!("John"));
        ctx.set_var("last", json!("Doe"));

        let input = json!("Name: {{var.first}} {{var.last}}");
        let result = ctx.interpolate_value(&input);

        assert_eq!(result, json!("Name: John Doe"));
    }

    #[test]
    fn test_interpolate_nested_object() {
        let mut ctx = ExecutionContext::new("wf-001".to_string());
        ctx.set_var("url", json!("https://api.example.com"));

        let input = json!({
            "endpoint": "{{var.url}}/users",
            "method": "GET"
        });
        let result = ctx.interpolate_value(&input);

        assert_eq!(result["endpoint"], "https://api.example.com/users");
        assert_eq!(result["method"], "GET");
    }

    #[test]
    fn test_interpolate_array() {
        let mut ctx = ExecutionContext::new("wf-001".to_string());
        ctx.set_var("item", json!("apple"));

        let input = json!(["{{var.item}}", "banana", "{{var.item}}"]);
        let result = ctx.interpolate_value(&input);

        assert_eq!(result[0], "apple");
        assert_eq!(result[1], "banana");
        assert_eq!(result[2], "apple");
    }

    #[test]
    fn test_interpolate_deeply_nested() {
        let mut ctx = ExecutionContext::new("wf-001".to_string());
        ctx.set_node("api", json!({
            "response": {
                "data": {
                    "items": [{"id": 1}, {"id": 2}]
                }
            }
        }));

        let input = json!({
            "result": {
                "nested": {
                    "value": "ID: {{node.api.response}}"
                }
            }
        });
        let result = ctx.interpolate_value(&input);

        // Should interpolate to JSON string representation
        let nested = &result["result"]["nested"]["value"];
        assert!(nested.as_str().unwrap().contains("data"));
    }

    #[test]
    fn test_interpolate_non_existent_variable() {
        let ctx = ExecutionContext::new("wf-001".to_string());

        let input = json!("Value: {{var.nonexistent}}");
        let result = ctx.interpolate_value(&input);

        // Should remain unchanged
        assert_eq!(result, json!("Value: {{var.nonexistent}}"));
    }

    #[test]
    fn test_interpolate_non_string_values() {
        let ctx = ExecutionContext::new("wf-001".to_string());

        let input = json!({
            "number": 42,
            "boolean": true,
            "null": null
        });
        let result = ctx.interpolate_value(&input);

        assert_eq!(result["number"], 42);
        assert_eq!(result["boolean"], true);
        assert_eq!(result["null"], json!(null));
    }

    #[test]
    fn test_resolve_path_two_level_key() {
        let mut ctx = ExecutionContext::new("wf-001".to_string());
        ctx.set("node.http1", json!({"status": 200}));

        let result = ctx.resolve_path("node.http1");
        assert_eq!(result, Some(json!({"status": 200})));
    }

    #[test]
    fn test_resolve_path_with_nested_navigation() {
        let mut ctx = ExecutionContext::new("wf-001".to_string());
        ctx.set("node.http1", json!({
            "response": {
                "data": {
                    "user": {"name": "Alice"}
                }
            }
        }));

        let result = ctx.resolve_path("node.http1.response.data.user.name");
        assert_eq!(result, Some(json!("Alice")));
    }

    #[test]
    fn test_navigate_nested_valid_path() {
        let value = json!({
            "level1": {
                "level2": {
                    "level3": "deep_value"
                }
            }
        });

        let result = ExecutionContext::navigate_nested(&value, &["level1", "level2", "level3"]);
        assert_eq!(result, Some(json!("deep_value")));
    }

    #[test]
    fn test_navigate_nested_invalid_path() {
        let value = json!({"a": {"b": "value"}});

        let result = ExecutionContext::navigate_nested(&value, &["a", "nonexistent", "c"]);
        assert_eq!(result, None);
    }

    #[test]
    fn test_interpolation_regex_performance() {
        let mut ctx = ExecutionContext::new("wf-001".to_string());
        ctx.set_var("x", json!(1));

        // Test that INTERPOLATION_REGEX is lazily initialized and reused
        for _ in 0..100 {
            let input = json!("Value: {{var.x}}");
            let result = ctx.interpolate_value(&input);
            assert_eq!(result, json!("Value: 1"));
        }
    }

    #[test]
    fn test_namespace_constants() {
        assert_eq!(namespace::trigger::PAYLOAD, "trigger.payload");
        assert_eq!(namespace::node("test"), "node.test");
        assert_eq!(namespace::var("counter"), "var.counter");
        assert_eq!(namespace::config("api_key"), "config.api_key");
    }

    #[test]
    fn test_complex_workflow_context() {
        let mut ctx = ExecutionContext::new("wf-001".to_string());

        // Simulate a workflow with trigger, multiple nodes
        ctx.set(namespace::trigger::PAYLOAD, json!({
            "webhook": {
                "body": {"user_id": 123}
            }
        }));

        ctx.set_node("fetch_user", json!({
            "status": 200,
            "user": {"name": "Bob", "email": "bob@example.com"}
        }));

        ctx.set_var("notification_template", json!("Hello {{var.username}}!"));
        ctx.set_var("username", json!("Bob"));

        // Test interpolation across namespaces
        let template = json!({
            "to": "{{node.fetch_user.user.email}}",
            "subject": "Welcome",
            "body": "User ID: {{trigger.payload.webhook.body.user_id}}, Name: {{node.fetch_user.user.name}}"
        });

        let result = ctx.interpolate_value(&template);
        assert_eq!(result["to"], "bob@example.com");
        assert_eq!(result["body"], "User ID: 123, Name: Bob");
    }
}
