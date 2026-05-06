use anyhow::Result;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};
use std::sync::{Mutex, OnceLock};

type StoreKey = (usize, &'static str);
type StoreMap = HashMap<StoreKey, BTreeMap<String, Vec<u8>>>;

fn simple_stores() -> &'static Mutex<StoreMap> {
    static STORES: OnceLock<Mutex<StoreMap>> = OnceLock::new();
    STORES.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn namespace_for_db(db: &Arc<redb::Database>) -> usize {
    type NamespaceRegistry = HashMap<usize, (Weak<redb::Database>, usize)>;

    static NEXT_NAMESPACE: AtomicUsize = AtomicUsize::new(1);
    static REGISTRY: OnceLock<Mutex<NamespaceRegistry>> = OnceLock::new();

    let ptr = Arc::as_ptr(db) as usize;
    let mut registry = REGISTRY
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("storage namespace registry lock poisoned");

    if let Some((weak, namespace)) = registry.get(&ptr)
        && weak.upgrade().is_some()
    {
        return *namespace;
    }

    let namespace = NEXT_NAMESPACE.fetch_add(1, Ordering::Relaxed);
    registry.insert(ptr, (Arc::downgrade(db), namespace));
    namespace
}

/// Trait for simple in-process key-value storage modules.
pub trait SimpleStorage: Send + Sync {
    /// Logical store name for this storage type.
    const STORE: &'static str;

    /// Process-local namespace derived from the owning database handle.
    fn namespace(&self) -> usize;

    /// Get reference to the database handle that owns this namespace.
    fn db(&self) -> &std::sync::Arc<redb::Database>;

    /// Insert only if key doesn't exist (atomic check-and-insert).
    fn insert_if_absent(&self, id: &str, data: &[u8]) -> Result<bool> {
        let mut stores = simple_stores().lock().expect("simple store lock poisoned");
        let store = stores.entry((self.namespace(), Self::STORE)).or_default();
        if store.contains_key(id) {
            return Ok(false);
        }
        store.insert(id.to_string(), data.to_vec());
        Ok(true)
    }

    /// Store raw bytes by ID.
    fn put_raw(&self, id: &str, data: &[u8]) -> Result<()> {
        let mut stores = simple_stores().lock().expect("simple store lock poisoned");
        stores
            .entry((self.namespace(), Self::STORE))
            .or_default()
            .insert(id.to_string(), data.to_vec());
        Ok(())
    }

    /// Get raw bytes by ID.
    fn get_raw(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let stores = simple_stores().lock().expect("simple store lock poisoned");
        Ok(stores
            .get(&(self.namespace(), Self::STORE))
            .and_then(|store| store.get(id).cloned()))
    }

    /// List all entries as (id, data) pairs.
    fn list_raw(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let stores = simple_stores().lock().expect("simple store lock poisoned");
        Ok(stores
            .get(&(self.namespace(), Self::STORE))
            .map(|store| {
                store
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Delete by ID, returns true if existed.
    fn delete(&self, id: &str) -> Result<bool> {
        let mut stores = simple_stores().lock().expect("simple store lock poisoned");
        Ok(stores
            .get_mut(&(self.namespace(), Self::STORE))
            .is_some_and(|store| store.remove(id).is_some()))
    }

    /// Delete multiple IDs in one transaction.
    fn delete_many(&self, ids: &[String]) -> Result<usize> {
        let mut stores = simple_stores().lock().expect("simple store lock poisoned");
        let Some(store) = stores.get_mut(&(self.namespace(), Self::STORE)) else {
            return Ok(0);
        };
        let mut deleted = 0usize;
        for id in ids {
            if store.remove(id).is_some() {
                deleted += 1;
            }
        }
        Ok(deleted)
    }

    /// Check if ID exists.
    fn exists(&self, id: &str) -> Result<bool> {
        let stores = simple_stores().lock().expect("simple store lock poisoned");
        Ok(stores
            .get(&(self.namespace(), Self::STORE))
            .is_some_and(|store| store.contains_key(id)))
    }

    /// Check which IDs exist in a single read transaction.
    fn exists_many(&self, ids: &[&str]) -> Result<HashSet<String>> {
        let stores = simple_stores().lock().expect("simple store lock poisoned");
        let Some(store) = stores.get(&(self.namespace(), Self::STORE)) else {
            return Ok(HashSet::new());
        };
        let found = ids
            .iter()
            .copied()
            .filter(|id| store.contains_key(*id))
            .map(str::to_string)
            .collect();
        Ok(found)
    }

    /// Count all entries.
    fn count(&self) -> Result<usize> {
        let stores = simple_stores().lock().expect("simple store lock poisoned");
        Ok(stores
            .get(&(self.namespace(), Self::STORE))
            .map(BTreeMap::len)
            .unwrap_or_default())
    }
}

/// Macro to generate a simple storage struct with common implementations.
#[macro_export]
macro_rules! define_simple_storage {
    ( $(#[$meta:meta])* $vis:vis struct $name:ident { store: $store_name:literal } ) => {
        $(#[$meta])*
        #[derive(Debug, Clone)]
        $vis struct $name {
            db: std::sync::Arc<redb::Database>,
            namespace: usize,
        }

        impl $name {
            pub fn new(db: std::sync::Arc<redb::Database>) -> anyhow::Result<Self> {
                Ok(Self {
                    namespace: $crate::simple_storage::namespace_for_db(&db),
                    db,
                })
            }
        }

        impl $crate::SimpleStorage for $name {
            const STORE: &'static str = $store_name;

            fn namespace(&self) -> usize {
                self.namespace
            }

            fn db(&self) -> &std::sync::Arc<redb::Database> {
                &self.db
            }
        }
    };
}
