//! Process-local pairing and route binding storage.

use crate::simple_storage::namespace_for_db;
use anyhow::Result;
use redb::Database;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Default)]
struct PairingStore {
    allowed_peers: BTreeMap<String, Vec<u8>>,
    pairing_requests: BTreeMap<String, PairingRequestRecord>,
    pairing_peer_index: HashMap<String, String>,
    route_bindings: BTreeMap<String, RouteBindingRecord>,
    route_binding_target_index: HashMap<String, String>,
}

#[derive(Clone)]
struct PairingRequestRecord {
    peer_id: String,
    data: Vec<u8>,
}

#[derive(Clone)]
struct RouteBindingRecord {
    index_key: String,
    data: Vec<u8>,
}

fn stores() -> &'static Mutex<HashMap<usize, PairingStore>> {
    static STORES: OnceLock<Mutex<HashMap<usize, PairingStore>>> = OnceLock::new();
    STORES.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Clone)]
pub struct PairingStorage {
    namespace: usize,
}

impl PairingStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Ok(Self {
            namespace: namespace_for_db(&db),
        })
    }

    pub fn add_peer(&self, peer_id: &str, data: &[u8]) -> Result<()> {
        let mut stores = stores().lock().expect("pairing store lock poisoned");
        stores
            .entry(self.namespace)
            .or_default()
            .allowed_peers
            .insert(peer_id.to_string(), data.to_vec());
        Ok(())
    }

    pub fn remove_peer(&self, peer_id: &str) -> Result<bool> {
        let mut stores = stores().lock().expect("pairing store lock poisoned");
        Ok(stores
            .get_mut(&self.namespace)
            .is_some_and(|store| store.allowed_peers.remove(peer_id).is_some()))
    }

    pub fn is_peer_allowed(&self, peer_id: &str) -> Result<bool> {
        let stores = stores().lock().expect("pairing store lock poisoned");
        Ok(stores
            .get(&self.namespace)
            .is_some_and(|store| store.allowed_peers.contains_key(peer_id)))
    }

    pub fn get_peer(&self, peer_id: &str) -> Result<Option<Vec<u8>>> {
        let stores = stores().lock().expect("pairing store lock poisoned");
        Ok(stores
            .get(&self.namespace)
            .and_then(|store| store.allowed_peers.get(peer_id).cloned()))
    }

    pub fn list_peers(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let stores = stores().lock().expect("pairing store lock poisoned");
        Ok(stores
            .get(&self.namespace)
            .map(|store| {
                store
                    .allowed_peers
                    .iter()
                    .map(|(id, data)| (id.clone(), data.clone()))
                    .collect()
            })
            .unwrap_or_default())
    }

    pub fn create_pairing_request(&self, code: &str, peer_id: &str, data: &[u8]) -> Result<()> {
        let mut stores = stores().lock().expect("pairing store lock poisoned");
        let store = stores.entry(self.namespace).or_default();
        store.pairing_requests.insert(
            code.to_string(),
            PairingRequestRecord {
                peer_id: peer_id.to_string(),
                data: data.to_vec(),
            },
        );
        store
            .pairing_peer_index
            .insert(peer_id.to_string(), code.to_string());
        Ok(())
    }

    pub fn get_pairing_request(&self, code: &str) -> Result<Option<Vec<u8>>> {
        let stores = stores().lock().expect("pairing store lock poisoned");
        Ok(stores.get(&self.namespace).and_then(|store| {
            store
                .pairing_requests
                .get(code)
                .map(|record| record.data.clone())
        }))
    }

    pub fn get_pairing_request_by_peer(&self, peer_id: &str) -> Result<Option<String>> {
        let stores = stores().lock().expect("pairing store lock poisoned");
        Ok(stores
            .get(&self.namespace)
            .and_then(|store| store.pairing_peer_index.get(peer_id).cloned()))
    }

    pub fn delete_pairing_request(&self, code: &str) -> Result<()> {
        let mut stores = stores().lock().expect("pairing store lock poisoned");
        if let Some(store) = stores.get_mut(&self.namespace)
            && let Some(record) = store.pairing_requests.remove(code)
        {
            store.pairing_peer_index.remove(&record.peer_id);
        }
        Ok(())
    }

    pub fn list_pairing_requests(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let stores = stores().lock().expect("pairing store lock poisoned");
        Ok(stores
            .get(&self.namespace)
            .map(|store| {
                store
                    .pairing_requests
                    .iter()
                    .map(|(code, record)| (code.clone(), record.data.clone()))
                    .collect()
            })
            .unwrap_or_default())
    }

    pub fn cleanup_expired_requests(&self, now_ms: i64) -> Result<u32> {
        let mut stores = stores().lock().expect("pairing store lock poisoned");
        let Some(store) = stores.get_mut(&self.namespace) else {
            return Ok(0);
        };
        let expired = store
            .pairing_requests
            .iter()
            .filter_map(|(code, record)| {
                let expires_at = serde_json::from_slice::<serde_json::Value>(&record.data)
                    .ok()
                    .and_then(|value| value.get("expires_at_ms").and_then(|v| v.as_i64()));
                expires_at
                    .is_some_and(|value| value <= now_ms)
                    .then_some(code.clone())
            })
            .collect::<Vec<_>>();
        for code in &expired {
            if let Some(record) = store.pairing_requests.remove(code) {
                store.pairing_peer_index.remove(&record.peer_id);
            }
        }
        Ok(expired.len() as u32)
    }

    pub fn add_route_binding(&self, id: &str, index_key: &str, data: &[u8]) -> Result<()> {
        let mut stores = stores().lock().expect("pairing store lock poisoned");
        let store = stores.entry(self.namespace).or_default();
        if let Some(previous) = store.route_bindings.insert(
            id.to_string(),
            RouteBindingRecord {
                index_key: index_key.to_string(),
                data: data.to_vec(),
            },
        ) {
            store.route_binding_target_index.remove(&previous.index_key);
        }
        store
            .route_binding_target_index
            .insert(index_key.to_string(), id.to_string());
        Ok(())
    }

    pub fn remove_route_binding(&self, id: &str) -> Result<bool> {
        let mut stores = stores().lock().expect("pairing store lock poisoned");
        let Some(store) = stores.get_mut(&self.namespace) else {
            return Ok(false);
        };
        if let Some(record) = store.route_bindings.remove(id) {
            store.route_binding_target_index.remove(&record.index_key);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn get_route_binding(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let stores = stores().lock().expect("pairing store lock poisoned");
        Ok(stores.get(&self.namespace).and_then(|store| {
            store
                .route_bindings
                .get(id)
                .map(|record| record.data.clone())
        }))
    }

    pub fn resolve_route_by_key(&self, index_key: &str) -> Result<Option<Vec<u8>>> {
        let stores = stores().lock().expect("pairing store lock poisoned");
        let Some(store) = stores.get(&self.namespace) else {
            return Ok(None);
        };
        let Some(id) = store.route_binding_target_index.get(index_key) else {
            return Ok(None);
        };
        Ok(store
            .route_bindings
            .get(id)
            .map(|record| record.data.clone()))
    }

    pub fn list_route_bindings(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let stores = stores().lock().expect("pairing store lock poisoned");
        Ok(stores
            .get(&self.namespace)
            .map(|store| {
                store
                    .route_bindings
                    .iter()
                    .map(|(id, record)| (id.clone(), record.data.clone()))
                    .collect()
            })
            .unwrap_or_default())
    }
}
