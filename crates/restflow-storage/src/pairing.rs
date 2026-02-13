//! Pairing and Route Binding storage - byte-level API for Telegram DM pairing
//! and multi-dimension agent routing persistence.

use anyhow::Result;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::sync::Arc;

/// Allowed peers table: peer_id -> JSON AllowedPeer
const ALLOWED_PEERS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("allowed_peers");
/// Pairing requests table: code -> JSON PairingRequest
const PAIRING_REQUESTS_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("pairing_requests");
/// Index: peer_id -> code (for lookup by peer)
const PAIRING_PEER_INDEX_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("pairing_peer_index");
/// Route bindings table: id -> JSON RouteBinding
const ROUTE_BINDINGS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("route_bindings");
/// Index: "{type}:{target_id}" -> id
const ROUTE_BINDING_TARGET_INDEX_TABLE: TableDefinition<&str, &str> =
    TableDefinition::new("route_binding_target_index");

/// Low-level pairing and route binding storage
#[derive(Clone)]
pub struct PairingStorage {
    db: Arc<Database>,
}

impl PairingStorage {
    /// Create a new PairingStorage, initializing all tables.
    pub fn new(db: Arc<Database>) -> Result<Self> {
        let write_txn = db.begin_write()?;
        write_txn.open_table(ALLOWED_PEERS_TABLE)?;
        write_txn.open_table(PAIRING_REQUESTS_TABLE)?;
        write_txn.open_table(PAIRING_PEER_INDEX_TABLE)?;
        write_txn.open_table(ROUTE_BINDINGS_TABLE)?;
        write_txn.open_table(ROUTE_BINDING_TARGET_INDEX_TABLE)?;
        write_txn.commit()?;
        Ok(Self { db })
    }

    // ============== Allowed Peer Operations ==============

