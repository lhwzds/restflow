//! Process-local memory storage.

use crate::simple_storage::namespace_for_db;
use anyhow::Result;
use redb::Database;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone)]
struct ChunkRecord {
    agent_id: String,
    session_id: Option<String>,
    content_hash: String,
    tags: Vec<String>,
    data: Vec<u8>,
}

#[derive(Clone)]
struct SessionRecord {
    agent_id: String,
    data: Vec<u8>,
}

#[derive(Default)]
struct MemoryStore {
    chunks: BTreeMap<String, ChunkRecord>,
    sessions: BTreeMap<String, SessionRecord>,
}

fn stores() -> &'static Mutex<HashMap<usize, MemoryStore>> {
    static STORES: OnceLock<Mutex<HashMap<usize, MemoryStore>>> = OnceLock::new();
    STORES.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Clone)]
pub struct MemoryStorage {
    namespace: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PutChunkResult {
    Created(String),
    Existing(String),
}

impl MemoryStorage {
    pub fn new(db: Arc<Database>) -> Result<Self> {
        Ok(Self {
            namespace: namespace_for_db(&db),
        })
    }

    pub fn put_chunk_if_not_exists(
        &self,
        chunk_id: &str,
        agent_id: &str,
        session_id: Option<&str>,
        content_hash: &str,
        tags: &[String],
        data: &[u8],
    ) -> Result<PutChunkResult> {
        let mut stores = stores().lock().expect("memory store lock poisoned");
        let store = stores.entry(self.namespace).or_default();
        if let Some((existing_id, _)) = store
            .chunks
            .iter()
            .find(|(_, record)| record.agent_id == agent_id && record.content_hash == content_hash)
        {
            return Ok(PutChunkResult::Existing(existing_id.clone()));
        }
        store.chunks.insert(
            chunk_id.to_string(),
            ChunkRecord {
                agent_id: agent_id.to_string(),
                session_id: session_id.map(str::to_string),
                content_hash: content_hash.to_string(),
                tags: tags.to_vec(),
                data: data.to_vec(),
            },
        );
        Ok(PutChunkResult::Created(chunk_id.to_string()))
    }

    pub fn put_chunk_raw(
        &self,
        chunk_id: &str,
        agent_id: &str,
        session_id: Option<&str>,
        content_hash: &str,
        tags: &[String],
        data: &[u8],
    ) -> Result<()> {
        let mut stores = stores().lock().expect("memory store lock poisoned");
        stores.entry(self.namespace).or_default().chunks.insert(
            chunk_id.to_string(),
            ChunkRecord {
                agent_id: agent_id.to_string(),
                session_id: session_id.map(str::to_string),
                content_hash: content_hash.to_string(),
                tags: tags.to_vec(),
                data: data.to_vec(),
            },
        );
        Ok(())
    }

    pub fn get_chunk_raw(&self, chunk_id: &str) -> Result<Option<Vec<u8>>> {
        let stores = stores().lock().expect("memory store lock poisoned");
        Ok(stores
            .get(&self.namespace)
            .and_then(|store| store.chunks.get(chunk_id).map(|record| record.data.clone())))
    }

    pub fn list_chunks_by_agent_raw(&self, agent_id: &str) -> Result<Vec<(String, Vec<u8>)>> {
        self.list_chunks_matching(|record| record.agent_id == agent_id)
    }

    pub fn list_all_chunks_raw(&self) -> Result<Vec<(String, Vec<u8>)>> {
        self.list_chunks_matching(|_| true)
    }

    pub fn list_chunks_by_session_raw(&self, session_id: &str) -> Result<Vec<(String, Vec<u8>)>> {
        self.list_chunks_matching(|record| record.session_id.as_deref() == Some(session_id))
    }

    pub fn has_chunks_for_session(&self, session_id: &str) -> Result<bool> {
        let stores = stores().lock().expect("memory store lock poisoned");
        Ok(stores.get(&self.namespace).is_some_and(|store| {
            store
                .chunks
                .values()
                .any(|record| record.session_id.as_deref() == Some(session_id))
        }))
    }

    pub fn list_chunks_by_tag_raw(&self, tag: &str) -> Result<Vec<(String, Vec<u8>)>> {
        self.list_chunks_matching(|record| record.tags.iter().any(|item| item == tag))
    }

    fn list_chunks_matching<F>(&self, predicate: F) -> Result<Vec<(String, Vec<u8>)>>
    where
        F: Fn(&ChunkRecord) -> bool,
    {
        let stores = stores().lock().expect("memory store lock poisoned");
        Ok(stores
            .get(&self.namespace)
            .map(|store| {
                store
                    .chunks
                    .iter()
                    .filter(|(_, record)| predicate(record))
                    .map(|(id, record)| (id.clone(), record.data.clone()))
                    .collect()
            })
            .unwrap_or_default())
    }

    pub fn find_by_hash(&self, agent_id: &str, content_hash: &str) -> Result<Option<String>> {
        let stores = stores().lock().expect("memory store lock poisoned");
        Ok(stores.get(&self.namespace).and_then(|store| {
            store
                .chunks
                .iter()
                .find(|(_, record)| {
                    record.agent_id == agent_id && record.content_hash == content_hash
                })
                .map(|(id, _)| id.clone())
        }))
    }

