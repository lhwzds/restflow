//! Secret model for secure credential storage.
//! Values are base64 encoded in storage for additional security.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Secret {
    pub key: String,
    pub value: String,
    pub description: Option<String>,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
}

impl Secret {
    pub fn new(key: String, value: String, description: Option<String>) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            key,
            value,
            description,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn update(&mut self, value: String, description: Option<String>) {
        self.value = value;
        self.description = description;
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }
}
