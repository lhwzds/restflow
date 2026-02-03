//! Secrets storage - encrypted storage for API keys and credentials.

use crate::encryption::SecretEncryptor;
use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rand::RngCore;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::info;
use ts_rs::TS;

const SECRETS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("secrets");
const MASTER_KEY_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("secret_master_key");
const MASTER_KEY_ENV: &str = "RESTFLOW_MASTER_KEY";
const MASTER_KEY_STATE_DIR_ENV: &str = "RESTFLOW_STATE_DIR";
const MASTER_KEY_RECORD: &str = "default";
const MASTER_KEY_FILE_NAME: &str = "secret-master-key.json";
const MASTER_KEY_FILE_VERSION: u32 = 1;


#[derive(Debug, Clone)]
pub enum MasterKeySource {
    /// Key from environment variable (recommended for production)
    Environment,
    /// Key from OS keychain (recommended for development)
    Keychain,
    /// Key from database (insecure - only for testing)
    Database { warn: bool },
}

#[derive(Debug, Clone)]
pub struct SecretStorageConfig {
    pub key_source: MasterKeySource,
    pub allow_insecure_fallback: bool,
}

impl Default for SecretStorageConfig {
    fn default() -> Self {
        Self {
            key_source: MasterKeySource::Environment,
            allow_insecure_fallback: false,
        }
    }
}

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
        Self::with_config(db, SecretStorageConfig::default())
    }

    pub fn with_config(db: Arc<Database>, config: SecretStorageConfig) -> Result<Self> {
        let write_txn = db.begin_write()?;
        write_txn.open_table(SECRETS_TABLE)?;
        write_txn.commit()?;

        let master_key = load_master_key(&db, &config)?;
        let encryptor = Arc::new(SecretEncryptor::new(&master_key)?);
        let storage = Self { db, encryptor };

        storage.migrate_legacy_secrets()?;

        Ok(storage)
    }

    /// Create with insecure database fallback (for testing only)
    #[cfg(test)]
    pub fn new_insecure(db: Arc<Database>) -> Result<Self> {
        Self::with_config(
            db,
            SecretStorageConfig {
                key_source: MasterKeySource::Database { warn: false },
                allow_insecure_fallback: true,
            },
        )
    }

    /// Set or update a secret
    pub fn set_secret(&self, key: &str, value: &str, description: Option<String>) -> Result<()> {
        self.write_secret_with_constraint(key, value, description, WriteConstraint::None)
    }

    /// Create a new secret (fails if already exists)
    ///
    /// This operation is atomic - the existence check and insert happen
    /// within the same write transaction to prevent race conditions.
    pub fn create_secret(&self, key: &str, value: &str, description: Option<String>) -> Result<()> {
        self.write_secret_with_constraint(key, value, description, WriteConstraint::MustBeNew)
    }

    /// Update an existing secret (fails if not exists)
    ///
    /// This operation is atomic - the existence check and update happen
    /// within the same write transaction to prevent race conditions.
    pub fn update_secret(&self, key: &str, value: &str, description: Option<String>) -> Result<()> {
        self.write_secret_with_constraint(key, value, description, WriteConstraint::MustExist)
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
                Err(_) => Err(anyhow::anyhow!("Failed to decrypt secret payload: {}", err)),
            },
        }
    }

    fn write_secret_with_constraint(
        &self,
        key: &str,
        value: &str,
        description: Option<String>,
        constraint: WriteConstraint,
    ) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SECRETS_TABLE)?;
            let existing = table
                .get(key)?
                .map(|data| self.decode_secret_bytes(data.value()))
                .transpose()?;

            match constraint {
                WriteConstraint::MustExist => {
                    if existing.is_none() {
                        return Err(anyhow::anyhow!("Secret {} not found", key));
                    }
                }
                WriteConstraint::MustBeNew => {
                    if existing.is_some() {
                        return Err(anyhow::anyhow!("Secret {} already exists", key));
                    }
                }
                WriteConstraint::None => {}
            }

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

