//! Secrets storage - encrypted storage for API keys and credentials.

use crate::encryption::SecretEncryptor;
use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::RngCore;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::env;
use std::sync::Arc;
use ts_rs::TS;

const SECRETS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("secrets");
const MASTER_KEY_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("secret_master_key");
const MASTER_KEY_ENV: &str = "RESTFLOW_MASTER_KEY";
const MASTER_KEY_RECORD: &str = "default";

/// A stored secret with metadata
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Secret {
    pub key: String,
    pub value: String,
    pub description: Option<String>,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
}

impl Secret {
    /// Create a new secret
    pub fn new(key: String, value: String, description: Option<String>) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            key,
            value,
            description,
            created_at: now,
            updated_at: now,
        }
    }

    /// Update the secret value and description
    ///
    /// Pass `None` for description to clear it, or `Some(...)` to set a new one.
    pub fn update(&mut self, value: String, description: Option<String>) {
        self.value = value;
        self.description = description; // Always set, allowing None to clear
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }
}

/// Secret storage with AES-256-GCM encryption
#[derive(Clone)]
pub struct SecretStorage {
    db: Arc<Database>,
    encryptor: Arc<SecretEncryptor>,
}

impl std::fmt::Debug for SecretStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretStorage")
            .field("db", &"<redb::Database>")
            .field("encryptor", &"<SecretEncryptor>")
            .finish()
    }
}

impl SecretStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        let write_txn = db.begin_write()?;
        write_txn.open_table(SECRETS_TABLE)?;
        write_txn.open_table(MASTER_KEY_TABLE)?;
        write_txn.commit()?;

        let master_key = load_master_key(&db)?;
        let encryptor = Arc::new(SecretEncryptor::new(&master_key)?);
        let storage = Self { db, encryptor };

        storage.migrate_legacy_secrets()?;

        Ok(storage)
    }

    /// Set or update a secret
    pub fn set_secret(&self, key: &str, value: &str, description: Option<String>) -> Result<()> {
        let existing = self.get_secret_model(key)?;

        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SECRETS_TABLE)?;

            let secret = if let Some(mut existing_secret) = existing {
                existing_secret.update(value.to_string(), description);
                existing_secret
            } else {
                Secret::new(key.to_string(), value.to_string(), description)
            };

            let encrypted = self.encode_secret(&secret)?;
            table.insert(key, encrypted.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Create a new secret (fails if already exists)
    pub fn create_secret(&self, key: &str, value: &str, description: Option<String>) -> Result<()> {
        if self.get_secret_model(key)?.is_some() {
            return Err(anyhow::anyhow!("Secret {} already exists", key));
        }
        self.set_secret(key, value, description)
    }

    /// Update an existing secret (fails if not exists)
    pub fn update_secret(&self, key: &str, value: &str, description: Option<String>) -> Result<()> {
        if self.get_secret_model(key)?.is_none() {
            return Err(anyhow::anyhow!("Secret {} not found", key));
        }
        self.set_secret(key, value, description)
    }

    /// Get secret model (internal)
    fn get_secret_model(&self, key: &str) -> Result<Option<Secret>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SECRETS_TABLE)?;

        if let Some(data) = table.get(key)? {
            let raw = data.value();
            let secret = self.decode_secret_bytes(raw)?;
            Ok(Some(secret))
        } else {
            Ok(None)
        }
    }

    /// Get secret value, falling back to environment variable
    pub fn get_secret(&self, key: &str) -> Result<Option<String>> {
        if let Some(secret) = self.get_secret_model(key)? {
            Ok(Some(secret.value))
        } else {
            // Fallback to environment variable (e.g., OPENAI_API_KEY)
            Ok(std::env::var(key.to_uppercase().replace('-', "_")).ok())
        }
    }

    /// Delete a secret
    pub fn delete_secret(&self, key: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SECRETS_TABLE)?;
            table.remove(key)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// List all secrets (values are cleared for security)
    pub fn list_secrets(&self) -> Result<Vec<Secret>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SECRETS_TABLE)?;

        let mut secrets = Vec::new();
        for item in table.iter()? {
            let (_, value) = item?;
            let secret = self.decode_secret_bytes(value.value())?;
            let mut secret = secret;
            // Clear the value for security
            secret.value = String::new();
            secrets.push(secret);
        }

        Ok(secrets)
    }

    /// Check if a secret exists
    pub fn has_secret(&self, key: &str) -> Result<bool> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SECRETS_TABLE)?;
        Ok(table.get(key)?.is_some())
    }

    fn encode_secret(&self, secret: &Secret) -> Result<Vec<u8>> {
        let json = serde_json::to_vec(secret)?;
        self.encryptor.encrypt(&json)
    }

    fn decode_secret_bytes(&self, payload: &[u8]) -> Result<Secret> {
        match self.encryptor.decrypt(payload) {
            Ok(plaintext) => Ok(serde_json::from_slice(&plaintext)?),
            Err(err) => match decode_legacy_secret(payload) {
                Ok(secret) => Ok(secret),
                Err(_) => Err(anyhow::anyhow!(
                    "Failed to decrypt secret payload: {}",
                    err
                )),
            },
        }
    }

    fn migrate_legacy_secrets(&self) -> Result<usize> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SECRETS_TABLE)?;

        let mut legacy_entries = Vec::new();
        for item in table.iter()? {
            let (key, value) = item?;
            let key = key.value().to_string();
            let payload = value.value();

            if self.encryptor.decrypt(payload).is_ok() {
                continue;
            }

            let secret = decode_legacy_secret(payload)
                .with_context(|| format!("Failed to decode legacy secret {}", key))?;
            legacy_entries.push((key, secret));
        }

        drop(table);
        drop(read_txn);

        if legacy_entries.is_empty() {
            return Ok(0);
        }

        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SECRETS_TABLE)?;
            for (key, secret) in legacy_entries.iter() {
                let encrypted = self.encode_secret(secret)?;
                table.insert(key.as_str(), encrypted.as_slice())?;
            }
        }
        write_txn.commit()?;

        Ok(legacy_entries.len())
    }
}

