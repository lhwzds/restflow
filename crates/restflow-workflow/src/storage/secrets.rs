use crate::models::Secret;
use anyhow::Result;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::sync::Arc;

const SECRETS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("secrets");

#[derive(Debug, Clone)]
pub struct SecretStorage {
    db: Arc<Database>,
}

impl SecretStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        let write_txn = db.begin_write()?;
        write_txn.open_table(SECRETS_TABLE)?;
        write_txn.commit()?;

        Ok(Self { db })
    }

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

            let json = serde_json::to_string(&secret)?;
            let encoded = STANDARD.encode(json.as_bytes());
            table.insert(key, encoded.as_bytes())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    // Create a new secret (fails if already exists)
    pub fn create_secret(&self, key: &str, value: &str, description: Option<String>) -> Result<()> {
        if self.get_secret_model(key)?.is_some() {
            return Err(anyhow::anyhow!("Secret {} already exists", key));
        }
        self.set_secret(key, value, description)
    }

    // Update an existing secret (fails if not exists)
    pub fn update_secret(&self, key: &str, value: &str, description: Option<String>) -> Result<()> {
        if self.get_secret_model(key)?.is_none() {
            return Err(anyhow::anyhow!("Secret {} not found", key));
        }
        self.set_secret(key, value, description)
    }

    // Internal use only
    fn get_secret_model(&self, key: &str) -> Result<Option<Secret>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SECRETS_TABLE)?;

        if let Some(data) = table.get(key)? {
            let encoded = std::str::from_utf8(data.value())?;
            let decoded = STANDARD.decode(encoded)?;
            let json = String::from_utf8(decoded)?;
            Ok(Some(serde_json::from_str(&json)?))
        } else {
            Ok(None)
        }
    }

    pub fn get_secret(&self, key: &str) -> Result<Option<String>> {
        if let Some(secret) = self.get_secret_model(key)? {
            Ok(Some(secret.value))
        } else {
            // Fallback to environment variable (e.g., OPENAI_API_KEY)
            Ok(std::env::var(key.to_uppercase().replace('-', "_")).ok())
        }
    }

    pub fn delete_secret(&self, key: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SECRETS_TABLE)?;
            table.remove(key)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    // Returns all secrets without values for security
    pub fn list_secrets(&self) -> Result<Vec<Secret>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SECRETS_TABLE)?;

        let mut secrets = Vec::new();
        for item in table.iter()? {
            let (_, value) = item?;
            let encoded = std::str::from_utf8(value.value())?;
            let decoded = STANDARD.decode(encoded)?;
            let json = String::from_utf8(decoded)?;
            let mut secret: Secret = serde_json::from_str(&json)?;
            // Clear the value for security
            secret.value = String::new();
            secrets.push(secret);
        }

        Ok(secrets)
    }

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
        storage
            .set_secret("API_KEY_3", "value3", Some("Third key".to_string()))
            .unwrap();

        let secrets = storage.list_secrets().unwrap();
        assert_eq!(secrets.len(), 3);

        let key1 = secrets.iter().find(|s| s.key == "API_KEY_1").unwrap();
        assert_eq!(key1.description, Some("First key".to_string()));
        assert_eq!(key1.value, ""); // Value should be cleared

        let key2 = secrets.iter().find(|s| s.key == "API_KEY_2").unwrap();
        assert_eq!(key2.description, None);
    }

    #[test]
    fn test_update_preserves_created_at() {
        let (storage, _temp_dir) = setup();

        storage
            .set_secret("KEY", "initial", Some("Test key".to_string()))
            .unwrap();

        let secrets = storage.list_secrets().unwrap();
        let initial = secrets.iter().find(|s| s.key == "KEY").unwrap();
        let created_at = initial.created_at;
        let initial_updated_at = initial.updated_at;

        // Wait to ensure time difference
        std::thread::sleep(std::time::Duration::from_millis(10));

        storage
            .set_secret("KEY", "updated", Some("Updated description".to_string()))
            .unwrap();

        let secrets = storage.list_secrets().unwrap();
        let updated = secrets.iter().find(|s| s.key == "KEY").unwrap();

        println!(
            "created_at: {}, initial_updated_at: {}, new_updated_at: {}",
            created_at, initial_updated_at, updated.updated_at
        );

        assert_eq!(updated.created_at, created_at); // created_at preserved
        assert!(
            updated.updated_at > initial_updated_at,
            "updated_at should be greater: {} > {}",
            updated.updated_at,
            initial_updated_at
        );
        assert_eq!(updated.description, Some("Updated description".to_string()));
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
}
