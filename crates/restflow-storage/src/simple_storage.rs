use anyhow::Result;
use redb::{Database, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition};
use std::sync::Arc;

/// Trait for simple key-value storage modules.
///
/// Provides default implementations for common CRUD operations.
/// Implementors only need to specify the table definition and database reference.
pub trait SimpleStorage: Send + Sync {
    /// The table definition for this storage type.
    const TABLE: TableDefinition<'static, &'static str, &'static [u8]>;

    /// Get reference to the database.
    fn db(&self) -> &Arc<Database>;

    /// Insert only if key doesn't exist (atomic check-and-insert).
    ///
    /// Returns `Ok(true)` if the key was inserted (didn't exist before).
    /// Returns `Ok(false)` if the key already existed (no modification made).
    ///
    /// This operation is atomic - the existence check and insert happen
    /// in a single write transaction, preventing TOCTOU race conditions.
    fn insert_if_absent(&self, id: &str, data: &[u8]) -> Result<bool> {
        let write_txn = self.db().begin_write()?;
        let inserted = {
            let mut table = write_txn.open_table(Self::TABLE)?;
            let existed = table.get(id)?.is_some();
            if !existed {
                table.insert(id, data)?;
            }
            !existed
        };
        write_txn.commit()?;
        Ok(inserted)
    }

    /// Store raw bytes by ID.
    fn put_raw(&self, id: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db().begin_write()?;
        {
            let mut table = write_txn.open_table(Self::TABLE)?;
            table.insert(id, data)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get raw bytes by ID.
    fn get_raw(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db().begin_read()?;
        let table = read_txn.open_table(Self::TABLE)?;
        if let Some(value) = table.get(id)? {
            Ok(Some(value.value().to_vec()))
        } else {
            Ok(None)
        }
    }

    /// List all entries as (id, data) pairs.
    fn list_raw(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let read_txn = self.db().begin_read()?;
        let table = read_txn.open_table(Self::TABLE)?;
        let mut items = Vec::new();
        for item in table.iter()? {
            let (key, value) = item?;
            items.push((key.value().to_string(), value.value().to_vec()));
        }
        Ok(items)
    }

    /// Delete by ID, returns true if existed.
    fn delete(&self, id: &str) -> Result<bool> {
        let write_txn = self.db().begin_write()?;
        let existed = {
            let mut table = write_txn.open_table(Self::TABLE)?;
            table.remove(id)?.is_some()
        };
        write_txn.commit()?;
        Ok(existed)
    }

    /// Check if ID exists.
    fn exists(&self, id: &str) -> Result<bool> {
        let read_txn = self.db().begin_read()?;
        let table = read_txn.open_table(Self::TABLE)?;
        Ok(table.get(id)?.is_some())
    }

    /// Count all entries.
    fn count(&self) -> Result<usize> {
        let read_txn = self.db().begin_read()?;
        let table = read_txn.open_table(Self::TABLE)?;
        Ok(table.len()? as usize)
    }
}

/// Macro to generate a simple storage struct with common implementations.
#[macro_export]
macro_rules! define_simple_storage {
    ( $(#[$meta:meta])* $vis:vis struct $name:ident { table: $table_name:literal } ) => {
        $(#[$meta])*
        #[derive(Debug, Clone)]
        $vis struct $name {
            db: std::sync::Arc<redb::Database>,
        }

        impl $name {
            pub fn new(db: std::sync::Arc<redb::Database>) -> anyhow::Result<Self> {
                let write_txn = db.begin_write()?;
                write_txn.open_table(<Self as $crate::SimpleStorage>::TABLE)?;
                write_txn.commit()?;
                Ok(Self { db })
            }
        }

        impl $crate::SimpleStorage for $name {
            const TABLE: redb::TableDefinition<'static, &'static str, &'static [u8]> =
                redb::TableDefinition::new($table_name);

            fn db(&self) -> &std::sync::Arc<redb::Database> {
                &self.db
            }
        }
    };
}
