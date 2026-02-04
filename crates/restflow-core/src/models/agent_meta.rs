use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::models::{AIModel, ApiKeyConfig, StorageMode};

/// Agent metadata stored in the database (file content lives on disk).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentMeta {
    pub id: String,
    pub name: String,
    /// Optional folder path for agents stored on disk
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder_path: Option<String>,
    /// Optional content hash for change detection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    /// AI model configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<AIModel>,
    /// Temperature setting for model responses
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// API key configuration (direct or from secret)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_config: Option<ApiKeyConfig>,
    /// List of tool names the agent is allowed to use
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
    /// List of skill IDs to load into the system prompt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<String>>,
    /// Variables available for skill prompt substitution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_variables: Option<HashMap<String, String>>,
    /// Type identifier for the agent
    pub agent_type: AgentType,
    /// Storage mode for the agent
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

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export)]
pub enum AgentType {
    Main,
    Cron,
    Sub,
    Inline,
}
