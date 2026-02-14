use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use anyhow::{Result, anyhow, bail};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use regex::Regex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const SECURITY_AMENDMENTS_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("security_amendments");
const MAX_REGEX_PATTERN_LEN: usize = 512;
const MAX_CACHED_REGEX_PATTERNS: usize = 1024;
static REGEX_CACHE: OnceLock<RwLock<HashMap<String, Arc<Regex>>>> = OnceLock::new();

fn regex_cache() -> &'static RwLock<HashMap<String, Arc<Regex>>> {
    REGEX_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn compile_and_cache_regex(pattern: &str) -> Option<Arc<Regex>> {
    if pattern.len() > MAX_REGEX_PATTERN_LEN {
        return None;
    }

    if let Ok(cache) = regex_cache().read()
        && let Some(cached) = cache.get(pattern)
    {
        return Some(Arc::clone(cached));
    }

    let compiled = Arc::new(Regex::new(pattern).ok()?);

    if let Ok(mut cache) = regex_cache().write() {
        if let Some(existing) = cache.get(pattern) {
            return Some(Arc::clone(existing));
        }

        if cache.len() >= MAX_CACHED_REGEX_PATTERNS {
            cache.clear();
        }
        cache.insert(pattern.to_string(), Arc::clone(&compiled));
    }

    Some(compiled)
}

fn validate_regex_pattern(pattern: &str) -> Result<()> {
    if pattern.len() > MAX_REGEX_PATTERN_LEN {
        bail!(
            "Regex pattern length {} exceeds max {}",
            pattern.len(),
            MAX_REGEX_PATTERN_LEN
        );
    }
    Regex::new(pattern).map_err(|err| anyhow!("Invalid regex pattern: {}", err))?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AmendmentMatchType {
    Exact,
    Prefix,
    Regex,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AmendmentScope {
    Workspace,
    Agent { agent_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityAmendment {
    pub id: String,
    pub tool_name: String,
    pub command_pattern: String,
    pub match_type: AmendmentMatchType,
    pub scope: AmendmentScope,
    pub enabled: bool,
    pub created_at_ms: i64,
}

impl SecurityAmendment {
    pub fn matches(&self, tool_name: &str, command: &str, agent_id: Option<&str>) -> bool {
        if !self.enabled || self.tool_name != tool_name {
            return false;
        }

        match &self.scope {
            AmendmentScope::Workspace => {}
            AmendmentScope::Agent {
                agent_id: amendment_agent_id,
            } => {
                if Some(amendment_agent_id.as_str()) != agent_id {
                    return false;
                }
            }
        }

        match self.match_type {
            AmendmentMatchType::Exact => self.command_pattern == command,
            AmendmentMatchType::Prefix => command.starts_with(&self.command_pattern),
            AmendmentMatchType::Regex => compile_and_cache_regex(&self.command_pattern)
                .map(|pattern| pattern.is_match(command))
                .unwrap_or(false),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SecurityAmendmentStore {
    db: Arc<Database>,
}

impl SecurityAmendmentStore {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        let write_txn = db.begin_write()?;
        {
            let _ = write_txn.open_table(SECURITY_AMENDMENTS_TABLE)?;
        }
        write_txn.commit()?;
        Ok(Self { db })
    }

    pub fn add_allow_rule(
        &self,
        tool_name: impl Into<String>,
        command_pattern: impl Into<String>,
        match_type: AmendmentMatchType,
        scope: AmendmentScope,
    ) -> Result<SecurityAmendment> {
        let command_pattern = command_pattern.into();
        if matches!(match_type, AmendmentMatchType::Regex) {
            validate_regex_pattern(&command_pattern)?;
        }

        let rule = SecurityAmendment {
            id: format!("amendment-{}", Uuid::new_v4()),
            tool_name: tool_name.into(),
            command_pattern,
            match_type,
            scope,
            enabled: true,
            created_at_ms: chrono::Utc::now().timestamp_millis(),
        };

        let payload = serde_json::to_vec(&rule)?;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SECURITY_AMENDMENTS_TABLE)?;
            table.insert(rule.id.as_str(), payload.as_slice())?;
        }
        write_txn.commit()?;

        Ok(rule)
    }

    pub fn list_rules(&self) -> Result<Vec<SecurityAmendment>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SECURITY_AMENDMENTS_TABLE)?;
        let mut rules = Vec::new();
        for entry in table.iter()? {
            let (_, value) = entry?;
            let rule: SecurityAmendment = serde_json::from_slice(value.value())?;
            rules.push(rule);
        }
        rules.sort_by(|a, b| b.created_at_ms.cmp(&a.created_at_ms));
        Ok(rules)
    }

    pub fn find_matching_rule(
        &self,
        tool_name: &str,
        command: &str,
        agent_id: Option<&str>,
    ) -> Result<Option<SecurityAmendment>> {
        let matching = self
            .list_rules()?
            .into_iter()
            .find(|rule| rule.matches(tool_name, command, agent_id));
        Ok(matching)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_store() -> SecurityAmendmentStore {
        let dir = tempdir().unwrap();
        let db = Arc::new(Database::create(dir.path().join("security-amendments.db")).unwrap());
        SecurityAmendmentStore::new(db).unwrap()
    }

    #[test]
    fn match_exact_prefix_and_regex() {
        let store = create_store();
        store
            .add_allow_rule(
                "bash",
                "cargo test",
                AmendmentMatchType::Exact,
                AmendmentScope::Workspace,
            )
            .unwrap();
        store
            .add_allow_rule(
                "bash",
                "cargo ",
                AmendmentMatchType::Prefix,
                AmendmentScope::Workspace,
            )
            .unwrap();
        store
            .add_allow_rule(
                "bash",
                "^git\\s+status$",
                AmendmentMatchType::Regex,
                AmendmentScope::Workspace,
            )
            .unwrap();

        assert!(
            store
                .find_matching_rule("bash", "cargo test", Some("agent-a"))
                .unwrap()
                .is_some()
        );
        assert!(
            store
                .find_matching_rule("bash", "cargo clippy --all-targets", Some("agent-a"))
                .unwrap()
                .is_some()
        );
        assert!(
            store
                .find_matching_rule("bash", "git status", Some("agent-a"))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn respect_agent_scope() {
        let store = create_store();
        store
            .add_allow_rule(
                "bash",
                "npm ",
                AmendmentMatchType::Prefix,
                AmendmentScope::Agent {
                    agent_id: "agent-a".to_string(),
                },
            )
            .unwrap();

        assert!(
            store
                .find_matching_rule("bash", "npm test", Some("agent-a"))
                .unwrap()
                .is_some()
        );
        assert!(
            store
                .find_matching_rule("bash", "npm test", Some("agent-b"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn reject_invalid_regex_pattern_on_create() {
        let store = create_store();
        let result = store.add_allow_rule(
            "bash",
            "([invalid",
            AmendmentMatchType::Regex,
            AmendmentScope::Workspace,
        );
        assert!(result.is_err());
    }

    #[test]
    fn reject_oversized_regex_pattern_on_create() {
        let store = create_store();
        let oversized = "a".repeat(MAX_REGEX_PATTERN_LEN + 1);
        let result = store.add_allow_rule(
            "bash",
            oversized,
            AmendmentMatchType::Regex,
            AmendmentScope::Workspace,
        );
        assert!(result.is_err());
    }
}
