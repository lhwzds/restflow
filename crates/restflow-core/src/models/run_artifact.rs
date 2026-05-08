use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunArtifactKind {
    FinalOutput,
    ToolOutput,
    Report,
    Data,
    File,
    Artifact,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct RunArtifact {
    pub id: String,
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub kind: RunArtifactKind,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    pub size_bytes: usize,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<BTreeMap<String, String>>,
}

impl RunArtifact {
    pub fn has_payload(&self) -> bool {
        self.content
            .as_deref()
            .is_some_and(|content| !content.trim().is_empty())
            || self
                .content_ref
                .as_deref()
                .is_some_and(|content_ref| !content_ref.trim().is_empty())
    }
}
