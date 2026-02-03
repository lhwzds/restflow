//! Agent metadata model for file-based agent definitions.

use crate::models::{ApiKeyConfig, AIModel};
use crate::models::skill::StorageMode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ts_rs::TS;

/// Agent metadata stored in the database for file-based agents.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentMeta {
    /// Unique identifier for the agent
    pub id: String,
    /// Display name of the agent
    pub name: String,
    /// Optional folder path for file-based agents (e.g. "agents/main-assistant")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder_path: Option<String>,
    /// Optional content hash for change detection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    /// Optional AI model override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<AIModel>,
    /// Optional temperature override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// Optional API key configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_config: Option<ApiKeyConfig>,
    /// Optional tools list override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
    /// Optional skill IDs to load
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<String>>,
    /// Optional skill variables for prompt substitution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_variables: Option<HashMap<String, String>>,
    /// Type classification for the agent
    pub agent_type: AgentType,
    /// Storage mode for the agent definition
    #[serde(default)]
    pub storage_mode: StorageMode,
    /// Whether the agent is synced between storage modes
    #[serde(default)]
    pub is_synced: bool,
    /// Timestamp when the agent was created (milliseconds since epoch)
    #[ts(type = "number")]
    pub created_at: i64,
    /// Timestamp when the agent was last updated (milliseconds since epoch)
    #[ts(type = "number")]
    pub updated_at: i64,
}

impl AgentMeta {
    /// Create a new agent metadata entry with required fields.
    pub fn new(id: String, name: String, agent_type: AgentType) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id,
            name,
            folder_path: None,
            content_hash: None,
            model: None,
            temperature: None,
            api_key_config: None,
            tools: None,
            skills: None,
            skill_variables: None,
            agent_type,
            storage_mode: StorageMode::DatabaseOnly,
            is_synced: false,
            created_at: now,
            updated_at: now,
        }
    }

    /// Update mutable metadata fields.
    pub fn touch(&mut self) {
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }
}

/// Agent type classification.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum AgentType {
    Main,
    Cron,
    Sub,
    Inline,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_meta_new() {
        let meta = AgentMeta::new(
            "main-assistant".to_string(),
            "Main Assistant".to_string(),
            AgentType::Main,
        );

        assert_eq!(meta.id, "main-assistant");
        assert_eq!(meta.name, "Main Assistant");
        assert!(matches!(meta.agent_type, AgentType::Main));
        assert_eq!(meta.storage_mode, StorageMode::DatabaseOnly);
        assert!(!meta.is_synced);
    }
}
