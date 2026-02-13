use std::sync::Arc;

use anyhow::Result;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use regex::Regex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const SECURITY_AMENDMENTS_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("security_amendments");

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
            AmendmentMatchType::Regex => Regex::new(&self.command_pattern)
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
        let rule = SecurityAmendment {
            id: format!("amendment-{}", Uuid::new_v4()),
            tool_name: tool_name.into(),
            command_pattern: command_pattern.into(),
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
}
