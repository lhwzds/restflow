//! Secrets storage - encrypted storage for API keys and credentials.

use crate::encryption::SecretEncryptor;
use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rand::RngCore;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition, TableError};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, warn};
use ts_rs::TS;

const SECRETS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("secrets");
const MASTER_KEY_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("secret_master_key");
const MASTER_KEY_ENV: &str = "RESTFLOW_MASTER_KEY";
const MASTER_KEY_RECORD: &str = "default";
const STATE_DIR_ENV: &str = "RESTFLOW_STATE_DIR";
const MASTER_KEY_JSON_FILE: &str = "secret-master-key.json";

#[derive(Debug, Clone)]
pub struct SecretStorageConfig {
    pub allow_insecure_file_permissions: bool,
}

impl Default for SecretStorageConfig {
    fn default() -> Self {
        Self {
            allow_insecure_file_permissions: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MasterKeyFile {
    pub version: u32,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    pub key: String,
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
        write_txn.open_table(MASTER_KEY_TABLE)?;
        write_txn.commit()?;

        let master_key = load_master_key(&db, &config)?;
        let encryptor = Arc::new(SecretEncryptor::new(&master_key)?);
        let storage = Self { db, encryptor };

        storage.migrate_legacy_secrets()?;

        Ok(storage)
    }

    /// Create for testing with relaxed file permission checks.
    #[cfg(test)]
    pub fn new_insecure(db: Arc<Database>) -> Result<Self> {
        Self::with_config(
            db,
            SecretStorageConfig {
                allow_insecure_file_permissions: true,
            },
        )
    }

    /// Migrate the master key from the database to the JSON file.
    pub fn migrate_master_key_from_db(&self) -> Result<PathBuf> {
        migrate_master_key_from_db_inner(&self.db)
    }

    /// Set or update a secret
    pub fn set_secret(&self, key: &str, value: &str, description: Option<String>) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SECRETS_TABLE)?;

            let existing = table
                .get(key)?
                .map(|data| self.decode_secret_bytes(data.value()))
                .transpose()?;

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
    ///
    /// This operation is atomic - the existence check and insert happen
    /// within the same write transaction to prevent race conditions.
    pub fn create_secret(&self, key: &str, value: &str, description: Option<String>) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SECRETS_TABLE)?;

            // Check existence within write transaction to prevent TOCTOU race
            if table.get(key)?.is_some() {
                return Err(anyhow::anyhow!("Secret {} already exists", key));
            }

            let secret = Secret::new(key.to_string(), value.to_string(), description);
            let encrypted = self.encode_secret(&secret)?;
            table.insert(key, encrypted.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Update an existing secret (fails if not exists)
    ///
    /// This operation is atomic - the existence check and update happen
    /// within the same write transaction to prevent race conditions.
    pub fn update_secret(&self, key: &str, value: &str, description: Option<String>) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SECRETS_TABLE)?;

            // Check existence and get current data within write transaction
            let existing = table
                .get(key)?
                .map(|data| self.decode_secret_bytes(data.value()))
                .transpose()?;

            let mut existing_secret =
                existing.ok_or_else(|| anyhow::anyhow!("Secret {} not found", key))?;