fn decode_legacy_secret(payload: &[u8]) -> Result<Secret> {
    let encoded = std::str::from_utf8(payload)?;
    let decoded = STANDARD.decode(encoded)?;
    let json = String::from_utf8(decoded)?;
    Ok(serde_json::from_str(&json)?)
}

fn load_master_key(db: &Arc<Database>) -> Result<[u8; 32]> {
    if let Some(key) = load_master_key_from_env()? {
        return Ok(key);
    }

    if let Some(key) = read_master_key_from_db(db)? {
        return Ok(key);
    }

    let mut key = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut key);
    store_master_key_in_db(db, &key)?;
    Ok(key)
}

fn load_master_key_from_env() -> Result<Option<[u8; 32]>> {
    match env::var(MASTER_KEY_ENV) {
        Ok(value) => decode_master_key(&value).map(Some),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(err) => Err(anyhow::anyhow!(
            "Failed to read {}: {}",
            MASTER_KEY_ENV,
            err
        )),
    }
}

fn decode_master_key(value: &str) -> Result<[u8; 32]> {
    let trimmed = value.trim();
    if trimmed.len() == 64 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        let mut key = [0u8; 32];
        for i in 0..32 {
            let start = i * 2;
            let byte = u8::from_str_radix(&trimmed[start..start + 2], 16)
                .context("Invalid hex master key")?;
            key[i] = byte;
        }
        return Ok(key);
    }

    let decoded = STANDARD
        .decode(trimmed.as_bytes())
        .context("Invalid base64 master key")?;
    if decoded.len() != 32 {
        return Err(anyhow::anyhow!(
            "Master key must be 32 bytes after decoding"
        ));
    }

    let mut key = [0u8; 32];
    key.copy_from_slice(&decoded);
    Ok(key)
}

fn read_master_key_from_db(db: &Arc<Database>) -> Result<Option<[u8; 32]>> {
    let read_txn = db.begin_read()?;
    let table = read_txn.open_table(MASTER_KEY_TABLE)?;

    if let Some(data) = table.get(MASTER_KEY_RECORD)? {
        let payload = data.value();
        if payload.len() != 32 {
            return Err(anyhow::anyhow!(
                "Stored master key must be 32 bytes"
            ));
        }
        let key: [u8; 32] = payload
            .try_into()
            .map_err(|_| anyhow::anyhow!("Stored master key must be 32 bytes"))?;
        Ok(Some(key))
    } else {
        Ok(None)
    }
}

