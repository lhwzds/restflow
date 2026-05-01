//! Shared runtime payload types.

use serde::{Deserialize, Serialize};

/// Runtime payload passed when expanding batch work.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct RuntimeTaskPayload {
    #[serde(default)]
    pub task: Option<String>,
    #[serde(default)]
    pub tasks: Option<Vec<String>>,
}

impl RuntimeTaskPayload {
    /// Validate that single and multi payloads are not combined.
    pub fn validate(&self, single_label: &str, multi_label: &str) -> Result<(), String> {
        if self.task.is_some() && self.tasks.is_some() {
            return Err(format!(
                "Use either '{}' or '{}', not both.",
                single_label, multi_label
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_task_payload_rejects_mixed_modes() {
        let payload = RuntimeTaskPayload {
            task: Some("a".to_string()),
            tasks: Some(vec!["b".to_string()]),
        };

        let error = payload.validate("task", "tasks").unwrap_err();
        assert!(error.contains("either 'task' or 'tasks'"));
    }
}
