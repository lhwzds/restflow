use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::env;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ApiKey {
    pub id: String,
    pub name: String,
    pub key_hash: String,
    pub scopes: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub last_used: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ApiKeyIssued {
    pub id: String,
    pub name: String,
    pub key: String,
    pub key_hash: String,
    pub scopes: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Default)]
pub struct ApiKeyManager {
    keys_by_hash: RwLock<HashMap<String, ApiKey>>,
    hash_by_id: RwLock<HashMap<String, String>>,
}

impl ApiKeyManager {
    pub fn new() -> Self {
        Self {
            keys_by_hash: RwLock::new(HashMap::new()),
            hash_by_id: RwLock::new(HashMap::new()),
        }
    }

    pub fn from_env() -> Arc<Self> {
        let manager = Arc::new(Self::new());
        if let Ok(raw) = env::var("RESTFLOW_API_KEYS") {
            for value in raw.split(',').map(str::trim).filter(|v| !v.is_empty()) {
                manager.insert_existing_key(value, "env", Vec::new());
            }
        }
        manager
    }

    #[allow(dead_code)]
    pub fn create_key(&self, name: &str, scopes: Vec<String>) -> ApiKeyIssued {
        let id = Uuid::new_v4().to_string();
        let key = format!("rfk_{}", Uuid::new_v4().simple());
        let key_hash = hash_key(&key);
        let created_at = Utc::now();

        let record = ApiKey {
            id: id.clone(),
            name: name.to_string(),
            key_hash: key_hash.clone(),
            scopes: scopes.clone(),
            created_at,
            last_used: None,
        };

        self.keys_by_hash
            .write()
            .expect("api key lock")
            .insert(key_hash.clone(), record);
        self.hash_by_id
            .write()
            .expect("api key lock")
            .insert(id.clone(), key_hash.clone());

        ApiKeyIssued {
            id,
            name: name.to_string(),
            key,
            key_hash,
            scopes,
            created_at,
        }
    }

    pub fn validate_key(&self, key: &str) -> Option<ApiKey> {
        let key_hash = hash_key(key);
        let mut keys = self.keys_by_hash.write().expect("api key lock");
        let record = keys.get_mut(&key_hash)?;
        record.last_used = Some(Utc::now());
        Some(record.clone())
    }

    #[allow(dead_code)]
    pub fn revoke_key(&self, key_id: &str) -> bool {
        let hash = {
            let mut ids = self.hash_by_id.write().expect("api key lock");
            ids.remove(key_id)
        };
        if let Some(hash) = hash {
            self.keys_by_hash
                .write()
                .expect("api key lock")
                .remove(&hash);
            return true;
        }
        false
    }

    #[allow(dead_code)]
    pub fn list_keys(&self) -> Vec<ApiKey> {
        self.keys_by_hash
            .read()
            .expect("api key lock")
            .values()
            .cloned()
            .collect()
    }

    fn insert_existing_key(&self, key: &str, name: &str, scopes: Vec<String>) {
        let id = Uuid::new_v4().to_string();
        let key_hash = hash_key(key);
        let record = ApiKey {
            id: id.clone(),
            name: name.to_string(),
            key_hash: key_hash.clone(),
            scopes,
            created_at: Utc::now(),
            last_used: None,
        };
        self.keys_by_hash
            .write()
            .expect("api key lock")
            .insert(key_hash.clone(), record);
        self.hash_by_id
            .write()
            .expect("api key lock")
            .insert(id, key_hash);
    }
}

fn hash_key(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    hex::encode(digest)
}
