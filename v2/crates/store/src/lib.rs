//! # codocia
//!
//! Store owns backend-neutral repository contracts.
//!
//! ## Owns
//! - Repository trait
//! - memory repository
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

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
}

pub type Store<T> = dyn Repository<T>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    #[derive(Debug, Clone, PartialEq, Eq)]
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
