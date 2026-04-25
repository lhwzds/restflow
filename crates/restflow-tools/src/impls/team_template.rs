//! Shared structural team storage helpers for tool implementations.

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::{Result, ToolError};
use restflow_traits::TeamTemplateDocument;
use restflow_traits::store::{TeamTemplateEntry, TeamTemplateStore};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TeamTemplateScope {
    pub namespace: &'static str,
    pub type_hint: &'static str,
    pub version: u32,
}

impl TeamTemplateScope {
    pub const fn new(namespace: &'static str, type_hint: &'static str, version: u32) -> Self {
        Self {
            namespace,
            type_hint,
            version,
        }
    }
}

pub(crate) struct TeamWriteResult<TMember> {
    pub document: TeamTemplateDocument<TMember>,
    pub storage: Value,
}

pub(crate) fn validate_team_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(ToolError::Tool("Team name must not be empty".to_string()));
    }
    if trimmed.contains(':') {
        return Err(ToolError::Tool(
            "Team name must not contain ':'.".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

pub(crate) fn is_not_found_error(error: &ToolError) -> bool {
    let text = error.to_string().to_ascii_lowercase();
    text.contains("not found") || text.contains("no such")
}

pub(crate) fn read_team_raw(
    store: &dyn TeamTemplateStore,
    namespace: &str,
    team_name: &str,
) -> Result<Option<String>> {
    let team = validate_team_name(team_name)?;
    match store.get_template(namespace, &team) {
        Ok(entry) => Ok(entry.map(|entry| entry.content)),
        Err(error) if is_not_found_error(&error) => Ok(None),
        Err(error) => Err(error),
    }
}

pub(crate) fn load_team_document<TMember>(
    store: &dyn TeamTemplateStore,
    namespace: &str,
    team_name: &str,
) -> Result<TeamTemplateDocument<TMember>>
where
    TMember: DeserializeOwned,
{
    let raw = read_team_raw(store, namespace, team_name)?
        .ok_or_else(|| ToolError::Tool(format!("Team '{}' was not found.", team_name)))?;
    serde_json::from_str(&raw)
        .map_err(|error| ToolError::Tool(format!("Failed to decode team '{team_name}': {error}")))
}

pub(crate) fn load_scoped_team_document<TMember>(
    store: &dyn TeamTemplateStore,
    scope: TeamTemplateScope,
    team_name: &str,
) -> Result<TeamTemplateDocument<TMember>>
where
    TMember: DeserializeOwned,
{
    load_team_document(store, scope.namespace, team_name)
}

pub(crate) fn save_team_document<TMember>(
    store: &dyn TeamTemplateStore,
    namespace: &str,
    type_hint: &str,
    version: u32,
    team_name: &str,
    members: Vec<TMember>,
    tags: Option<Vec<String>>,
) -> Result<TeamWriteResult<TMember>>
where
    TMember: Serialize + DeserializeOwned + Clone,
{
    if members.is_empty() {
        return Err(ToolError::Tool(
            "Cannot save team with empty members.".to_string(),
        ));
    }
    let normalized = validate_team_name(team_name)?;
    let now = chrono::Utc::now().timestamp_millis();
    let existing = read_team_raw(store, namespace, &normalized)?;
    let created_at = existing
        .as_deref()
        .and_then(|raw| serde_json::from_str::<TeamTemplateDocument<Value>>(raw).ok())
        .map(|document| document.created_at)
        .unwrap_or(now);
    let document = TeamTemplateDocument {
        version,
        name: normalized.clone(),
        members,
        created_at,
        updated_at: now,
    };
    let serialized = serde_json::to_string(&document).map_err(|error| {
        ToolError::Tool(format!("Failed to serialize team '{normalized}': {error}"))
    })?;
    let storage =
        store.save_template(namespace, &normalized, &serialized, Some(type_hint), tags)?;
    Ok(TeamWriteResult {
        document,
        storage: serde_json::to_value(storage)?,
    })
}

pub(crate) fn save_scoped_team_document<TMember>(
    store: &dyn TeamTemplateStore,
    scope: TeamTemplateScope,
    team_name: &str,
    members: Vec<TMember>,
    tags: Option<Vec<String>>,
) -> Result<TeamWriteResult<TMember>>
where
    TMember: Serialize + DeserializeOwned + Clone,
{
    save_team_document(
        store,
        scope.namespace,
        scope.type_hint,
        scope.version,
        team_name,
        members,
        tags,
    )
}

pub(crate) fn list_team_entries(
    store: &dyn TeamTemplateStore,
    namespace: &str,
) -> Result<Vec<TeamTemplateEntry>> {
    store.list_templates(namespace)
}

pub(crate) fn list_scoped_team_entries(
    store: &dyn TeamTemplateStore,
    scope: TeamTemplateScope,
) -> Result<Vec<TeamTemplateEntry>> {
    list_team_entries(store, scope.namespace)
}

pub(crate) fn delete_team_document(
    store: &dyn TeamTemplateStore,
    namespace: &str,
    team_name: &str,
) -> Result<Value> {
    let normalized = validate_team_name(team_name)?;
    let deleted = store.delete_template(namespace, &normalized)?;
    Ok(json!({
        "team": normalized,
        "result": { "deleted": deleted }
    }))
}

pub(crate) fn delete_scoped_team_document(
    store: &dyn TeamTemplateStore,
    scope: TeamTemplateScope,
    team_name: &str,
) -> Result<Value> {
    delete_team_document(store, scope.namespace, team_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MockTeamTemplateStore {
        entries: Mutex<HashMap<String, String>>,
    }

    impl TeamTemplateStore for MockTeamTemplateStore {
        fn get_template(&self, namespace: &str, team: &str) -> Result<Option<TeamTemplateEntry>> {
            let key = format!("{namespace}:{team}");
            let entries = self
                .entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            Ok(entries.get(&key).map(|content| TeamTemplateEntry {
                namespace: namespace.to_string(),
                team: team.to_string(),
                content: content.clone(),
                type_hint: None,
                tags: Vec::new(),
                created_at: 1,
                updated_at: 2,
            }))
        }

        fn save_template(
            &self,
            namespace: &str,
            team: &str,
            content: &str,
            type_hint: Option<&str>,
            tags: Option<Vec<String>>,
        ) -> Result<TeamTemplateEntry> {
            let key = format!("{namespace}:{team}");
            let mut entries = self
                .entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            entries.insert(key, content.to_string());
            Ok(TeamTemplateEntry {
                namespace: namespace.to_string(),
                team: team.to_string(),
                content: content.to_string(),
                type_hint: type_hint.map(str::to_string),
                tags: tags.unwrap_or_default(),
                created_at: 1,
                updated_at: 2,
            })
        }

        fn delete_template(&self, namespace: &str, team: &str) -> Result<bool> {
            let key = format!("{namespace}:{team}");
            let mut entries = self
                .entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            Ok(entries.remove(&key).is_some())
        }

        fn list_templates(&self, namespace: &str) -> Result<Vec<TeamTemplateEntry>> {
            let entries = self
                .entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let prefix = format!("{namespace}:");
            Ok(entries
                .iter()
                .filter_map(|(key, content)| {
                    Some(TeamTemplateEntry {
                        namespace: namespace.to_string(),
                        team: key.strip_prefix(&prefix)?.to_string(),
                        content: content.clone(),
                        type_hint: None,
                        tags: Vec::new(),
                        created_at: 1,
                        updated_at: 2,
                    })
                })
                .collect())
        }
    }

    #[test]
    fn test_save_and_load_team_document() {
        let store = MockTeamTemplateStore::default();
        let saved = save_team_document(
            &store,
            "demo_team",
            "demo_team",
            1,
            "TeamA",
            vec![json!({"count": 2})],
            None,
        )
        .unwrap();
        assert_eq!(saved.document.name, "TeamA");

        let loaded: TeamTemplateDocument<Value> =
            load_team_document(&store, "demo_team", "TeamA").unwrap();
        assert_eq!(loaded.members.len(), 1);
    }

    #[test]
    fn scoped_helpers_round_trip_document() {
        let store = MockTeamTemplateStore::default();
        let scope = TeamTemplateScope::new("subagent_team", "subagent_team", 3);

        let saved = save_scoped_team_document(
            &store,
            scope,
            "Analysts",
            vec![json!({"count": 2})],
            Some(vec!["team".to_string()]),
        )
        .unwrap();

        assert_eq!(saved.document.version, 3);

        let loaded: TeamTemplateDocument<Value> =
            load_scoped_team_document(&store, scope, "Analysts").unwrap();
        assert_eq!(loaded.name, "Analysts");
        assert_eq!(loaded.members.len(), 1);
    }

    #[test]
    fn lists_scoped_team_entries() {
        let store = MockTeamTemplateStore::default();
        let scope = TeamTemplateScope::new("background_agent_team", "background_agent_team", 2);
        save_scoped_team_document(&store, scope, "nightly", vec![json!({"count": 1})], None)
            .unwrap();

        let entries = list_scoped_team_entries(&store, scope).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].team, "nightly");
    }
}
