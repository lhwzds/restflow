//! # codocia
//!
//! Store owns backend-neutral repository contracts.
//!
//! ## Owns
//! - Repository trait
//! - memory repository
//! - redb repository adapter
//! - get/list/put/delete repository contract
//! - backend abstraction boundary
//!
//! ## Must Not
//! - own business decisions
//! - expose backend handles to runtime modules
//! - store UI overlay state
//!
//! ## Inputs
//! - record IDs
//! - typed records
//!
//! ## Outputs
//! - persisted records
//! - record lists
//! - deletion status
//!
//! ## Used By
//! - auth
//! - chat
//! - run
//!
//! ## Verify
//! - cargo check -p store

use anyhow::{Context, Result};
use async_trait::async_trait;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::path::Path;
use std::sync::{Arc, RwLock};

#[async_trait]
pub trait Repository<T>: Send + Sync
where
    T: Clone + Send + Sync,
{
    async fn get(&self, id: &str) -> Result<Option<T>>;
    async fn list(&self) -> Result<Vec<T>>;
    async fn put(&self, id: &str, value: T) -> Result<()>;
    async fn delete(&self, id: &str) -> Result<bool>;
    async fn replace_all(&self, records: Vec<(String, T)>) -> Result<()>;

    async fn exists(&self, id: &str) -> Result<bool> {
        Ok(self.get(id).await?.is_some())
    }
}

pub trait Identified {
    fn id(&self) -> &str;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record<T> {
    pub id: String,
    pub value: T,
}

impl<T> Identified for Record<T> {
    fn id(&self) -> &str {
        &self.id
    }
}

#[derive(Debug, Clone)]
pub struct MemoryStore<T> {
    records: Arc<RwLock<BTreeMap<String, T>>>,
}

impl<T> Default for MemoryStore<T> {
    fn default() -> Self {
        Self {
            records: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }
}

impl<T> MemoryStore<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shared() -> SharedStore<T>
    where
        T: Clone + Send + Sync + 'static,
    {
        Arc::new(Self::new())
    }
}

#[derive(Debug, Clone)]
pub struct RedbStore<T> {
    database: Arc<Database>,
    table_name: &'static str,
    _record: PhantomData<fn() -> T>,
}

impl<T> RedbStore<T> {
    pub fn open(path: impl AsRef<Path>, table_name: &'static str) -> Result<Self> {
        Self::new(Arc::new(Database::create(path)?), table_name)
    }

    pub fn new(database: Arc<Database>, table_name: &'static str) -> Result<Self> {
        let store = Self {
            database,
            table_name,
            _record: PhantomData,
        };
        store.ensure_table()?;
        Ok(store)
    }

    pub fn shared(self) -> SharedStore<T>
    where
        T: Clone + Send + Sync + Serialize + DeserializeOwned + 'static,
    {
        Arc::new(self)
    }

    fn table(&self) -> TableDefinition<'static, &'static str, &'static [u8]> {
        TableDefinition::new(self.table_name)
    }

    fn ensure_table(&self) -> Result<()> {
        let write = self.database.begin_write()?;
        write.open_table(self.table())?;
        write.commit()?;
        Ok(())
    }

    fn encode(value: &T) -> Result<Vec<u8>>
    where
        T: Serialize,
    {
        serde_json::to_vec(value).context("encode redb store record")
    }

    fn decode(bytes: &[u8]) -> Result<T>
    where
        T: DeserializeOwned,
    {
        serde_json::from_slice(bytes).context("decode redb store record")
    }
}

