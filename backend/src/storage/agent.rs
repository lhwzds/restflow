use crate::node::agent::AgentNode;
use anyhow::Result;
use base64::read;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::vec;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StoredAgent {
    pub id: String,
    pub name: String,
    pub agent: AgentNode,
}
const AGENT_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("agents");

pub struct AgentStorage {
    db: Arc<Database>,
}

impl AgentStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        // Create table
        let write_txn = db.begin_write()?;
        write_txn.open_table(AGENT_TABLE)?;
        write_txn.commit()?;

        Ok(Self { db })
    }
    pub fn insert_agent(&self, name: String, agent: AgentNode) -> Result<StoredAgent> {
        let stored_agent = StoredAgent {
            id: Uuid::new_v4().to_string(),
            name,
            agent,
        };
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(AGENT_TABLE)?;
            let json_bytes = serde_json::to_vec(&stored_agent)?;
            table.insert(stored_agent.id.as_str(), json_bytes.as_slice())?;
        }
        write_txn.commit()?;

        Ok(stored_agent)
    }

    pub fn get_agent(&self, id: String) -> Result<Option<StoredAgent>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(AGENT_TABLE)?;
        if let Some(value) = table.get(id.as_str())? {
            let agent: StoredAgent = serde_json::from_slice(value.value())?;
            Ok(Some(agent))
        } else {
            Ok(None)
        }
    }

    pub fn list_agents(&self) -> Result<Vec<StoredAgent>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(AGENT_TABLE)?;
        let mut agents = Vec::new();
        for item in table.iter()? {
            let (_, value) = item?;
            let agent: StoredAgent = serde_json::from_slice(value.value())?;
            agents.push(agent);
        }
        Ok(agents)
    }
}
