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
        const TABLE: redb::TableDefinition<'static, &'static str, &'static [u8]> =
            redb::TableDefinition::new($table_name);

        $(#[$meta])*
        #[derive(Debug, Clone)]
        $vis struct $name {
            db: std::sync::Arc<redb::Database>,
        }

        impl $name {
            pub fn new(db: std::sync::Arc<redb::Database>) -> anyhow::Result<Self> {
                let write_txn = db.begin_write()?;
                write_txn.open_table(TABLE)?;
                write_txn.commit()?;

                Ok(Self { db })
            }

            pub fn put_raw(&self, id: &str, data: &[u8]) -> anyhow::Result<()> {
                <Self as $crate::SimpleStorage>::put_raw(self, id, data)
            }

            pub fn get_raw(&self, id: &str) -> anyhow::Result<Option<Vec<u8>>> {
                <Self as $crate::SimpleStorage>::get_raw(self, id)
            }

            pub fn list_raw(&self) -> anyhow::Result<Vec<(String, Vec<u8>)>> {
                <Self as $crate::SimpleStorage>::list_raw(self)
            }

            pub fn delete(&self, id: &str) -> anyhow::Result<bool> {
                <Self as $crate::SimpleStorage>::delete(self, id)
            }

            pub fn exists(&self, id: &str) -> anyhow::Result<bool> {
                <Self as $crate::SimpleStorage>::exists(self, id)
            }

            pub fn count(&self) -> anyhow::Result<usize> {
                <Self as $crate::SimpleStorage>::count(self)
            }
        }

        impl $crate::SimpleStorage for $name {
            const TABLE: redb::TableDefinition<'static, &'static str, &'static [u8]> = TABLE;

            fn db(&self) -> &std::sync::Arc<redb::Database> {
                &self.db
            }
        }
    };
}