    /// Add an allowed peer
    pub fn add_peer(&self, peer_id: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(ALLOWED_PEERS_TABLE)?;
            table.insert(peer_id, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Remove an allowed peer
    pub fn remove_peer(&self, peer_id: &str) -> Result<bool> {
        let write_txn = self.db.begin_write()?;
        let removed = {
            let mut table = write_txn.open_table(ALLOWED_PEERS_TABLE)?;
            table.remove(peer_id)?.is_some()
        };
        write_txn.commit()?;
        Ok(removed)
    }

    /// Check if a peer is allowed
    pub fn is_peer_allowed(&self, peer_id: &str) -> Result<bool> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(ALLOWED_PEERS_TABLE)?;
        Ok(table.get(peer_id)?.is_some())
    }

    /// Get a peer's data
    pub fn get_peer(&self, peer_id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(ALLOWED_PEERS_TABLE)?;
        Ok(table.get(peer_id)?.map(|v| v.value().to_vec()))
    }

    /// List all allowed peers
    pub fn list_peers(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(ALLOWED_PEERS_TABLE)?;
        let mut result = Vec::new();
        for entry in table.iter()? {
            let (key, value) = entry?;
            result.push((key.value().to_string(), value.value().to_vec()));
        }
        Ok(result)
    }

    // ============== Pairing Request Operations ==============

    /// Create a pairing request
    pub fn create_pairing_request(&self, code: &str, peer_id: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(PAIRING_REQUESTS_TABLE)?;
            table.insert(code, data)?;
            let mut index = write_txn.open_table(PAIRING_PEER_INDEX_TABLE)?;
            index.insert(peer_id, code)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get a pairing request by code
    pub fn get_pairing_request(&self, code: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(PAIRING_REQUESTS_TABLE)?;
        Ok(table.get(code)?.map(|v| v.value().to_vec()))
    }

    /// Get a pairing request code by peer_id
    pub fn get_pairing_request_by_peer(&self, peer_id: &str) -> Result<Option<String>> {
        let read_txn = self.db.begin_read()?;
        let index = read_txn.open_table(PAIRING_PEER_INDEX_TABLE)?;
        Ok(index.get(peer_id)?.map(|v| v.value().to_string()))
    }

    /// Delete a pairing request by code
    pub fn delete_pairing_request(&self, code: &str) -> Result<()> {
        // First read peer_id from request for index cleanup
        let peer_id = {
            let read_txn = self.db.begin_read()?;
            // Iterate the index to find the peer_id for this code
            let index = read_txn.open_table(PAIRING_PEER_INDEX_TABLE)?;
            let mut found_peer = None;
            for entry in index.iter()? {
                let (key, value) = entry?;
                if value.value() == code {
                    found_peer = Some(key.value().to_string());
                    break;
                }
            }
            found_peer
        };

        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(PAIRING_REQUESTS_TABLE)?;
            table.remove(code)?;
            if let Some(peer_id) = &peer_id {
                let mut index = write_txn.open_table(PAIRING_PEER_INDEX_TABLE)?;
                index.remove(peer_id.as_str())?;
            }
        }
        write_txn.commit()?;
        Ok(())
    }

    /// List all pairing requests
    pub fn list_pairing_requests(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(PAIRING_REQUESTS_TABLE)?;
        let mut result = Vec::new();
        for entry in table.iter()? {
            let (key, value) = entry?;
            result.push((key.value().to_string(), value.value().to_vec()));
        }
        Ok(result)
    }

    /// Cleanup expired pairing requests (older than given timestamp in ms)
    pub fn cleanup_expired_requests(&self, now_ms: i64) -> Result<u32> {
        // Collect expired codes first
        let expired: Vec<(String, Option<String>)> = {
            let read_txn = self.db.begin_read()?;
            let table = read_txn.open_table(PAIRING_REQUESTS_TABLE)?;
            let index = read_txn.open_table(PAIRING_PEER_INDEX_TABLE)?;
            let mut expired = Vec::new();

            for entry in table.iter()? {
                let (key, value) = entry?;
                let data: serde_json::Value = serde_json::from_slice(value.value())?;
                if let Some(expires_at) = data.get("expires_at").and_then(|v| v.as_i64())
                    && expires_at <= now_ms
                {
                    let code = key.value().to_string();
                    // Find peer_id from index
                    let mut found_peer = None;
                    for idx_entry in index.iter()? {
                        let (idx_key, idx_value) = idx_entry?;
                        if idx_value.value() == code.as_str() {
                            found_peer = Some(idx_key.value().to_string());
                            break;
                        }
                    }
                    expired.push((code, found_peer));
                }
            }
            expired
        };

        let count = expired.len() as u32;
        if count > 0 {
            let write_txn = self.db.begin_write()?;
            {
                let mut table = write_txn.open_table(PAIRING_REQUESTS_TABLE)?;
                let mut index = write_txn.open_table(PAIRING_PEER_INDEX_TABLE)?;
                for (code, peer_id) in &expired {
                    table.remove(code.as_str())?;
                    if let Some(pid) = peer_id {
                        index.remove(pid.as_str())?;
                    }
                }
            }
            write_txn.commit()?;
        }
        Ok(count)
    }

    // ============== Route Binding Operations ==============

    /// Add a route binding
    pub fn add_route_binding(&self, id: &str, index_key: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(ROUTE_BINDINGS_TABLE)?;
            table.insert(id, data)?;
            let mut index = write_txn.open_table(ROUTE_BINDING_TARGET_INDEX_TABLE)?;
            index.insert(index_key, id)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Remove a route binding
    pub fn remove_route_binding(&self, id: &str) -> Result<bool> {
        // First get the index key from the binding data
        let index_key = {
            let read_txn = self.db.begin_read()?;
            let table = read_txn.open_table(ROUTE_BINDINGS_TABLE)?;
            if let Some(data) = table.get(id)? {
                let binding: serde_json::Value = serde_json::from_slice(data.value())?;
                let binding_type = binding
                    .get("binding_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("default");
                let target_id = binding
                    .get("target_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("*");
                Some(format!("{}:{}", binding_type, target_id))
            } else {
                None
            }
        };

        let write_txn = self.db.begin_write()?;
        let removed = {
            let mut table = write_txn.open_table(ROUTE_BINDINGS_TABLE)?;
            let was_present = table.remove(id)?.is_some();
            if let Some(index_key) = &index_key {
                let mut index = write_txn.open_table(ROUTE_BINDING_TARGET_INDEX_TABLE)?;
                index.remove(index_key.as_str())?;
            }
            was_present
        };
        write_txn.commit()?;
        Ok(removed)
    }

    /// Get a route binding by id
    pub fn get_route_binding(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(ROUTE_BINDINGS_TABLE)?;
        Ok(table.get(id)?.map(|v| v.value().to_vec()))
    }

    /// Resolve route binding by target index key (e.g., "peer:12345")
    pub fn resolve_route_by_key(&self, index_key: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let index = read_txn.open_table(ROUTE_BINDING_TARGET_INDEX_TABLE)?;
        if let Some(id_guard) = index.get(index_key)? {
            let id = id_guard.value().to_string();
            drop(id_guard);
            let table = read_txn.open_table(ROUTE_BINDINGS_TABLE)?;
            Ok(table.get(id.as_str())?.map(|v| v.value().to_vec()))
        } else {
            Ok(None)
        }
    }

    /// List all route bindings
    pub fn list_route_bindings(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(ROUTE_BINDINGS_TABLE)?;
        let mut result = Vec::new();
        for entry in table.iter()? {
            let (key, value) = entry?;
            result.push((key.value().to_string(), value.value().to_vec()));
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::NamedTempFile;

    fn create_test_storage() -> PairingStorage {
        let tmp = NamedTempFile::new().unwrap();
        let db = Arc::new(Database::create(tmp.path()).unwrap());
        PairingStorage::new(db).unwrap()
    }

    #[test]
    fn test_add_and_check_peer() {
        let storage = create_test_storage();
        let peer_data = json!({
            "peer_id": "12345",
            "peer_name": "Alice",
            "approved_at": 1000,
            "approved_by": "cli"
        });
        let data = serde_json::to_vec(&peer_data).unwrap();

        storage.add_peer("12345", &data).unwrap();
        assert!(storage.is_peer_allowed("12345").unwrap());
        assert!(!storage.is_peer_allowed("99999").unwrap());

        let fetched = storage.get_peer("12345").unwrap().unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&fetched).unwrap();
        assert_eq!(parsed["peer_name"], "Alice");
    }

    #[test]
    fn test_create_and_get_pairing_request() {
        let storage = create_test_storage();
        let req_data = json!({
            "code": "A7k9Bm2X",
            "peer_id": "12345",
            "peer_name": "Alice",
            "chat_id": "chat-1",
            "created_at": 1000,
            "expires_at": 4600000
        });
        let data = serde_json::to_vec(&req_data).unwrap();

        storage
            .create_pairing_request("A7k9Bm2X", "12345", &data)
            .unwrap();

        let fetched = storage.get_pairing_request("A7k9Bm2X").unwrap().unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&fetched).unwrap();
        assert_eq!(parsed["peer_id"], "12345");

        // Lookup by peer
        let code = storage
            .get_pairing_request_by_peer("12345")
            .unwrap()
            .unwrap();
        assert_eq!(code, "A7k9Bm2X");
    }

    #[test]
    fn test_pairing_request_expiry() {
        let storage = create_test_storage();
        let now = 10000i64;

        // Create an expired request
        let expired = json!({
            "code": "EXP12345",
            "peer_id": "111",
            "expires_at": 5000
        });
        storage
            .create_pairing_request("EXP12345", "111", &serde_json::to_vec(&expired).unwrap())
            .unwrap();

        // Create a valid request
        let valid = json!({
            "code": "VAL67890",
            "peer_id": "222",
            "expires_at": 20000
        });
        storage
            .create_pairing_request("VAL67890", "222", &serde_json::to_vec(&valid).unwrap())
            .unwrap();

        let cleaned = storage.cleanup_expired_requests(now).unwrap();
        assert_eq!(cleaned, 1);

        // Expired one should be gone
        assert!(storage.get_pairing_request("EXP12345").unwrap().is_none());
        // Valid one should remain
        assert!(storage.get_pairing_request("VAL67890").unwrap().is_some());
    }

    #[test]
    fn test_add_and_resolve_route_binding() {
        let storage = create_test_storage();
        let binding = json!({
            "id": "rb-1",
            "binding_type": "peer",
            "target_id": "12345",
            "agent_id": "coding-agent",
            "created_at": 1000,
            "priority": 0
        });
        let data = serde_json::to_vec(&binding).unwrap();

        storage
            .add_route_binding("rb-1", "peer:12345", &data)
            .unwrap();

        let resolved = storage.resolve_route_by_key("peer:12345").unwrap().unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&resolved).unwrap();
        assert_eq!(parsed["agent_id"], "coding-agent");
    }

    #[test]
    fn test_route_binding_priority() {
        let storage = create_test_storage();

        // Add peer binding
        let peer_binding = json!({
            "id": "rb-peer",
            "binding_type": "peer",
            "target_id": "12345",
            "agent_id": "peer-agent",
            "priority": 0
        });
        storage
            .add_route_binding(
                "rb-peer",
                "peer:12345",
                &serde_json::to_vec(&peer_binding).unwrap(),
            )
            .unwrap();

        // Add group binding
        let group_binding = json!({
            "id": "rb-group",
            "binding_type": "group",
            "target_id": "group-1",
            "agent_id": "group-agent",
            "priority": 1
        });
        storage
            .add_route_binding(
                "rb-group",
                "group:group-1",
                &serde_json::to_vec(&group_binding).unwrap(),
            )
            .unwrap();

        // Add default binding
        let default_binding = json!({
            "id": "rb-default",
            "binding_type": "default",
            "target_id": "*",
            "agent_id": "default-agent",
            "priority": 2
        });
        storage
            .add_route_binding(
                "rb-default",
                "default:*",
                &serde_json::to_vec(&default_binding).unwrap(),
            )
            .unwrap();

        // Peer binding should resolve
        assert!(
            storage
                .resolve_route_by_key("peer:12345")
                .unwrap()
                .is_some()
        );
        // Group binding should resolve
        assert!(
            storage
                .resolve_route_by_key("group:group-1")
                .unwrap()
                .is_some()
        );
        // Default should resolve
        assert!(storage.resolve_route_by_key("default:*").unwrap().is_some());
        // Unknown should not resolve
        assert!(
            storage
                .resolve_route_by_key("peer:99999")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn test_remove_peer() {
        let storage = create_test_storage();
        let peer_data = json!({ "peer_id": "12345", "approved_at": 1000, "approved_by": "cli" });
        storage
            .add_peer("12345", &serde_json::to_vec(&peer_data).unwrap())
            .unwrap();

        assert!(storage.is_peer_allowed("12345").unwrap());
        assert!(storage.remove_peer("12345").unwrap());
        assert!(!storage.is_peer_allowed("12345").unwrap());
        assert!(!storage.remove_peer("12345").unwrap());
    }

    #[test]
    fn test_list_peers() {
        let storage = create_test_storage();
        for id in &["111", "222", "333"] {
            let data = json!({ "peer_id": id });
            storage
                .add_peer(id, &serde_json::to_vec(&data).unwrap())
                .unwrap();
        }
        let peers = storage.list_peers().unwrap();
        assert_eq!(peers.len(), 3);
    }

    #[test]
    fn test_remove_route_binding() {
        let storage = create_test_storage();
        let binding = json!({
            "id": "rb-1",
            "binding_type": "peer",
            "target_id": "12345",
            "agent_id": "agent-1",
            "priority": 0
        });
        storage
            .add_route_binding("rb-1", "peer:12345", &serde_json::to_vec(&binding).unwrap())
            .unwrap();

        assert!(storage.remove_route_binding("rb-1").unwrap());
        assert!(
            storage
                .resolve_route_by_key("peer:12345")
                .unwrap()
                .is_none()
        );
        assert!(!storage.remove_route_binding("rb-1").unwrap());
    }
}
