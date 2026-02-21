//! Unified search across memory chunks and chat sessions.

use crate::memory::{SearchConfig, SearchEngine};
use crate::models::chat_session::{ChatMessage, ChatRole, ChatSession};
use crate::models::memory::UnifiedSearchQuery;
use crate::storage::{ChatSessionStorage, MemoryStorage};
use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Source of a unified search result.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum SearchResultSource {
    /// Result from long-term memory storage.
    Memory,
    /// Result from a chat session message.
    Session {
        session_id: String,
        session_name: String,
    },
}

/// Unified search result across memory and chat sessions.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UnifiedSearchResult {
    /// Unique identifier for this result.
    pub id: String,
    /// Matched content.
    pub content: String,
    /// Result source.
    pub source: SearchResultSource,
    /// Relevance score (0-100).
    pub score: f64,
    /// Timestamp of the matched content.
    pub timestamp: i64,
    /// Optional context surrounding the match.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

/// Result counts grouped by source.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SourceCounts {
    pub memory: u32,
    pub sessions: u32,
}

/// Unified search result set.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UnifiedSearchResults {
    pub results: Vec<UnifiedSearchResult>,
    pub total_count: u32,
    pub has_more: bool,
    pub source_counts: SourceCounts,
}

/// Configuration for unified search.
#[derive(Debug, Clone)]
pub struct UnifiedSearchConfig {
    pub memory_config: SearchConfig,
    pub memory_weight: f64,
    pub min_score: f64,
}

impl Default for UnifiedSearchConfig {
    fn default() -> Self {
        Self {
            memory_config: SearchConfig::default(),
            memory_weight: 0.5,
            min_score: 10.0,
        }
    }
}

/// Unified search engine combining memory and session results.
#[derive(Clone)]
pub struct UnifiedSearchEngine {
    memory_engine: SearchEngine,
    chat_storage: ChatSessionStorage,
    config: UnifiedSearchConfig,
}

impl UnifiedSearchEngine {
    /// Create a new unified search engine with default configuration.
    pub fn new(memory_storage: MemoryStorage, chat_storage: ChatSessionStorage) -> Self {
        Self {
            memory_engine: SearchEngine::new(memory_storage),
            chat_storage,
            config: UnifiedSearchConfig::default(),
        }
    }

    /// Create a new unified search engine with custom configuration.
    pub fn with_config(
        memory_storage: MemoryStorage,
        chat_storage: ChatSessionStorage,
        config: UnifiedSearchConfig,
    ) -> Self {
        Self {
            memory_engine: SearchEngine::with_config(memory_storage, config.memory_config.clone()),
            chat_storage,
            config,
        }
    }

    /// Search both memory chunks and chat sessions.
    pub fn search(&self, query: &UnifiedSearchQuery) -> Result<UnifiedSearchResults> {
        let mut all_results = Vec::new();
        let mut source_counts = SourceCounts::default();

        let mut memory_query = query.base.clone();
        memory_query.limit = u32::MAX;
        memory_query.offset = 0;

        let memory_results = self.memory_engine.search_ranked(&memory_query)?;
        for scored in memory_results.chunks {
            all_results.push(UnifiedSearchResult {
                id: scored.chunk.id.clone(),
                content: scored.chunk.content.clone(),
                source: SearchResultSource::Memory,
                score: scored.score * self.config.memory_weight,
                timestamp: scored.chunk.created_at,
                context: None,
            });
        }
        source_counts.memory = memory_results.total_count;

        if query.include_sessions {
            let session_results = self.search_sessions(query)?;
            source_counts.sessions = session_results.len() as u32;
            for result in session_results {
                all_results.push(UnifiedSearchResult {
                    score: result.score * (1.0 - self.config.memory_weight),
                    ..result
                });
            }
        }

        all_results.retain(|result| result.score >= self.config.min_score);

        all_results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let total_count = all_results.len() as u32;
        let has_more = total_count > query.base.offset.saturating_add(query.base.limit);

        let offset = query.base.offset as usize;
        let limit = query.base.limit as usize;
        let paginated = all_results.into_iter().skip(offset).take(limit).collect();

        Ok(UnifiedSearchResults {
            results: paginated,
            total_count,
            has_more,
            source_counts,
        })
    }

    fn search_sessions(&self, query: &UnifiedSearchQuery) -> Result<Vec<UnifiedSearchResult>> {
        let Some(ref search_text) = query.base.query else {
            return Ok(Vec::new());
        };

        let agent_id = query
            .session_agent_id
            .as_deref()
            .unwrap_or(&query.base.agent_id);

        let search_lower = search_text.to_lowercase();
        let keywords: Vec<&str> = search_lower.split_whitespace().collect();
        if keywords.is_empty() {
            return Ok(Vec::new());
        }

        let sessions = self.chat_storage.list_by_agent(agent_id)?;
        let mut results = Vec::new();

        for session in sessions {
            if !query.session_ids.is_empty() && !query.session_ids.contains(&session.id) {
                continue;
            }
            self.collect_session_results(&session, &keywords, &mut results);
        }

        Ok(results)
    }