            existing_secret.update(value.to_string(), description);
            let encrypted = self.encode_secret(&existing_secret)?;
            table.insert(key, encrypted.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
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

/// Migrate the master key from the database file without initializing SecretStorage.
pub fn migrate_master_key_from_db_path(db_path: impl AsRef<Path>) -> Result<PathBuf> {
    let db = Arc::new(Database::create(db_path.as_ref())?);
    ensure_master_key_table(&db)?;
    migrate_master_key_from_db_inner(&db)
}

fn migrate_master_key_from_db_inner(db: &Arc<Database>) -> Result<PathBuf> {
    let path = master_key_json_path()?;
    if path.exists() {
        anyhow::bail!(
            "Master key JSON already exists at {}",
            path.to_string_lossy()
        );
    }

    let key = read_master_key_from_db(db)?
        .ok_or_else(|| anyhow::anyhow!("No master key found in database to migrate"))?;

    write_master_key_json(&key)?;
    remove_master_key_from_db(db)?;

    Ok(path)
}

fn ensure_master_key_table(db: &Arc<Database>) -> Result<()> {
    let write_txn = db.begin_write()?;
    write_txn.open_table(MASTER_KEY_TABLE)?;
    write_txn.commit()?;
    Ok(())
}

fn decode_legacy_secret(payload: &[u8]) -> Result<Secret> {
    let encoded = std::str::from_utf8(payload)?;
    let decoded = STANDARD.decode(encoded)?;
    let json = String::from_utf8(decoded)?;
    Ok(serde_json::from_str(&json)?)
}

fn load_master_key(db: &Arc<Database>, config: &SecretStorageConfig) -> Result<[u8; 32]> {
    if let Some(key) = load_master_key_from_env()? {
        info!("Using master key from environment variable");
        return Ok(key);
    }

    if let Some(key) = load_master_key_from_json(config)? {
        info!("Using master key from JSON file");
        return Ok(key);
    }

    if read_master_key_from_db(db)?.is_some() {
        anyhow::bail!(
            "Master key is stored in the database. Run `restflow secret migrate-master-key` before upgrading."
        );
    }

    let mut key = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut key);
    match write_master_key_json(&key) {
        Ok(_) => Ok(key),
        Err(err) => {
            if let Some(io_err) = err.downcast_ref::<std::io::Error>() {
                if io_err.kind() == std::io::ErrorKind::AlreadyExists {
                    if let Some(existing) = load_master_key_from_json(config)? {
                        return Ok(existing);
                    }
                }
            }
            Err(err)
        }
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

fn load_master_key_from_json(config: &SecretStorageConfig) -> Result<Option<[u8; 32]>> {
    let path = master_key_json_path()?;
    if !path.exists() {
        return Ok(None);
    }

    check_master_key_permissions(&path, config.allow_insecure_file_permissions)?;
    let raw = fs::read_to_string(&path)?;
    let payload: MasterKeyFile = serde_json::from_str(&raw)?;
    if payload.version != 1 {
        anyhow::bail!("Unsupported master key JSON version: {}", payload.version);
    }

    let key = decode_master_key(&payload.key)?;
    Ok(Some(key))
}

fn write_master_key_json(key: &[u8; 32]) -> Result<PathBuf> {
    let dir = ensure_state_dir()?;
    let path = dir.join(MASTER_KEY_JSON_FILE);

    let payload = MasterKeyFile {
        version: 1,
        created_at: chrono::Utc::now().to_rfc3339(),
        key: encode_master_key_hex(key),
    };

    let json = serde_json::to_string_pretty(&payload)?;

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let mut file = options.open(&path)?;
    file.write_all(json.as_bytes())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }

    Ok(path)
}

fn master_key_json_path() -> Result<PathBuf> {
    Ok(resolve_state_dir()?.join(MASTER_KEY_JSON_FILE))
}

fn resolve_state_dir() -> Result<PathBuf> {
    if let Ok(value) = env::var(STATE_DIR_ENV) {
        if !value.trim().is_empty() {
            return Ok(PathBuf::from(value));
        }
    }

    let home = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Failed to determine home directory for state storage"))?;

    Ok(home.join(".restflow"))
}

fn ensure_state_dir() -> Result<PathBuf> {
    let dir = resolve_state_dir()?;
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn check_master_key_permissions(path: &Path, allow_insecure: bool) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = fs::metadata(path)?;
        let mode = metadata.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            if allow_insecure {
                warn!(
                    "Master key file permissions are too open (0o{:o}) at {}",
                    mode,
                    path.to_string_lossy()
                );
            } else {
                anyhow::bail!(
                    "Master key file permissions are too open (0o{:o}) at {}",
                    mode,
                    path.to_string_lossy()
                );
            }
        }
    }

    Ok(())
}

