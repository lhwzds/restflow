use anyhow::Result;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::sync::Arc;

const SECRETS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("secrets");

pub struct SecretStorage {
    db: Arc<Database>,
}

impl SecretStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        // Create table if not exists
        let write_txn = db.begin_write()?;
        write_txn.open_table(SECRETS_TABLE)?;
        write_txn.commit()?;

        Ok(Self { db })
    }

    /// Set a secret value (base64 encoded for obfuscation)
    pub fn set_secret(&self, key: &str, value: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SECRETS_TABLE)?;
            // Simple base64 encoding - not encryption, just obfuscation
            let encoded = STANDARD.encode(value.as_bytes());
            table.insert(key, encoded.as_bytes())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get a secret value (decoded from base64)
    pub fn get_secret(&self, key: &str) -> Result<Option<String>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SECRETS_TABLE)?;

        if let Some(data) = table.get(key)? {
            let encoded = std::str::from_utf8(data.value())?;
            let decoded = STANDARD.decode(encoded)?;
            Ok(Some(String::from_utf8(decoded)?))
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

    /// List all secret keys (not values)
    pub fn list_keys(&self) -> Result<Vec<String>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SECRETS_TABLE)?;

        let mut keys = Vec::new();
        for item in table.iter()? {
            let (key, _) = item?;
            keys.push(key.value().to_string());
        }

        Ok(keys)
    }

    /// Check if a secret exists
    pub fn has_secret(&self, key: &str) -> Result<bool> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SECRETS_TABLE)?;
        Ok(table.get(key)?.is_some())
    }
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

        // Set secret
        storage.set_secret("OPENAI_API_KEY", "sk-test123").unwrap();

        // Get secret
        let value = storage.get_secret("OPENAI_API_KEY").unwrap();
        assert_eq!(value, Some("sk-test123".to_string()));
    }

    #[test]
    fn test_delete_secret() {
        let (storage, _temp_dir) = setup();

        // Set and delete
        storage.set_secret("TEST_KEY", "test_value").unwrap();
        storage.delete_secret("TEST_KEY").unwrap();

        // Should not exist
        let value = storage.get_secret("TEST_KEY").unwrap();
        assert_eq!(value, None);
    }

    #[test]
    fn test_list_keys() {
        let (storage, _temp_dir) = setup();

        // Set multiple secrets
        storage.set_secret("API_KEY_1", "value1").unwrap();
        storage.set_secret("API_KEY_2", "value2").unwrap();
        storage.set_secret("API_KEY_3", "value3").unwrap();

        // List keys
        let keys = storage.list_keys().unwrap();
        assert_eq!(keys.len(), 3);
        assert!(keys.contains(&"API_KEY_1".to_string()));
        assert!(keys.contains(&"API_KEY_2".to_string()));
        assert!(keys.contains(&"API_KEY_3".to_string()));
    }

    #[test]
    fn test_has_secret() {
        let (storage, _temp_dir) = setup();

        storage.set_secret("EXISTS", "value").unwrap();

        assert!(storage.has_secret("EXISTS").unwrap());
        assert!(!storage.has_secret("NOT_EXISTS").unwrap());
    }

    #[test]
    fn test_update_secret() {
        let (storage, _temp_dir) = setup();

        // Set initial value
        storage.set_secret("KEY", "initial").unwrap();
        assert_eq!(
            storage.get_secret("KEY").unwrap(),
            Some("initial".to_string())
        );

        // Update value
        storage.set_secret("KEY", "updated").unwrap();
        assert_eq!(
            storage.get_secret("KEY").unwrap(),
            Some("updated".to_string())
        );
    }
}
