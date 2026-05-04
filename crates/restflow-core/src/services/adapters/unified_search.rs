//! UnifiedMemorySearch adapter backed by UnifiedSearchEngine.

use crate::memory::UnifiedSearchEngine;
use crate::models::{MemorySearchQuery, SearchMode, UnifiedSearchQuery};
use restflow_traits::store::UnifiedMemorySearch;
use serde_json::Value;

pub struct UnifiedMemorySearchAdapter {
    engine: UnifiedSearchEngine,
}

impl UnifiedMemorySearchAdapter {
    pub fn new(engine: UnifiedSearchEngine) -> Self {
        Self { engine }
    }
}

impl UnifiedMemorySearch for UnifiedMemorySearchAdapter {
    fn search(
        &self,
        agent_id: &str,
        query: &str,
        include_sessions: bool,
        limit: u32,
        offset: u32,
    ) -> restflow_tools::Result<Value> {
        let base = MemorySearchQuery::new(agent_id.to_string())
            .with_query(query.to_string())
            .with_mode(SearchMode::Keyword)
            .paginate(limit, offset);
        let unified_query = UnifiedSearchQuery::new(base).with_sessions(include_sessions);

        let results = self.engine.search(&unified_query)?;

        Ok(serde_json::to_value(results)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::session::SessionService;
    use crate::storage::MemoryStorage;
    use crate::storage::Storage;
    use restflow_traits::store::UnifiedMemorySearch;
    use tempfile::tempdir;

    fn setup() -> (UnifiedMemorySearchAdapter, MemoryStorage, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
        let memory_storage = storage.memory.clone();
        let session_service = SessionService::new(
            storage.sessions.clone(),
            Some(storage.agents.clone()),
            storage.tasks.clone(),
            Some(storage.memory.clone()),
        );
        let engine = UnifiedSearchEngine::new(memory_storage.clone(), session_service);
        (
            UnifiedMemorySearchAdapter::new(engine),
            memory_storage,
            temp_dir,
        )
    }

    #[test]
    fn test_search_empty_returns_valid_json() {
        let (adapter, _storage, _dir) = setup();
        let result = adapter.search("agent-1", "anything", false, 10, 0).unwrap();
        assert!(result.is_object());
    }

    #[test]
    fn test_search_with_data() {
        let (adapter, storage, _dir) = setup();
        let chunk = crate::models::memory::MemoryChunk::new(
            "agent-1".to_string(),
            "Rust is a systems programming language".to_string(),
        );
        storage.store_chunk(&chunk).unwrap();

        let result = adapter.search("agent-1", "rust", false, 10, 0).unwrap();
        assert!(result.is_object());
    }

    #[test]
    fn test_search_with_sessions() {
        let (adapter, _storage, _dir) = setup();
        let result = adapter.search("agent-1", "test", true, 10, 0).unwrap();
        assert!(result.is_object());
    }
}
