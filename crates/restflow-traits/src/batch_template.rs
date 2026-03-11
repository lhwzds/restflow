//! Shared team template and runtime payload types.

use serde::{Deserialize, Serialize};

/// Persisted structural team document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TeamTemplateDocument<TMember> {
    pub version: u32,
    pub name: String,
    pub members: Vec<TMember>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Runtime payload passed when spawning one saved team.
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

    #[test]
    fn test_team_template_document_round_trip() {
        let document = TeamTemplateDocument {
            version: 2,
            name: "TeamA".to_string(),
            members: vec!["one".to_string(), "two".to_string()],
            created_at: 1,
            updated_at: 2,
        };

        let encoded = serde_json::to_string(&document).unwrap();
        let decoded: TeamTemplateDocument<String> = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.members.len(), 2);
        assert_eq!(decoded.name, "TeamA");
    }
}