    fn collect_session_results(
        &self,
        session: &ChatSession,
        keywords: &[&str],
        results: &mut Vec<UnifiedSearchResult>,
    ) {
        let now = Utc::now().timestamp_millis();

        for (idx, message) in session.messages.iter().enumerate() {
            let content_lower = message.content.to_lowercase();
            let match_count: usize = keywords
                .iter()
                .map(|keyword| content_lower.matches(keyword).count())
                .sum();

            if match_count == 0 {
                continue;
            }

            let word_count = message.content.split_whitespace().count().max(1);
            let frequency_score = (match_count as f64 / word_count as f64) * 100.0;

            let age_hours = (now - message.timestamp).max(0) as f64 / (1000.0 * 60.0 * 60.0);
            let recency_score = 100.0 / (1.0 + age_hours * 0.01);

            let score = frequency_score * 0.6 + recency_score * 0.4;
            let context = self.build_message_context(&session.messages, idx);

            results.push(UnifiedSearchResult {
                id: format!("{}:{}", session.id, idx),
                content: message.content.clone(),
                source: SearchResultSource::Session {
                    session_id: session.id.clone(),
                    session_name: session.name.clone(),
                },
                score,
                timestamp: message.timestamp,
                context: Some(context),
            });
        }
    }

    fn build_message_context(&self, messages: &[ChatMessage], idx: usize) -> String {
        let mut context_parts = Vec::new();

        if idx > 0 {
            let prev = &messages[idx - 1];
            context_parts.push(format!(
                "[{}]: {}...",
                role_label(&prev.role),
                preview_content(&prev.content)
            ));
        }

        if idx + 1 < messages.len() {
            let next = &messages[idx + 1];
            context_parts.push(format!(
                "[{}]: {}...",
                role_label(&next.role),
                preview_content(&next.content)
            ));
        }

        context_parts.join("\n")
    }
}

fn role_label(role: &ChatRole) -> &'static str {
    match role {
        ChatRole::User => "User",
        ChatRole::Assistant => "Assistant",
        ChatRole::System => "System",
    }
}

fn preview_content(content: &str) -> String {
    content.chars().take(50).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::chat_session::ChatMessage;
    use crate::models::memory::{MemoryChunk, MemorySearchQuery, SearchMode};
    use crate::storage::ChatSessionStorage;
    use redb::Database;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn create_engine() -> (UnifiedSearchEngine, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let memory_storage = MemoryStorage::new(db.clone()).unwrap();
        let chat_storage = ChatSessionStorage::new(db).unwrap();

        (
            UnifiedSearchEngine::new(memory_storage, chat_storage),
            temp_dir,
        )
    }

    #[test]
    fn test_search_memory_only() {
        let (engine, _temp) = create_engine();

        let chunk = MemoryChunk::new("agent-1".to_string(), "Rust memory guide".to_string());
        engine.memory_engine.storage().store_chunk(&chunk).unwrap();

        let base = MemorySearchQuery::new("agent-1".to_string())
            .with_query("rust".to_string())
            .with_mode(SearchMode::Keyword)
            .paginate(10, 0);
        let query = UnifiedSearchQuery::new(base).with_sessions(false);

        let results = engine.search(&query).unwrap();
        assert_eq!(results.source_counts.memory, 1);
        assert_eq!(results.source_counts.sessions, 0);
        assert!(!results.results.is_empty());
    }

    #[test]
    fn test_search_sessions() {
        let (engine, _temp) = create_engine();
        let mut session = ChatSession::new("agent-1".to_string(), "claude".to_string());
        session.add_message(ChatMessage::user("Tell me about Rust"));
        session.add_message(ChatMessage::assistant(
            "Rust is a systems programming language",
        ));
        engine.chat_storage.save(&session).unwrap();

        let base = MemorySearchQuery::new("agent-1".to_string())
            .with_query("rust".to_string())
            .with_mode(SearchMode::Keyword)
            .paginate(10, 0);
        let query = UnifiedSearchQuery::new(base).with_sessions(true);

        let results = engine.search(&query).unwrap();
        assert!(results.source_counts.sessions > 0);
    }

    #[test]
    fn test_unified_search_combines_sources() {
        let (engine, _temp) = create_engine();

        let chunk = MemoryChunk::new("agent-1".to_string(), "Rust memory safety".to_string());
        engine.memory_engine.storage().store_chunk(&chunk).unwrap();

        let mut session = ChatSession::new("agent-1".to_string(), "claude".to_string());
        session.add_message(ChatMessage::assistant("Rust prevents memory leaks"));
        engine.chat_storage.save(&session).unwrap();

        let base = MemorySearchQuery::new("agent-1".to_string())
            .with_query("rust memory".to_string())
            .with_mode(SearchMode::Keyword)
            .paginate(10, 0);
        let query = UnifiedSearchQuery::new(base).with_sessions(true);

        let results = engine.search(&query).unwrap();
        assert!(results.source_counts.memory > 0);
        assert!(results.source_counts.sessions > 0);
        assert!(results.results.len() >= 2);
    }
}