    pub fn delete_chunk(
        &self,
        chunk_id: &str,
        _agent_id: &str,
        _session_id: Option<&str>,
        _content_hash: &str,
        _tags: &[String],
    ) -> Result<bool> {
        let mut stores = stores().lock().expect("memory store lock poisoned");
        Ok(stores
            .get_mut(&self.namespace)
            .is_some_and(|store| store.chunks.remove(chunk_id).is_some()))
    }

    pub fn count_chunks_by_agent(&self, agent_id: &str) -> Result<u32> {
        let stores = stores().lock().expect("memory store lock poisoned");
        Ok(stores
            .get(&self.namespace)
            .map(|store| {
                store
                    .chunks
                    .values()
                    .filter(|record| record.agent_id == agent_id)
                    .count() as u32
            })
            .unwrap_or_default())
    }

    pub fn list_chunks_by_agent_paginated(
        &self,
        agent_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<(String, Vec<u8>)>> {
        let mut rows = self.list_chunks_by_agent_raw(agent_id)?;
        rows.sort_by(|left, right| left.0.cmp(&right.0));
        Ok(rows.into_iter().skip(offset).take(limit).collect())
    }

    pub fn put_session_raw(&self, session_id: &str, agent_id: &str, data: &[u8]) -> Result<()> {
        let mut stores = stores().lock().expect("memory store lock poisoned");
        stores.entry(self.namespace).or_default().sessions.insert(
            session_id.to_string(),
            SessionRecord {
                agent_id: agent_id.to_string(),
                data: data.to_vec(),
            },
        );
        Ok(())
    }

    pub fn get_session_raw(&self, session_id: &str) -> Result<Option<Vec<u8>>> {
        let stores = stores().lock().expect("memory store lock poisoned");
        Ok(stores.get(&self.namespace).and_then(|store| {
            store
                .sessions
                .get(session_id)
                .map(|record| record.data.clone())
        }))
    }

    pub fn list_sessions_by_agent_raw(&self, agent_id: &str) -> Result<Vec<(String, Vec<u8>)>> {
        let stores = stores().lock().expect("memory store lock poisoned");
        Ok(stores
            .get(&self.namespace)
            .map(|store| {
                store
                    .sessions
                    .iter()
                    .filter(|(_, record)| record.agent_id == agent_id)
                    .map(|(id, record)| (id.clone(), record.data.clone()))
                    .collect()
            })
            .unwrap_or_default())
    }

    pub fn delete_session(&self, session_id: &str, _agent_id: &str) -> Result<bool> {
        let mut stores = stores().lock().expect("memory store lock poisoned");
        Ok(stores
            .get_mut(&self.namespace)
            .is_some_and(|store| store.sessions.remove(session_id).is_some()))
    }

    pub fn count_sessions_by_agent(&self, agent_id: &str) -> Result<u32> {
        let stores = stores().lock().expect("memory store lock poisoned");
        Ok(stores
            .get(&self.namespace)
            .map(|store| {
                store
                    .sessions
                    .values()
                    .filter(|record| record.agent_id == agent_id)
                    .count() as u32
            })
            .unwrap_or_default())
    }

    pub fn delete_all_chunks_for_agent_with_metadata(
        &self,
        agent_id: &str,
        chunk_metadata: &[(String, Option<String>, String, Vec<String>)],
    ) -> Result<u32> {
        let mut stores = stores().lock().expect("memory store lock poisoned");
        let Some(store) = stores.get_mut(&self.namespace) else {
            return Ok(0);
        };
        let mut deleted = 0_u32;
        for (chunk_id, _, _, _) in chunk_metadata {
            if store
                .chunks
                .get(chunk_id)
                .is_some_and(|record| record.agent_id == agent_id)
                && store.chunks.remove(chunk_id).is_some()
            {
                deleted += 1;
            }
        }
        Ok(deleted)
    }

    pub fn delete_all_chunks_for_agent(&self, agent_id: &str) -> Result<u32> {
        let mut stores = stores().lock().expect("memory store lock poisoned");
        let Some(store) = stores.get_mut(&self.namespace) else {
            return Ok(0);
        };
        let ids = store
            .chunks
            .iter()
            .filter_map(|(id, record)| (record.agent_id == agent_id).then_some(id.clone()))
            .collect::<Vec<_>>();
        for id in &ids {
            store.chunks.remove(id);
        }
        Ok(ids.len() as u32)
    }

    pub fn delete_all_sessions_for_agent(&self, agent_id: &str) -> Result<u32> {
        let mut stores = stores().lock().expect("memory store lock poisoned");
        let Some(store) = stores.get_mut(&self.namespace) else {
            return Ok(0);
        };
        let ids = store
            .sessions
            .iter()
            .filter_map(|(id, record)| (record.agent_id == agent_id).then_some(id.clone()))
            .collect::<Vec<_>>();
        for id in &ids {
            store.sessions.remove(id);
        }
        Ok(ids.len() as u32)
    }
}
