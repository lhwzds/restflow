//! Typed storage for reusable team templates.

use anyhow::Result;
use redb::Database;
use restflow_traits::store::TeamTemplateEntry;
use std::sync::Arc;

#[derive(Clone)]
pub struct TeamTemplateStorage {
    inner: restflow_storage::TeamTemplateStorage,
}

impl TeamTemplateStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Ok(Self {
            inner: restflow_storage::TeamTemplateStorage::new(db)?,
        })
    }

    pub fn get(&self, namespace: &str, team: &str) -> Result<Option<TeamTemplateEntry>> {
        let Some(bytes) = self.inner.get_raw(&template_key(namespace, team))? else {
            return Ok(None);
        };
        Ok(Some(serde_json::from_slice(&bytes)?))
    }

    pub fn save(&self, entry: &TeamTemplateEntry) -> Result<()> {
        let bytes = serde_json::to_vec(entry)?;
        self.inner
            .put_raw(&template_key(&entry.namespace, &entry.team), &bytes)
    }

    pub fn delete(&self, namespace: &str, team: &str) -> Result<bool> {
        self.inner.delete(&template_key(namespace, team))
    }

    pub fn list(&self, namespace: &str) -> Result<Vec<TeamTemplateEntry>> {
        let mut entries = self
            .inner
            .list_raw(namespace)?
            .into_iter()
            .map(|(_, bytes)| serde_json::from_slice::<TeamTemplateEntry>(&bytes))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        entries.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.team.cmp(&right.team))
        });
        Ok(entries)
    }
}

fn template_key(namespace: &str, team: &str) -> String {
    format!("{namespace}:{team}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn stores_and_lists_templates() {
        let dir = tempdir().expect("temp dir should be created");
        let db_path = dir.path().join("team-template-core.db");
        let db = Arc::new(Database::create(db_path).expect("db should be created"));
        let storage = TeamTemplateStorage::new(db).expect("storage should be created");

        storage
            .save(&TeamTemplateEntry {
                namespace: "subagent_team".to_string(),
                team: "review".to_string(),
                content: "{}".to_string(),
                type_hint: Some("subagent_team".to_string()),
                tags: vec!["subagent".to_string()],
                created_at: 1,
                updated_at: 2,
            })
            .expect("save should succeed");

        let loaded = storage
            .get("subagent_team", "review")
            .expect("get should succeed")
            .expect("entry should exist");
        assert_eq!(loaded.team, "review");
        assert_eq!(storage.list("subagent_team").unwrap().len(), 1);
        assert!(storage.delete("subagent_team", "review").unwrap());
    }
}