fn load_master_key(_db: &Arc<Database>, _config: &SecretStorageConfig) -> Result<[u8; 32]> {
    if let Some(key) = load_master_key_from_env()? {
        info!("Using master key from environment variable");
        return Ok(key);
    }

    if let Some(key) = read_master_key_from_json()? {
        return Ok(key);
    }

    let mut key = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut key);
    match write_master_key_to_json(&key)? {
        MasterKeyWriteResult::Written => Ok(key),
        MasterKeyWriteResult::AlreadyExists => read_master_key_from_json()?
            .ok_or_else(|| anyhow::anyhow!("Master key file exists but could not be read")),
    }
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
        for (i, chunk) in trimmed.as_bytes().chunks(2).enumerate() {
            let hex = std::str::from_utf8(chunk).context("Invalid hex master key")?;
            let byte = u8::from_str_radix(hex, 16).context("Invalid hex master key")?;
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
    let table = match read_txn.open_table(MASTER_KEY_TABLE) {
        Ok(table) => table,
        Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
        Err(err) => return Err(err.into()),
    };

    if let Some(data) = table.get(MASTER_KEY_RECORD)? {
        let payload = data.value();
        if payload.len() != 32 {
            return Err(anyhow::anyhow!("Stored master key must be 32 bytes"));
        }
        let key: [u8; 32] = payload
            .try_into()
            .map_err(|_| anyhow::anyhow!("Stored master key must be 32 bytes"))?;
        Ok(Some(key))
    } else {
        Ok(None)
    }
}

fn delete_master_key_from_db(db: &Arc<Database>) -> Result<()> {
    let write_txn = db.begin_write()?;
    let mut table = match write_txn.open_table(MASTER_KEY_TABLE) {
        Ok(table) => table,
        Err(redb::TableError::TableDoesNotExist(_)) => return Ok(()),
        Err(err) => return Err(err.into()),
    };
    table.remove(MASTER_KEY_RECORD)?;
    drop(table);
    write_txn.commit()?;
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct MasterKeyFile {
    version: u32,
    #[serde(rename = "createdAt")]
    created_at: String,
    key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MasterKeyWriteResult {
    Written,
    AlreadyExists,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MasterKeyMigrationStatus {
    Migrated,
    JsonAlreadyExists,
    NoDatabaseKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterKeyMigrationResult {
    pub status: MasterKeyMigrationStatus,
    pub path: PathBuf,
}

fn state_dir() -> Result<PathBuf> {
    if let Ok(path) = env::var(MASTER_KEY_STATE_DIR_ENV) {
        return Ok(PathBuf::from(path));
    }

    let base = dirs::data_dir()
        .or_else(dirs::home_dir)
        .ok_or_else(|| anyhow::anyhow!("Failed to determine state directory"))?;
    Ok(base.join("restflow"))
}

fn master_key_path() -> Result<PathBuf> {
    Ok(state_dir()?.join(MASTER_KEY_FILE_NAME))
}

fn read_master_key_from_json() -> Result<Option<[u8; 32]>> {
    let path = master_key_path()?;
    if !path.exists() {
        return Ok(None);
    }

    ensure_master_key_permissions(&path)?;

    let bytes = fs::read(&path)?;
    let file: MasterKeyFile = serde_json::from_slice(&bytes)?;
    if file.version != MASTER_KEY_FILE_VERSION {
        return Err(anyhow::anyhow!(
            "Unsupported master key file version: {}",
            file.version
        ));
    }

    let key = decode_master_key(&file.key)?;
    Ok(Some(key))
}

fn write_master_key_to_json(key: &[u8; 32]) -> Result<MasterKeyWriteResult> {
    let dir = state_dir()?;
    fs::create_dir_all(&dir)?;
    let path = dir.join(MASTER_KEY_FILE_NAME);
    let payload = MasterKeyFile {
        version: MASTER_KEY_FILE_VERSION,
        created_at: chrono::Utc::now().to_rfc3339(),
        key: STANDARD.encode(key),
    };
    let data = serde_json::to_vec_pretty(&payload)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = match OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                return Ok(MasterKeyWriteResult::AlreadyExists)
            }
            Err(err) => return Err(err.into()),
        };
        file.write_all(&data)?;
    }

    #[cfg(not(unix))]
    {
        let mut file = match OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
        {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                return Ok(MasterKeyWriteResult::AlreadyExists)
            }
            Err(err) => return Err(err.into()),
        };
        file.write_all(&data)?;
    }

    Ok(MasterKeyWriteResult::Written)
}

fn ensure_master_key_permissions(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = fs::metadata(path)?;
        let mode = metadata.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            return Err(anyhow::anyhow!(
                "Master key file permissions are too open: {:#o}",
                mode
            ));
        }
    }

    Ok(())
}

