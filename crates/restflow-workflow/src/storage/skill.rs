//! Skill storage implementation using redb.

use crate::models::Skill;
use anyhow::Result;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::sync::Arc;

const SKILLS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("skills");

#[derive(Debug, Clone)]
pub struct SkillStorage {
    db: Arc<Database>,
}

impl SkillStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        // Initialize the table
        let write_txn = db.begin_write()?;
        write_txn.open_table(SKILLS_TABLE)?;
        write_txn.commit()?;

        Ok(Self { db })
    }

    /// Create a new skill (fails if already exists)
    pub fn create(&self, skill: &Skill) -> Result<()> {
        if self.get(&skill.id)?.is_some() {
            return Err(anyhow::anyhow!("Skill {} already exists", skill.id));
        }

        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SKILLS_TABLE)?;
            let json = serde_json::to_string(skill)?;
            table.insert(skill.id.as_str(), json.as_bytes())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get a skill by ID
    pub fn get(&self, id: &str) -> Result<Option<Skill>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SKILLS_TABLE)?;

        if let Some(data) = table.get(id)? {
            let json = std::str::from_utf8(data.value())?;
            Ok(Some(serde_json::from_str(json)?))
        } else {
            Ok(None)
        }
    }

    /// List all skills
    pub fn list(&self) -> Result<Vec<Skill>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SKILLS_TABLE)?;

        let mut skills = Vec::new();
        for item in table.iter()? {
            let (_, value) = item?;
            let json = std::str::from_utf8(value.value())?;
            let skill: Skill = serde_json::from_str(json)?;
            skills.push(skill);
        }

        // Sort by updated_at descending (most recent first)
        skills.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        Ok(skills)
    }

    /// Update an existing skill
    pub fn update(&self, id: &str, skill: &Skill) -> Result<()> {
        if self.get(id)?.is_none() {
            return Err(anyhow::anyhow!("Skill {} not found", id));
        }

        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SKILLS_TABLE)?;
            let json = serde_json::to_string(skill)?;
            table.insert(id, json.as_bytes())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Delete a skill
    pub fn delete(&self, id: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SKILLS_TABLE)?;
            table.remove(id)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Check if a skill exists
    pub fn exists(&self, id: &str) -> Result<bool> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(SKILLS_TABLE)?;
        Ok(table.get(id)?.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn setup() -> (SkillStorage, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = SkillStorage::new(db).unwrap();
        (storage, temp_dir)
    }

    #[test]
    fn test_create_and_get() {
        let (storage, _temp_dir) = setup();

        let skill = Skill::new(
            "test-skill".to_string(),
            "Test Skill".to_string(),
            Some("A test skill".to_string()),
            Some(vec!["test".to_string()]),
            "# Test Content".to_string(),
        );

        storage.create(&skill).unwrap();

        let retrieved = storage.get("test-skill").unwrap().unwrap();
        assert_eq!(retrieved.id, "test-skill");
        assert_eq!(retrieved.name, "Test Skill");
        assert_eq!(retrieved.description, Some("A test skill".to_string()));
    }

    #[test]
    fn test_create_duplicate_fails() {
        let (storage, _temp_dir) = setup();

        let skill = Skill::new(
            "test-skill".to_string(),
            "Test Skill".to_string(),
            None,
            None,
            "# Test".to_string(),
        );

        storage.create(&skill).unwrap();
        let result = storage.create(&skill);
        assert!(result.is_err());
    }

    #[test]
    fn test_list() {
        let (storage, _temp_dir) = setup();

        let skill1 = Skill::new(
            "skill-1".to_string(),
            "Skill 1".to_string(),
            None,
            None,
            "# Skill 1".to_string(),
        );
        let skill2 = Skill::new(
            "skill-2".to_string(),
            "Skill 2".to_string(),
            None,
            None,
            "# Skill 2".to_string(),
        );

        storage.create(&skill1).unwrap();
        storage.create(&skill2).unwrap();

        let skills = storage.list().unwrap();
        assert_eq!(skills.len(), 2);
    }

    #[test]
    fn test_update() {
        let (storage, _temp_dir) = setup();

        let mut skill = Skill::new(
            "test-skill".to_string(),
            "Test Skill".to_string(),
            None,
            None,
            "# Test".to_string(),
        );

        storage.create(&skill).unwrap();

        skill.update(
            Some("Updated Name".to_string()),
            Some(Some("New description".to_string())),
            None,
            None,
        );

        storage.update("test-skill", &skill).unwrap();

        let retrieved = storage.get("test-skill").unwrap().unwrap();
        assert_eq!(retrieved.name, "Updated Name");
        assert_eq!(retrieved.description, Some("New description".to_string()));
    }

    #[test]
    fn test_delete() {
        let (storage, _temp_dir) = setup();

        let skill = Skill::new(
            "test-skill".to_string(),
            "Test Skill".to_string(),
            None,
            None,
            "# Test".to_string(),
        );

        storage.create(&skill).unwrap();
        assert!(storage.exists("test-skill").unwrap());

        storage.delete("test-skill").unwrap();
        assert!(!storage.exists("test-skill").unwrap());
    }

    #[test]
    fn test_update_nonexistent_fails() {
        let (storage, _temp_dir) = setup();

        let skill = Skill::new(
            "nonexistent".to_string(),
            "Test".to_string(),
            None,
            None,
            "# Test".to_string(),
        );

        let result = storage.update("nonexistent", &skill);
        assert!(result.is_err());
    }
}
