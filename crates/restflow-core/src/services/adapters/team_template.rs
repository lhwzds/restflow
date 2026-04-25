//! Team template adapter backed by typed team template storage.

use crate::storage::TeamTemplateStorage;
use chrono::Utc;
use restflow_tools::{Result, ToolError};
use restflow_traits::store::{TeamTemplateEntry, TeamTemplateStore};

#[derive(Clone)]
pub struct TeamTemplateStoreAdapter {
    storage: TeamTemplateStorage,
}

impl TeamTemplateStoreAdapter {
    pub fn new(storage: TeamTemplateStorage) -> Self {
        Self { storage }
    }
}

impl TeamTemplateStore for TeamTemplateStoreAdapter {
    fn get_template(&self, namespace: &str, team: &str) -> Result<Option<TeamTemplateEntry>> {
        self.storage.get(namespace, team).map_err(ToolError::from)
    }

    fn save_template(
        &self,
        namespace: &str,
        team: &str,
        content: &str,
        type_hint: Option<&str>,
        tags: Option<Vec<String>>,
    ) -> Result<TeamTemplateEntry> {
        let now = Utc::now().timestamp_millis();
        let existing = self.storage.get(namespace, team).map_err(ToolError::from)?;
        let entry = TeamTemplateEntry {
            namespace: namespace.to_string(),
            team: team.to_string(),
            content: content.to_string(),
            type_hint: type_hint.map(str::to_string),
            tags: tags.unwrap_or_default(),
            created_at: existing.as_ref().map(|item| item.created_at).unwrap_or(now),
            updated_at: now,
        };
        self.storage.save(&entry).map_err(ToolError::from)?;
        Ok(entry)
    }

    fn delete_template(&self, namespace: &str, team: &str) -> Result<bool> {
        self.storage
            .delete(namespace, team)
            .map_err(ToolError::from)
    }

    fn list_templates(&self, namespace: &str) -> Result<Vec<TeamTemplateEntry>> {
        self.storage.list(namespace).map_err(ToolError::from)
    }
}
