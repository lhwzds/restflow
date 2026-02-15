//! Shared space data models.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SharedEntry {
    pub key: String,
    pub value: String,
    pub visibility: Visibility,
    pub owner: Option<String>,
    pub content_type: Option<String>,
    pub type_hint: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_modified_by: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, Default, PartialEq)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    #[default]
    Public,
    Shared,
    Private,
}

impl SharedEntry {
    /// Check if accessor can read this entry
    pub fn can_read(&self, accessor_id: Option<&str>) -> bool {
        match self.visibility {
            Visibility::Public | Visibility::Shared => true,
            Visibility::Private => self.owner.as_deref() == accessor_id,
        }
    }

    /// Check if accessor can write this entry
    pub fn can_write(&self, accessor_id: Option<&str>) -> bool {
        match self.visibility {
            Visibility::Public => true,
            Visibility::Shared | Visibility::Private => self.owner.as_deref() == accessor_id,
        }
    }
}
