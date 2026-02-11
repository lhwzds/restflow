use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, Default, PartialEq, Eq)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum NoteStatus {
    #[default]
    Open,
    InProgress,
    Done,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export)]
pub struct WorkspaceNote {
    pub id: String,
    pub folder: String,
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub priority: Option<String>,
    pub status: NoteStatus,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub assignee: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkspaceNoteSpec {
    pub folder: String,
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct WorkspaceNotePatch {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub status: Option<NoteStatus>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub folder: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export)]
pub struct NoteQuery {
    #[serde(default)]
    pub folder: Option<String>,
    #[serde(default)]
    pub status: Option<NoteStatus>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub search: Option<String>,
}
