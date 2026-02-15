use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

/// Type of deliverable produced by an agent.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum DeliverableType {
    /// Markdown report or summary.
    Report,
    /// Structured JSON data.
    Data,
    /// File reference (path to generated file).
    File,
    /// Generic artifact (code snippet, config, etc.).
    Artifact,
}

/// A typed output produced by a background agent execution.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct Deliverable {
    pub id: String,
    pub task_id: String,
    pub execution_id: String,
    pub deliverable_type: DeliverableType,
    pub title: String,
    pub content: String,
    /// Optional file path for File type.
    #[serde(default)]
    pub file_path: Option<String>,
    /// MIME type hint (e.g., "text/markdown", "application/json").
    #[serde(default)]
    pub content_type: Option<String>,
    /// Size in bytes.
    pub size_bytes: usize,
    /// Unix timestamp in milliseconds.
    pub created_at: i64,
    /// Optional metadata (key-value pairs).
    #[serde(default)]
    pub metadata: Option<BTreeMap<String, String>>,
}