fn store_master_key_in_db(db: &Arc<Database>, key: &[u8; 32]) -> Result<()> {
    let write_txn = db.begin_write()?;
    {
        let mut table = write_txn.open_table(MASTER_KEY_TABLE)?;
        table.insert(MASTER_KEY_RECORD, key.as_slice())?;
    }
    write_txn.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn setup() -> (SecretStorage, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = SecretStorage::new(db).unwrap();
        (storage, temp_dir)
    }

    #[test]
    fn test_set_and_get_secret() {
        let (storage, _temp_dir) = setup();

        storage
            .set_secret(
                "OPENAI_API_KEY",
                "sk-test123",
                Some("OpenAI API key".to_string()),
            )
            .unwrap();

        let value = storage.get_secret("OPENAI_API_KEY").unwrap();
        assert_eq!(value, Some("sk-test123".to_string()));
    }

    #[test]
    fn test_list_secrets_with_metadata() {
        let (storage, _temp_dir) = setup();

        storage
            .set_secret("API_KEY_1", "value1", Some("First key".to_string()))
            .unwrap();
        storage.set_secret("API_KEY_2", "value2", None).unwrap();

        let secrets = storage.list_secrets().unwrap();
        assert_eq!(secrets.len(), 2);

        let key1 = secrets.iter().find(|s| s.key == "API_KEY_1").unwrap();
        assert_eq!(key1.description, Some("First key".to_string()));
        assert_eq!(key1.value, ""); // Value should be cleared
    }

    #[test]
    fn test_delete_secret() {
        let (storage, _temp_dir) = setup();

        storage.set_secret("TEST_KEY", "test_value", None).unwrap();
        storage.delete_secret("TEST_KEY").unwrap();

        let value = storage.get_secret("TEST_KEY").unwrap();
        assert_eq!(value, None);
    }

    #[test]
    fn test_has_secret() {
        let (storage, _temp_dir) = setup();

        storage.set_secret("EXISTS", "value", None).unwrap();

        assert!(storage.has_secret("EXISTS").unwrap());
        assert!(!storage.has_secret("NOT_EXISTS").unwrap());
    }

    #[test]
    fn test_clear_description() {
        let (storage, _temp_dir) = setup();

        // Create secret with description
        storage
            .set_secret(
                "TEST_KEY",
                "value1",
                Some("Initial description".to_string()),
            )
            .unwrap();

        // Verify description is set
        let secrets = storage.list_secrets().unwrap();
        let secret = secrets.iter().find(|s| s.key == "TEST_KEY").unwrap();
        assert_eq!(secret.description, Some("Initial description".to_string()));

        // Update with None to clear description
        storage.set_secret("TEST_KEY", "value2", None).unwrap();

        // Verify description is cleared
        let secrets = storage.list_secrets().unwrap();
        let secret = secrets.iter().find(|s| s.key == "TEST_KEY").unwrap();
        assert_eq!(
            secret.description, None,
            "Description should be cleared when None is passed"
        );
    }

    #[test]
    fn test_migrate_legacy_secret() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());

        let secret = Secret::new(
            "LEGACY_KEY".to_string(),
            "legacy-value".to_string(),
            Some("Legacy secret".to_string()),
        );
        let json = serde_json::to_string(&secret).unwrap();
        let encoded = STANDARD.encode(json.as_bytes());

        let write_txn = db.begin_write().unwrap();
        {
            let mut table = write_txn.open_table(SECRETS_TABLE).unwrap();
            table.insert(secret.key.as_str(), encoded.as_bytes()).unwrap();
        }
        write_txn.commit().unwrap();

        let storage = SecretStorage::new(db).unwrap();
        let value = storage.get_secret("LEGACY_KEY").unwrap();
        assert_eq!(value, Some("legacy-value".to_string()));

        let secrets = storage.list_secrets().unwrap();
        assert_eq!(secrets.len(), 1);
    }
}
