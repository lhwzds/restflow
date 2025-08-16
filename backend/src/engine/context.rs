use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    pub workflow_id: String,
    pub execution_id: String,
    pub variables: HashMap<String, Value>,
    pub node_outputs: HashMap<String, Value>,
    pub global_config: HashMap<String, Value>,
}

impl ExecutionContext {
    pub fn new(workflow_id: String) -> Self {
        Self {
            workflow_id,
            execution_id: uuid::Uuid::new_v4().to_string(),
            variables: HashMap::new(),
            node_outputs: HashMap::new(),
            global_config: HashMap::new(),
        }
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

                let re = regex::Regex::new(r"\{\{([^}]+)\}\}").unwrap();
                for cap in re.captures_iter(s) {
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

        if let Some(output) = self.node_outputs.get(parts[0]) {
            if parts.len() == 1 {
                return Some(output.clone());
            }

            let mut current = output;
            for part in &parts[1..] {
                match current {
                    Value::Object(map) => {
                        if let Some(next) = map.get(*part) {
                            current = next;
                        } else {
                            return None;
                        }
                    }
                    _ => return None,
                }
            }
            return Some(current.clone());
        }

        if let Some(var) = self.variables.get(parts[0]) {
            if parts.len() == 1 {
                return Some(var.clone());
            }
            let mut current = var;
            for part in &parts[1..] {
                match current {
                    Value::Object(map) => {
                        if let Some(next) = map.get(*part) {
                            current = next;
                        } else {
                            return None;
                        }
                    }
                    _ => return None,
                }
            }
            return Some(current.clone());
        }

        None
    }
}
