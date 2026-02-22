//! UnifiedMemorySearch adapter backed by UnifiedSearchEngine.

use crate::memory::UnifiedSearchEngine;
use crate::models::{MemorySearchQuery, SearchMode, UnifiedSearchQuery};
use restflow_ai::tools::UnifiedMemorySearch;
use restflow_tools::ToolError;
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

        let results = self
            .engine
            .search(&unified_query)
            .map_err(|e| ToolError::Tool(e.to_string()))?;

        Ok(serde_json::to_value(results)?)
    }
}