#[async_trait]
impl<T> Repository<T> for RedbStore<T>
where
    T: Clone + Send + Sync + Serialize + DeserializeOwned + 'static,
{
    async fn get(&self, id: &str) -> Result<Option<T>> {
        let read = self.database.begin_read()?;
        let table = read.open_table(self.table())?;
        let Some(value) = table.get(id)? else {
            return Ok(None);
        };
        Ok(Some(Self::decode(value.value()).with_context(|| {
            format!(
                "decode redb store record in table `{}` for id `{id}`",
                self.table_name
            )
        })?))
    }

    async fn list(&self) -> Result<Vec<T>> {
        let read = self.database.begin_read()?;
        let table = read.open_table(self.table())?;
        let mut records = Vec::new();
        for item in table.iter()? {
            let (key, value) = item?;
            let id = key.value().to_string();
            records.push(Self::decode(value.value()).with_context(|| {
                format!(
                    "decode redb store record in table `{}` for id `{id}`",
                    self.table_name
                )
            })?);
        }
        Ok(records)
    }

    async fn put(&self, id: &str, value: T) -> Result<()> {
        let encoded = Self::encode(&value)?;
        let write = self.database.begin_write()?;
        {
            let mut table = write.open_table(self.table())?;
            table.insert(id, encoded.as_slice())?;
        }
        write.commit()?;
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<bool> {
        let write = self.database.begin_write()?;
        let deleted = {
            let mut table = write.open_table(self.table())?;
            table.remove(id)?.is_some()
        };
        write.commit()?;
        Ok(deleted)
    }

    async fn replace_all(&self, records: Vec<(String, T)>) -> Result<()> {
        let encoded_records = records
            .iter()
            .map(|(id, value)| Ok((id.clone(), Self::encode(value)?)))
            .collect::<Result<Vec<_>>>()?;

        let write = self.database.begin_write()?;
        {
            let mut table = write.open_table(self.table())?;
            let existing_ids = table
                .iter()?
                .map(|item| item.map(|(key, _)| key.value().to_string()))
                .collect::<std::result::Result<Vec<_>, _>>()?;
            for id in existing_ids {
                table.remove(id.as_str())?;
            }
            for (id, encoded) in encoded_records {
                table.insert(id.as_str(), encoded.as_slice())?;
            }
        }
        write.commit()?;
        Ok(())
    }
}

#[async_trait]
impl<T> Repository<T> for MemoryStore<T>
where
    T: Clone + Send + Sync,
{
    async fn get(&self, id: &str) -> Result<Option<T>> {
        Ok(self
            .records
            .read()
            .expect("memory store lock")
            .get(id)
            .cloned())
    }

    async fn list(&self) -> Result<Vec<T>> {
        Ok(self
            .records
            .read()
            .expect("memory store lock")
            .values()
            .cloned()
            .collect())
    }

    async fn put(&self, id: &str, value: T) -> Result<()> {
        self.records
            .write()
            .expect("memory store lock")
            .insert(id.to_string(), value);
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<bool> {
        Ok(self
            .records
            .write()
            .expect("memory store lock")
            .remove(id)
            .is_some())
    }

    async fn replace_all(&self, records: Vec<(String, T)>) -> Result<()> {
        let mut current = self.records.write().expect("memory store lock");
        current.clear();
        current.extend(records);
        Ok(())
    }
}

#[async_trait]
impl<T, R> Repository<T> for Arc<R>
where
    T: Clone + Send + Sync + 'static,
    R: Repository<T> + ?Sized,
{
    async fn get(&self, id: &str) -> Result<Option<T>> {
        self.as_ref().get(id).await
    }

    async fn list(&self) -> Result<Vec<T>> {
        self.as_ref().list().await
    }

    async fn put(&self, id: &str, value: T) -> Result<()> {
        self.as_ref().put(id, value).await
    }

    async fn delete(&self, id: &str) -> Result<bool> {
        self.as_ref().delete(id).await
    }

    async fn replace_all(&self, records: Vec<(String, T)>) -> Result<()> {
        self.as_ref().replace_all(records).await
    }
}

pub type Store<T> = dyn Repository<T>;
pub type SharedStore<T> = Arc<Store<T>>;

pub fn memory_store<T>() -> SharedStore<T>
where
    T: Clone + Send + Sync + 'static,
{
    MemoryStore::shared()
}

pub fn open_redb_database(path: impl AsRef<Path>) -> Result<Arc<Database>> {
    Ok(Arc::new(Database::create(path)?))
}

pub fn redb_store<T>(database: Arc<Database>, table_name: &'static str) -> Result<SharedStore<T>>
where
    T: Clone + Send + Sync + Serialize + DeserializeOwned + 'static,
{
    Ok(RedbStore::new(database, table_name)?.shared())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct Item {
        id: String,
        value: String,
    }

    #[test]
    fn memory_store_put_get_list_and_delete() {
        block_on_once(async {
            let store = MemoryStore::new();
            store
                .put(
                    "a",
                    Item {
                        id: "a".to_string(),
                        value: "one".to_string(),
                    },
                )
                .await
                .unwrap();

            assert!(store.exists("a").await.unwrap());
            assert_eq!(store.get("a").await.unwrap().unwrap().value, "one");
            assert_eq!(store.list().await.unwrap().len(), 1);
            assert!(store.delete("a").await.unwrap());
            assert!(!store.exists("a").await.unwrap());
            assert!(!store.delete("a").await.unwrap());
        });
    }

    #[test]
    fn shared_store_uses_repository_trait_object() {
        block_on_once(async {
            let store: SharedStore<Item> = memory_store();
            store
                .put(
                    "a",
                    Item {
                        id: "a".to_string(),
                        value: "one".to_string(),
                    },
                )
                .await
                .unwrap();

            assert!(store.exists("a").await.unwrap());
            store.replace_all(Vec::new()).await.unwrap();
            assert!(!store.exists("a").await.unwrap());
        });
    }

    #[test]
    fn redb_store_persists_records_across_handles() {
        block_on_once(async {
            let path = temp_db_path("redb-store-persist");
            {
                let store = RedbStore::<Item>::open(&path, "items").unwrap();
                store
                    .put(
                        "a",
                        Item {
                            id: "a".to_string(),
                            value: "one".to_string(),
                        },
                    )
                    .await
                    .unwrap();
            }

            {
                let store = RedbStore::<Item>::open(&path, "items").unwrap();
                assert!(store.exists("a").await.unwrap());
                assert_eq!(store.get("a").await.unwrap().unwrap().value, "one");
                assert_eq!(store.list().await.unwrap().len(), 1);
                assert!(store.delete("a").await.unwrap());
                assert!(!store.exists("a").await.unwrap());
                assert!(!store.delete("a").await.unwrap());
            }

            let _ = std::fs::remove_file(path);
        });
    }

    #[test]
    fn redb_store_replace_all_is_table_scoped() {
        block_on_once(async {
            let path = temp_db_path("redb-store-replace");
            let database = Arc::new(Database::create(&path).unwrap());
            let items = RedbStore::<Item>::new(Arc::clone(&database), "items").unwrap();
            let other = RedbStore::<Item>::new(database, "other_items").unwrap();

            items
                .put(
                    "old",
                    Item {
                        id: "old".to_string(),
                        value: "stale".to_string(),
                    },
                )
                .await
                .unwrap();
            other
                .put(
                    "keep",
                    Item {
                        id: "keep".to_string(),
                        value: "other".to_string(),
                    },
                )
                .await
                .unwrap();

            items
                .replace_all(vec![(
                    "new".to_string(),
                    Item {
                        id: "new".to_string(),
                        value: "fresh".to_string(),
                    },
                )])
                .await
                .unwrap();

            assert!(!items.exists("old").await.unwrap());
            assert_eq!(items.get("new").await.unwrap().unwrap().value, "fresh");
            assert_eq!(other.get("keep").await.unwrap().unwrap().value, "other");

            let _ = std::fs::remove_file(path);
        });
    }

    fn temp_db_path(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{name}-{nanos}.redb"))
    }

    fn block_on_once<T>(future: impl Future<Output = T>) -> T {
        let waker = Waker::from(Arc::new(NoopWake));
        let mut context = Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);

        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("memory store future unexpectedly yielded"),
        }
    }

    struct NoopWake;

    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }
}
