//! Shared structural team storage helpers for tool implementations.

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::{Result, ToolError};
use restflow_traits::TeamTemplateDocument;
use restflow_traits::store::KvStore;

const TEAM_CONTENT_TYPE: &str = "application/json";
const TEAM_VISIBILITY: &str = "shared";

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

    pub fn key_prefix(self) -> String {
        format!("{}:", self.namespace)
    }

    pub fn team_name_from_entry(self, entry: &Value) -> Option<String> {
        let key = entry.get("key")?.as_str()?;
        let prefix = self.key_prefix();
        key.strip_prefix(&prefix).map(str::to_string)
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
            "Team name must not contain ':'".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

pub(crate) fn team_key(namespace: &str, team_name: &str) -> Result<String> {
    let normalized = validate_team_name(team_name)?;
    Ok(format!("{namespace}:{normalized}"))
}

pub(crate) fn is_not_found_error(error: &ToolError) -> bool {
    let text = error.to_string().to_ascii_lowercase();
    text.contains("not found") || text.contains("no such")
}

pub(crate) fn read_team_raw(
    store: &dyn KvStore,
    namespace: &str,
    team_name: &str,
) -> Result<Option<String>> {
    let key = team_key(namespace, team_name)?;
    let payload = match store.get_entry(&key) {
        Ok(payload) => payload,
        Err(error) if is_not_found_error(&error) => return Ok(None),
        Err(error) => return Err(error),
    };
    if !payload
        .get("found")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(None);
    }
    let raw = payload
        .get("value")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::Tool("Stored team payload is invalid.".to_string()))?;
    Ok(Some(raw.to_string()))
}

pub(crate) fn load_team_document<TMember>(
    store: &dyn KvStore,
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
    store: &dyn KvStore,
    scope: TeamTemplateScope,
    team_name: &str,
) -> Result<TeamTemplateDocument<TMember>>
where
    TMember: DeserializeOwned,
{
    load_team_document(store, scope.namespace, team_name)
}

pub(crate) fn save_team_document<TMember>(
    store: &dyn KvStore,
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
    let key = team_key(namespace, &normalized)?;
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
    let storage = store.set_entry(
        &key,
        &serialized,
        Some(TEAM_VISIBILITY),
        Some(TEAM_CONTENT_TYPE),
        Some(type_hint),
        tags,
        None,
    )?;
    Ok(TeamWriteResult { document, storage })
}

pub(crate) fn save_scoped_team_document<TMember>(
    store: &dyn KvStore,
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

pub(crate) fn list_team_entries(store: &dyn KvStore, namespace: &str) -> Result<Vec<Value>> {
    let payload = store.list_entries(Some(namespace))?;
    Ok(payload
        .get("entries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

pub(crate) fn list_scoped_team_entries(
    store: &dyn KvStore,
    scope: TeamTemplateScope,
) -> Result<Vec<Value>> {
    list_team_entries(store, scope.namespace)
}

pub(crate) fn delete_team_document(
    store: &dyn KvStore,
    namespace: &str,
    team_name: &str,
) -> Result<Value> {
    let key = team_key(namespace, team_name)?;
    let deleted = store.delete_entry(&key, None)?;
    Ok(json!({
        "team": validate_team_name(team_name)?,
        "result": deleted
    }))
}

pub(crate) fn delete_scoped_team_document(
    store: &dyn KvStore,
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
    struct MockKvStore {
        entries: Mutex<HashMap<String, String>>,
    }

    impl KvStore for MockKvStore {
        fn get_entry(&self, key: &str) -> Result<Value> {
            let entries = self
                .entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(value) = entries.get(key) {
                Ok(json!({
                    "found": true,
                    "key": key,
                    "value": value
                }))
            } else {
                Ok(json!({
                    "found": false,
                    "key": key
                }))
            }
        }

        fn set_entry(
            &self,
            key: &str,
            content: &str,
            _visibility: Option<&str>,
            _content_type: Option<&str>,
            _type_hint: Option<&str>,
            _tags: Option<Vec<String>>,
            _accessor_id: Option<&str>,
        ) -> Result<Value> {
            let mut entries = self
                .entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            entries.insert(key.to_string(), content.to_string());
            Ok(json!({ "success": true, "key": key }))
        }

        fn delete_entry(&self, key: &str, _accessor_id: Option<&str>) -> Result<Value> {
            let mut entries = self
                .entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            Ok(json!({ "deleted": entries.remove(key).is_some() }))
        }

        fn list_entries(&self, namespace: Option<&str>) -> Result<Value> {
            let entries = self
                .entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let prefix = namespace.map(|value| format!("{value}:"));
            let rows = entries
                .keys()
                .filter(|key| {
                    prefix
                        .as_ref()
                        .map(|value| key.starts_with(value))
                        .unwrap_or(true)
                })
                .map(|key| json!({ "key": key }))
                .collect::<Vec<_>>();
            Ok(json!({ "entries": rows }))
        }
    }

    #[test]
    fn test_save_and_load_team_document() {
        let store = MockKvStore::default();
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
        let store = MockKvStore::default();
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
    fn scope_extracts_team_name_from_storage_entry() {
        let scope = TeamTemplateScope::new("background_agent_team", "background_agent_team", 2);
        let entry = json!({"key": "background_agent_team:nightly"});

        assert_eq!(
            scope.team_name_from_entry(&entry),
            Some("nightly".to_string())
        );
    }
}