pub fn migrate_master_key_from_db(db: &Arc<Database>) -> Result<MasterKeyMigrationResult> {
    let path = master_key_path()?;
    if path.exists() {
        return Ok(MasterKeyMigrationResult {
            status: MasterKeyMigrationStatus::JsonAlreadyExists,
            path,
        });
    }

    let key = match read_master_key_from_db(db)? {
        Some(key) => key,
        None => {
            return Ok(MasterKeyMigrationResult {
                status: MasterKeyMigrationStatus::NoDatabaseKey,
                path,
            })
        }
    };

    match write_master_key_to_json(&key)? {
        MasterKeyWriteResult::Written => {
            delete_master_key_from_db(db)?;
        }
        MasterKeyWriteResult::AlreadyExists => {
            return Ok(MasterKeyMigrationResult {
                status: MasterKeyMigrationStatus::JsonAlreadyExists,
                path,
            })
        }
    }

    Ok(MasterKeyMigrationResult {
        status: MasterKeyMigrationStatus::Migrated,
        path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env as std_env;
    use std::sync::{Mutex, OnceLock};
    use std::thread;
    use tempfile::tempdir;

    fn setup() -> (SecretStorage, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = SecretStorage::new_insecure(db).unwrap();
        (storage, temp_dir)
    }

    fn state_dir_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn with_state_dir(dir: &Path, f: impl FnOnce()) {
        let _guard = state_dir_lock().lock().unwrap();
        std_env::set_var(MASTER_KEY_STATE_DIR_ENV, dir);
        f();
        std_env::remove_var(MASTER_KEY_STATE_DIR_ENV);
    }

    fn setup_db_for_master_key(temp_dir: &tempfile::TempDir) -> Arc<Database> {
        let db_path = temp_dir.path().join("test-master-key.db");
        Arc::new(Database::create(db_path).unwrap())
    }

    fn insert_db_master_key(db: &Arc<Database>, key: &[u8; 32]) {
        let write_txn = db.begin_write().unwrap();
        {
            let mut table = write_txn.open_table(MASTER_KEY_TABLE).unwrap();
            table.insert(MASTER_KEY_RECORD, key.as_slice()).unwrap();
        }
        write_txn.commit().unwrap();
    }

    #[test]
    fn test_env_key_takes_precedence() {
        let temp_dir = tempdir().unwrap();
        let state_dir = temp_dir.path().join("state");
        let db = setup_db_for_master_key(&temp_dir);
        let config = SecretStorageConfig::default();

        let env_key = [0xaa; 32];
        let env_value = STANDARD.encode(env_key);
        let json_key = [9u8; 32];

        with_state_dir(&state_dir, || {
            write_master_key_to_json(&json_key).unwrap();
            std_env::set_var(MASTER_KEY_ENV, env_value);
            let key = load_master_key(&db, &config).unwrap();
            std_env::remove_var(MASTER_KEY_ENV);
            assert_eq!(key, env_key);
        });
    }

    #[test]
    fn test_generates_key_when_missing() {
        let temp_dir = tempdir().unwrap();
        let state_dir = temp_dir.path().join("state");
        let db = setup_db_for_master_key(&temp_dir);
        let config = SecretStorageConfig::default();

        with_state_dir(&state_dir, || {
            std_env::remove_var(MASTER_KEY_ENV);
            let key = load_master_key(&db, &config).unwrap();
            let persisted = read_master_key_from_json().unwrap().unwrap();
            assert_eq!(key, persisted);
        });
    }

    #[test]
    fn test_master_key_json_non_overwrite() {
        let temp_dir = tempdir().unwrap();
        let state_dir = temp_dir.path().join("state");
        let db = setup_db_for_master_key(&temp_dir);
        let config = SecretStorageConfig::default();

        let key_a = [1u8; 32];
        let key_b = [2u8; 32];

        with_state_dir(&state_dir, || {
            let result = write_master_key_to_json(&key_a).unwrap();
            assert_eq!(result, MasterKeyWriteResult::Written);

            let result = write_master_key_to_json(&key_b).unwrap();
            assert_eq!(result, MasterKeyWriteResult::AlreadyExists);

            let key = load_master_key(&db, &config).unwrap();
            assert_eq!(key, key_a);
        });
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

    // Duplicate concurrent set secret test removed.

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
            table
                .insert(secret.key.as_str(), encoded.as_bytes())
                .unwrap();
        }
        write_txn.commit().unwrap();

        let storage = SecretStorage::new_insecure(db).unwrap();
        let value = storage.get_secret("LEGACY_KEY").unwrap();
        assert_eq!(value, Some("legacy-value".to_string()));

        let secrets = storage.list_secrets().unwrap();
        assert_eq!(secrets.len(), 1);
    }

    #[test]
    fn test_create_secret_atomic() {
        let (storage, _temp_dir) = setup();

        // First create should succeed
        storage
            .create_secret("UNIQUE_KEY", "value1", None)
            .unwrap();

        // Second create should fail
        let result = storage.create_secret("UNIQUE_KEY", "value2", None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));

        // Value should remain the first one
        let value = storage.get_secret("UNIQUE_KEY").unwrap();
        assert_eq!(value, Some("value1".to_string()));
    }

    #[test]
    fn test_update_secret_atomic() {
        let (storage, _temp_dir) = setup();

        // Update non-existent should fail
        let result = storage.update_secret("NON_EXISTENT", "value", None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));

        // Create then update should work
        storage.create_secret("UPDATE_KEY", "initial", None).unwrap();
        storage
            .update_secret("UPDATE_KEY", "updated", Some("desc".to_string()))
            .unwrap();

        let value = storage.get_secret("UPDATE_KEY").unwrap();
        assert_eq!(value, Some("updated".to_string()));
    }

    /// Test concurrent set_secret operations don't corrupt data.
    /// All threads write to the same key - the final value should be one of the written values.
    #[test]
    fn test_concurrent_set_secret() {
        use std::thread;

        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = Arc::new(SecretStorage::new_insecure(db).unwrap());

        let num_threads = 10;
        let handles: Vec<_> = (0..num_threads)
            .map(|i| {
                let s = Arc::clone(&storage);
                thread::spawn(move || {
                    s.set_secret("concurrent_key", &format!("value-{}", i), None)
                        .unwrap();
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // Should have exactly one secret, not corrupted
        let secret = storage.get_secret("concurrent_key").unwrap();
        assert!(secret.is_some());
        let value = secret.unwrap();
        assert!(value.starts_with("value-"));

        // Only one secret should exist
        let secrets = storage.list_secrets().unwrap();
        assert_eq!(secrets.len(), 1);
    }

    /// Test concurrent create_secret - only one should succeed.
    #[test]
    fn test_concurrent_create_secret() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::thread;

        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = Arc::new(SecretStorage::new_insecure(db).unwrap());

        let success_count = Arc::new(AtomicUsize::new(0));
        let num_threads = 10;

        let handles: Vec<_> = (0..num_threads)
            .map(|i| {
                let s = Arc::clone(&storage);
                let count = Arc::clone(&success_count);
                thread::spawn(move || {
                    if s.create_secret("race_key", &format!("value-{}", i), None)
                        .is_ok()
                    {
                        count.fetch_add(1, Ordering::SeqCst);
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // Exactly one create should have succeeded
        assert_eq!(success_count.load(Ordering::SeqCst), 1);

        // Only one secret should exist
        let secrets = storage.list_secrets().unwrap();
        assert_eq!(secrets.len(), 1);
    }

    #[test]
    fn test_master_key_env_override() {
        let temp_dir = tempdir().unwrap();
        let state_dir = temp_dir.path().join("state");
        let db = setup_db_for_master_key(&temp_dir);
        let config = SecretStorageConfig::default();

        let env_key = [7u8; 32];
        let env_value = STANDARD.encode(env_key);
        let json_key = [9u8; 32];

        with_state_dir(&state_dir, || {
            write_master_key_to_json(&json_key).unwrap();
            std_env::set_var(MASTER_KEY_ENV, env_value);
            let key = load_master_key(&db, &config).unwrap();
            std_env::remove_var(MASTER_KEY_ENV);
            assert_eq!(key, env_key);
        });
    }

    #[test]
    fn test_master_key_json_precedence_over_db() {
        let temp_dir = tempdir().unwrap();
        let state_dir = temp_dir.path().join("state");
        let db = setup_db_for_master_key(&temp_dir);
        let config = SecretStorageConfig::default();

        let json_key = [3u8; 32];
        let db_key = [5u8; 32];
        insert_db_master_key(&db, &db_key);

        with_state_dir(&state_dir, || {
            write_master_key_to_json(&json_key).unwrap();
            let key = load_master_key(&db, &config).unwrap();
            assert_eq!(key, json_key);
            let stored = read_master_key_from_db(&db).unwrap().unwrap();
            assert_eq!(stored, db_key);
        });
    }

    #[test]
    fn test_master_key_migration_from_db() {
        let temp_dir = tempdir().unwrap();
        let state_dir = temp_dir.path().join("state");
        let db = setup_db_for_master_key(&temp_dir);

        let db_key = [11u8; 32];
        insert_db_master_key(&db, &db_key);

        with_state_dir(&state_dir, || {
            let result = migrate_master_key_from_db(&db).unwrap();
            assert_eq!(result.status, MasterKeyMigrationStatus::Migrated);
            assert_eq!(result.path, master_key_path().unwrap());
            let stored = read_master_key_from_db(&db).unwrap();
            assert!(stored.is_none());
            let json_key = read_master_key_from_json().unwrap().unwrap();
            assert_eq!(json_key, db_key);
        });
    }
}


enum WriteConstraint {
    None,
    MustExist,
    MustBeNew,
}