fn encode_master_key_hex(key: &[u8; 32]) -> String {
    key.iter().map(|byte| format!("{:02x}", byte)).collect()
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
        Err(TableError::TableDoesNotExist(_)) => return Ok(None),
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

fn remove_master_key_from_db(db: &Arc<Database>) -> Result<()> {
    let write_txn = db.begin_write()?;
    {
        let mut table = write_txn.open_table(MASTER_KEY_TABLE)?;
        table.remove(MASTER_KEY_RECORD)?;
    }
    write_txn.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    fn setup() -> (SecretStorage, tempfile::TempDir) {
        let _env_lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let state_dir = temp_dir.path().join("state");
        std::fs::create_dir_all(&state_dir).unwrap();

        // SAFETY: Tests are single-threaded in this module.
        unsafe { std::env::set_var(STATE_DIR_ENV, &state_dir) };

        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = SecretStorage::new_insecure(db).unwrap();

        // SAFETY: This is a single-threaded test, no other threads access this env var
        unsafe { std::env::remove_var(STATE_DIR_ENV) };

        (storage, temp_dir)
    }

    #[test]
    fn test_env_key_takes_precedence() {
        let _env_lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let state_dir = temp_dir.path().join("state");
        std::fs::create_dir_all(&state_dir).unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());

        let write_txn = db.begin_write().unwrap();
        write_txn.open_table(MASTER_KEY_TABLE).unwrap();
        write_txn.commit().unwrap();

        let db_key = [1u8; 32];
        let write_txn = db.begin_write().unwrap();
        {
            let mut table = write_txn.open_table(MASTER_KEY_TABLE).unwrap();
            table.insert(MASTER_KEY_RECORD, db_key.as_slice()).unwrap();
        }
        write_txn.commit().unwrap();

        let env_value = "aa".repeat(32);
        // SAFETY: This is a single-threaded test, no other threads access this env var
        unsafe { std::env::set_var(MASTER_KEY_ENV, &env_value) };
        // SAFETY: This is a single-threaded test, no other threads access this env var
        unsafe { std::env::set_var(STATE_DIR_ENV, &state_dir) };

        let config = SecretStorageConfig {
            allow_insecure_file_permissions: true,
        };

        let key = load_master_key(&db, &config).unwrap();
        assert_eq!(key, [0xaa; 32]);

        // SAFETY: This is a single-threaded test, no other threads access this env var
        unsafe { std::env::remove_var(MASTER_KEY_ENV) };
        // SAFETY: This is a single-threaded test, no other threads access this env var
        unsafe { std::env::remove_var(STATE_DIR_ENV) };
    }

    #[test]
    fn test_json_key_takes_precedence() {
        let _env_lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let state_dir = temp_dir.path().join("state");
        std::fs::create_dir_all(&state_dir).unwrap();

        // SAFETY: This is a single-threaded test, no other threads access this env var
        unsafe { std::env::set_var(STATE_DIR_ENV, &state_dir) };

        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());

        let json_key = [0x11u8; 32];
        write_master_key_json(&json_key).unwrap();

        let config = SecretStorageConfig {
            allow_insecure_file_permissions: true,
        };

        let key = load_master_key(&db, &config).unwrap();
        assert_eq!(key, json_key);

        // SAFETY: This is a single-threaded test, no other threads access this env var
        unsafe { std::env::remove_var(STATE_DIR_ENV) };
    }

    #[test]
    fn test_write_master_key_json_is_atomic() {
        let _env_lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let state_dir = temp_dir.path().join("state");
        std::fs::create_dir_all(&state_dir).unwrap();

        // SAFETY: This is a single-threaded test, no other threads access this env var
        unsafe { std::env::set_var(STATE_DIR_ENV, &state_dir) };

        let first_key = [0x22u8; 32];
        write_master_key_json(&first_key).unwrap();

        let second_key = [0x33u8; 32];
        let err = write_master_key_json(&second_key).unwrap_err();
        let io_err = err.downcast_ref::<std::io::Error>().unwrap();
        assert_eq!(io_err.kind(), std::io::ErrorKind::AlreadyExists);

        let config = SecretStorageConfig {
            allow_insecure_file_permissions: true,
        };
        let existing = load_master_key_from_json(&config).unwrap().unwrap();
        assert_eq!(existing, first_key);

        // SAFETY: This is a single-threaded test, no other threads access this env var
        unsafe { std::env::remove_var(STATE_DIR_ENV) };
    }

    #[test]
    fn test_rejects_db_key_without_migration() {
        let _env_lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let state_dir = temp_dir.path().join("state");
        std::fs::create_dir_all(&state_dir).unwrap();

        // SAFETY: This is a single-threaded test, no other threads access this env var
        unsafe { std::env::set_var(STATE_DIR_ENV, &state_dir) };

        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());

        let write_txn = db.begin_write().unwrap();
        write_txn.open_table(MASTER_KEY_TABLE).unwrap();
        write_txn.commit().unwrap();

        let db_key = [1u8; 32];
        let write_txn = db.begin_write().unwrap();
        {
            let mut table = write_txn.open_table(MASTER_KEY_TABLE).unwrap();
            table.insert(MASTER_KEY_RECORD, db_key.as_slice()).unwrap();
        }
        write_txn.commit().unwrap();

        let config = SecretStorageConfig {
            allow_insecure_file_permissions: true,
        };

        let err = load_master_key(&db, &config).unwrap_err();
        assert!(err.to_string().contains("migrate-master-key"));

        // SAFETY: This is a single-threaded test, no other threads access this env var
        unsafe { std::env::remove_var(STATE_DIR_ENV) };
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
        let _env_lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let state_dir = temp_dir.path().join("state");
        std::fs::create_dir_all(&state_dir).unwrap();

        // SAFETY: This is a single-threaded test, no other threads access this env var
        unsafe { std::env::set_var(STATE_DIR_ENV, &state_dir) };

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

        // SAFETY: This is a single-threaded test, no other threads access this env var
        unsafe { std::env::remove_var(STATE_DIR_ENV) };
    }

    #[test]
    fn test_create_secret_atomic() {
        let (storage, _temp_dir) = setup();

        // First create should succeed
        storage.create_secret("UNIQUE_KEY", "value1", None).unwrap();

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
        storage
            .create_secret("UPDATE_KEY", "initial", None)
            .unwrap();
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

        let _env_lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let state_dir = temp_dir.path().join("state");
        std::fs::create_dir_all(&state_dir).unwrap();

        // SAFETY: This is a single-threaded test, no other threads access this env var
        unsafe { std::env::set_var(STATE_DIR_ENV, &state_dir) };

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

        // SAFETY: This is a single-threaded test, no other threads access this env var
        unsafe { std::env::remove_var(STATE_DIR_ENV) };
    }

    /// Test concurrent create_secret - only one should succeed.
    #[test]
    fn test_concurrent_create_secret() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::thread;

        let _env_lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let state_dir = temp_dir.path().join("state");
        std::fs::create_dir_all(&state_dir).unwrap();

        // SAFETY: This is a single-threaded test, no other threads access this env var
        unsafe { std::env::set_var(STATE_DIR_ENV, &state_dir) };

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

        // SAFETY: This is a single-threaded test, no other threads access this env var
        unsafe { std::env::remove_var(STATE_DIR_ENV) };
    }

    #[test]
    fn test_migrate_master_key_from_db() {
        let _env_lock = env_lock();
        let temp_dir = tempdir().unwrap();
        let state_dir = temp_dir.path().join("state");
        std::fs::create_dir_all(&state_dir).unwrap();

        // SAFETY: This is a single-threaded test, no other threads access this env var
        unsafe { std::env::set_var(STATE_DIR_ENV, &state_dir) };

        let db_path = temp_dir.path().join("test.db");
        let db_key = [9u8; 32];
        {
            let db = Arc::new(Database::create(&db_path).unwrap());

            let write_txn = db.begin_write().unwrap();
            write_txn.open_table(MASTER_KEY_TABLE).unwrap();
            write_txn.commit().unwrap();

            let write_txn = db.begin_write().unwrap();
            {
                let mut table = write_txn.open_table(MASTER_KEY_TABLE).unwrap();
                table.insert(MASTER_KEY_RECORD, db_key.as_slice()).unwrap();
            }
            write_txn.commit().unwrap();
        }

        let path = migrate_master_key_from_db_path(&db_path).unwrap();
        assert!(path.exists());

        let loaded = load_master_key_from_json(&SecretStorageConfig {
            allow_insecure_file_permissions: true,
        })
        .unwrap()
        .unwrap();
        assert_eq!(loaded, db_key);

        let db = Arc::new(Database::create(&db_path).unwrap());
        let remaining = read_master_key_from_db(&db).unwrap();
        assert!(remaining.is_none());

        // SAFETY: This is a single-threaded test, no other threads access this env var
        unsafe { std::env::remove_var(STATE_DIR_ENV) };
    }
}
