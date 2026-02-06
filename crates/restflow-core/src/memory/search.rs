//! Search engine for memory retrieval with relevance scoring.
//!
//! This module provides a search engine that wraps the storage layer's
//! basic search functionality and adds relevance scoring based on:
//!
//! - **Frequency**: How many times search terms appear in the content
//! - **Recency**: More recent chunks get higher scores
//!
//! # Example
//!
//! ```ignore
//! use restflow_core::memory::SearchEngine;
//!
//! let engine = SearchEngine::new(storage);
//!
//! // Search with relevance scoring
//! let results = engine.search_ranked(&query)?;
//! for result in results.chunks {
//!     println!("{:.2} - {}", result.score, result.chunk.content);
//! }
//! ```
//!
//! # Scoring Algorithm
//!
//! The scoring formula combines frequency and recency:
//!
//! ```text
//! score = (frequency_score * frequency_weight) + (recency_score * recency_weight)
//!
//! where:
//!   frequency_score = (match_count / total_words) * 100
//!   recency_score   = 1.0 / (1.0 + age_in_hours * decay_factor)
//! ```
//!
//! Default weights: frequency=0.7, recency=0.3

use crate::models::memory::{MemoryChunk, MemorySearchQuery, SearchMode};
use crate::storage::MemoryStorage;
use anyhow::Result;
use regex::RegexBuilder;
use restflow_storage::time_utils;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A search result with relevance score.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ScoredChunk {
    /// The memory chunk
    pub chunk: MemoryChunk,
    /// Relevance score (0.0 - 100.0)
    pub score: f64,
    /// Number of keyword matches found
    pub match_count: u32,
    /// Breakdown of score components
    pub score_breakdown: ScoreBreakdown,
}

/// Breakdown of how the score was calculated.
#[derive(Debug, Clone, Serialize, Deserialize, Default, TS)]
#[ts(export)]
pub struct ScoreBreakdown {
    /// Score contribution from keyword frequency
    pub frequency_score: f64,
    /// Score contribution from recency
    pub recency_score: f64,
    /// Score contribution from tag matches
    pub tag_score: f64,
}

/// Results from a ranked search.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RankedSearchResult {
    /// Scored chunks sorted by relevance
    pub chunks: Vec<ScoredChunk>,
    /// Total number of matching chunks (before pagination)
    pub total_count: u32,
    /// Whether there are more results available
    pub has_more: bool,
}

/// Configuration for the search engine scoring.
#[derive(Debug, Clone)]
pub struct SearchConfig {
    /// Weight for frequency score (0.0 - 1.0)
    pub frequency_weight: f64,
    /// Weight for recency score (0.0 - 1.0)
    pub recency_weight: f64,
    /// Weight for tag match score (0.0 - 1.0)
    pub tag_weight: f64,
    /// Decay factor for recency (higher = faster decay)
    pub recency_decay: f64,
    /// Minimum score threshold (results below this are filtered)
    pub min_score: f64,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            frequency_weight: 0.6,
            recency_weight: 0.3,
            tag_weight: 0.1,
            recency_decay: 0.01, // Gentle decay: ~50% after 70 hours
            min_score: 0.0,
        }
    }
}

impl SearchConfig {
    /// Create a config that prioritizes frequency over recency.
    pub fn frequency_focused() -> Self {
        Self {
            frequency_weight: 0.8,
            recency_weight: 0.1,
            tag_weight: 0.1,
            ..Default::default()
        }
    }

    /// Create a config that prioritizes recency over frequency.
    pub fn recency_focused() -> Self {
        Self {
            frequency_weight: 0.3,
            recency_weight: 0.6,
            tag_weight: 0.1,
            recency_decay: 0.05, // Faster decay
            ..Default::default()
        }
    }

    /// Create a config with equal weights.
    pub fn balanced() -> Self {
        Self {
            frequency_weight: 0.4,
            recency_weight: 0.4,
            tag_weight: 0.2,
            ..Default::default()
        }
    }

    /// Set minimum score threshold.
    pub fn with_min_score(mut self, min_score: f64) -> Self {
        self.min_score = min_score;
        self
    }
}

/// Search engine with relevance scoring.
#[derive(Clone)]
pub struct SearchEngine {
    storage: MemoryStorage,
    config: SearchConfig,
}

impl SearchEngine {
    /// Create a new search engine with default configuration.
    pub fn new(storage: MemoryStorage) -> Self {
        Self {
            storage,
            config: SearchConfig::default(),
        }
    }

    /// Create a new search engine with custom configuration.
    pub fn with_config(storage: MemoryStorage, config: SearchConfig) -> Self {
        Self { storage, config }
    }

    /// Get the current configuration.
    pub fn config(&self) -> &SearchConfig {
        &self.config
    }

    /// Update the configuration.
    pub fn set_config(&mut self, config: SearchConfig) {
        self.config = config;
    }

    /// Search memory with relevance scoring.
    ///
    /// Returns results sorted by score (highest first).
    pub fn search_ranked(&self, query: &MemorySearchQuery) -> Result<RankedSearchResult> {
        // Get basic search results from storage (without pagination for scoring)
        let mut search_query = query.clone();
        search_query.limit = u32::MAX; // Get all results for scoring
        search_query.offset = 0;

        let storage_results = self.storage.search(&search_query)?;

        // Score each chunk
        let mut scored_chunks: Vec<ScoredChunk> = storage_results
            .chunks
            .into_iter()
            .map(|chunk| self.score_chunk(&chunk, query))
            .filter(|sc| sc.score >= self.config.min_score)
            .collect();

        // Sort by score (highest first)
        scored_chunks.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let total_count = scored_chunks.len() as u32;

        // Apply pagination
        let offset = query.offset as usize;
        let limit = query.limit as usize;
        let has_more = total_count > query.offset + query.limit;

        let paginated: Vec<_> = scored_chunks.into_iter().skip(offset).take(limit).collect();

        Ok(RankedSearchResult {
            chunks: paginated,
            total_count,
            has_more,
        })
    }

    /// Score a single chunk based on the query.
    fn score_chunk(&self, chunk: &MemoryChunk, query: &MemorySearchQuery) -> ScoredChunk {
        let mut breakdown = ScoreBreakdown::default();
        let mut match_count = 0u32;

        // Calculate frequency score
        if let Some(ref search_text) = query.query {
            let (freq_score, matches) =
                self.calculate_frequency_score(&chunk.content, search_text, &query.search_mode);
            breakdown.frequency_score = freq_score;
            match_count = matches;
        } else {
            // No search text means all chunks match equally for frequency
            breakdown.frequency_score = 50.0;
        }

        // Calculate recency score
        breakdown.recency_score = self.calculate_recency_score(chunk.created_at);

        // Calculate tag match score
        if !query.tags.is_empty() {
            breakdown.tag_score = self.calculate_tag_score(&chunk.tags, &query.tags);
        }

        // Combine scores with weights
        let score = (breakdown.frequency_score * self.config.frequency_weight)
            + (breakdown.recency_score * self.config.recency_weight)
            + (breakdown.tag_score * self.config.tag_weight);

        ScoredChunk {
            chunk: chunk.clone(),
            score,
            match_count,
            score_breakdown: breakdown,
        }
    }

    /// Calculate frequency score based on keyword occurrences.
    ///
    /// Returns (score, match_count) where score is 0-100.
    fn calculate_frequency_score(
        &self,
        content: &str,
        search_text: &str,
        mode: &SearchMode,
    ) -> (f64, u32) {
        let content_lower = content.to_lowercase();
        let search_lower = search_text.to_lowercase();

        let match_count = match mode {
            SearchMode::Keyword => {
                // Count occurrences of each keyword
                let keywords: Vec<&str> = search_lower.split_whitespace().collect();
                let mut total_matches = 0u32;
                for keyword in &keywords {
                    total_matches += content_lower.matches(keyword).count() as u32;
                }
                total_matches
            }
            SearchMode::Phrase => {
                // Count occurrences of the exact phrase
                content_lower.matches(&search_lower).count() as u32
            }
            SearchMode::Regex => {
                // Count regex matches
                let regex = RegexBuilder::new(search_text)
                    .size_limit(10_000)
                    .dfa_size_limit(10_000)
                    .build();
                match regex {
                    Ok(re) => re.find_iter(content).count() as u32,
                    Err(_) => 0,
                }
            }
        };

        if match_count == 0 {
            return (0.0, 0);
        }

        // Calculate word count for normalization
        let word_count = content.split_whitespace().count().max(1) as f64;

        // Frequency score: matches per 100 words, capped at 100
        let raw_score = (match_count as f64 / word_count) * 100.0;
        let score = raw_score.min(100.0);

        (score, match_count)
    }

    /// Calculate recency score based on age.
    ///
    /// Uses exponential decay: score = 100 / (1 + age_hours * decay)
    fn calculate_recency_score(&self, created_at: i64) -> f64 {
        let now = time_utils::now_ms();

        let age_ms = (now - created_at).max(0) as f64;
        let age_hours = age_ms / (1000.0 * 60.0 * 60.0);

        // Exponential decay formula
        100.0 / (1.0 + age_hours * self.config.recency_decay)
    }

    /// Calculate tag match score.
    ///
    /// Returns percentage of query tags that match chunk tags.
    fn calculate_tag_score(&self, chunk_tags: &[String], query_tags: &[String]) -> f64 {
        if query_tags.is_empty() {
            return 0.0;
        }

        let matches = query_tags
            .iter()
            .filter(|qt| chunk_tags.iter().any(|ct| ct.eq_ignore_ascii_case(qt)))
            .count();

        (matches as f64 / query_tags.len() as f64) * 100.0
    }

    /// Get the underlying storage.
    pub fn storage(&self) -> &MemoryStorage {
        &self.storage
    }
}

/// Builder for SearchEngine with fluent configuration.
pub struct SearchEngineBuilder {
    storage: MemoryStorage,
    config: SearchConfig,
}

impl SearchEngineBuilder {
    /// Create a new builder with the given storage.
    pub fn new(storage: MemoryStorage) -> Self {
        Self {
            storage,
            config: SearchConfig::default(),
        }
    }

    /// Set the frequency weight (0.0 - 1.0).
    pub fn frequency_weight(mut self, weight: f64) -> Self {
        self.config.frequency_weight = weight.clamp(0.0, 1.0);
        self
    }

    /// Set the recency weight (0.0 - 1.0).
    pub fn recency_weight(mut self, weight: f64) -> Self {
        self.config.recency_weight = weight.clamp(0.0, 1.0);
        self
    }

    /// Set the tag weight (0.0 - 1.0).
    pub fn tag_weight(mut self, weight: f64) -> Self {
        self.config.tag_weight = weight.clamp(0.0, 1.0);
        self
    }

    /// Set the recency decay factor.
    pub fn recency_decay(mut self, decay: f64) -> Self {
        self.config.recency_decay = decay.max(0.0);
        self
    }

    /// Set the minimum score threshold.
    pub fn min_score(mut self, min_score: f64) -> Self {
        self.config.min_score = min_score.clamp(0.0, 100.0);
        self
    }

    /// Use frequency-focused preset.
    pub fn frequency_focused(mut self) -> Self {
        self.config = SearchConfig::frequency_focused();
        self
    }

    /// Use recency-focused preset.
    pub fn recency_focused(mut self) -> Self {
        self.config = SearchConfig::recency_focused();
        self
    }

    /// Use balanced preset.
    pub fn balanced(mut self) -> Self {
        self.config = SearchConfig::balanced();
        self
    }

    /// Build the search engine.
    pub fn build(self) -> SearchEngine {
        SearchEngine::with_config(self.storage, self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::memory::MemoryChunk;
    use redb::Database;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn create_test_engine() -> SearchEngine {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = MemoryStorage::new(db).unwrap();
        SearchEngine::new(storage)
    }

    fn now_ms() -> i64 {
        time_utils::now_ms()
    }

    #[test]
    fn test_search_config_default() {
        let config = SearchConfig::default();
        assert!((config.frequency_weight - 0.6).abs() < f64::EPSILON);
        assert!((config.recency_weight - 0.3).abs() < f64::EPSILON);
        assert!((config.tag_weight - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn test_search_config_presets() {
        let freq = SearchConfig::frequency_focused();
        assert!(freq.frequency_weight > freq.recency_weight);

        let recency = SearchConfig::recency_focused();
        assert!(recency.recency_weight > recency.frequency_weight);

        let balanced = SearchConfig::balanced();
        assert!((balanced.frequency_weight - balanced.recency_weight).abs() < f64::EPSILON);
    }

    #[test]
    fn test_search_engine_builder() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = MemoryStorage::new(db).unwrap();

        let engine = SearchEngineBuilder::new(storage)
            .frequency_weight(0.9)
            .recency_weight(0.05)
            .tag_weight(0.05)
            .min_score(10.0)
            .build();

        assert!((engine.config().frequency_weight - 0.9).abs() < f64::EPSILON);
        assert!((engine.config().min_score - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_frequency_score_keyword() {
        let engine = create_test_engine();

        // Test with multiple occurrences
        let content = "rust rust rust programming language rust";
        let (score, matches) =
            engine.calculate_frequency_score(content, "rust", &SearchMode::Keyword);

        assert_eq!(matches, 4);
        assert!(score > 0.0);
    }

    #[test]
    fn test_frequency_score_phrase() {
        let engine = create_test_engine();

        let content = "rust programming is fun. rust programming is great.";
        let (score, matches) =
            engine.calculate_frequency_score(content, "rust programming", &SearchMode::Phrase);

        assert_eq!(matches, 2);
        assert!(score > 0.0);
    }

    #[test]
    fn test_frequency_score_regex() {
        let engine = create_test_engine();

        let content = "error: 404, error: 500, error: 503";
        let (score, matches) =
            engine.calculate_frequency_score(content, r"error: \d+", &SearchMode::Regex);

        assert_eq!(matches, 3);
        assert!(score > 0.0);
    }

    #[test]
    fn test_frequency_score_no_match() {
        let engine = create_test_engine();

        let content = "python javascript typescript";
        let (score, matches) =
            engine.calculate_frequency_score(content, "rust", &SearchMode::Keyword);

        assert_eq!(matches, 0);
        assert!((score - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_recency_score_recent() {
        let engine = create_test_engine();

        // Just now
        let score = engine.calculate_recency_score(now_ms());
        assert!(score > 99.0); // Should be very close to 100
    }

    #[test]
    fn test_recency_score_old() {
        let engine = create_test_engine();

        // 30 days ago
        let old_time = now_ms() - (30 * 24 * 60 * 60 * 1000);
        let score = engine.calculate_recency_score(old_time);

        // Should have decayed significantly
        assert!(score < 50.0);
        assert!(score > 0.0);
    }

    #[test]
    fn test_recency_score_decay_ordering() {
        let engine = create_test_engine();

        let now = now_ms();
        let one_hour_ago = now - (60 * 60 * 1000);
        let one_day_ago = now - (24 * 60 * 60 * 1000);
        let one_week_ago = now - (7 * 24 * 60 * 60 * 1000);

        let score_now = engine.calculate_recency_score(now);
        let score_hour = engine.calculate_recency_score(one_hour_ago);
        let score_day = engine.calculate_recency_score(one_day_ago);
        let score_week = engine.calculate_recency_score(one_week_ago);

        assert!(score_now > score_hour);
        assert!(score_hour > score_day);
        assert!(score_day > score_week);
    }

    #[test]
    fn test_tag_score() {
        let engine = create_test_engine();

        let chunk_tags = vec!["rust".to_string(), "async".to_string(), "tokio".to_string()];
        let query_tags = vec!["rust".to_string(), "async".to_string()];

        let score = engine.calculate_tag_score(&chunk_tags, &query_tags);
        assert!((score - 100.0).abs() < f64::EPSILON); // All query tags match
    }

    #[test]
    fn test_tag_score_partial() {
        let engine = create_test_engine();

        let chunk_tags = vec!["rust".to_string()];
        let query_tags = vec!["rust".to_string(), "python".to_string()];

        let score = engine.calculate_tag_score(&chunk_tags, &query_tags);
        assert!((score - 50.0).abs() < f64::EPSILON); // 1 of 2 tags match
    }

    #[test]
    fn test_tag_score_no_match() {
        let engine = create_test_engine();

        let chunk_tags = vec!["rust".to_string()];
        let query_tags = vec!["python".to_string()];

        let score = engine.calculate_tag_score(&chunk_tags, &query_tags);
        assert!((score - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_search_ranked_basic() {
        let engine = create_test_engine();

        // Store chunks with different content
        let chunk1 = MemoryChunk::new(
            "agent-001".to_string(),
            "Rust is a systems programming language".to_string(),
        )
        .with_created_at(now_ms());
        let chunk2 = MemoryChunk::new(
            "agent-001".to_string(),
            "Rust Rust Rust - all about Rust".to_string(),
        )
        .with_created_at(now_ms());
        let chunk3 = MemoryChunk::new(
            "agent-001".to_string(),
            "Python is great for scripting".to_string(),
        )
        .with_created_at(now_ms());

        engine.storage().store_chunk(&chunk1).unwrap();
        engine.storage().store_chunk(&chunk2).unwrap();
        engine.storage().store_chunk(&chunk3).unwrap();

        let query = MemorySearchQuery::new("agent-001".to_string())
            .with_query("rust".to_string())
            .with_mode(SearchMode::Keyword);

        let results = engine.search_ranked(&query).unwrap();

        assert_eq!(results.chunks.len(), 2);
        // Chunk with more "Rust" occurrences should score higher
        assert!(results.chunks[0].match_count > results.chunks[1].match_count);
        assert!(results.chunks[0].score >= results.chunks[1].score);
    }

    #[test]
    fn test_search_ranked_recency_matters() {
        let mut engine = create_test_engine();

        // Use recency-focused config
        engine.set_config(SearchConfig::recency_focused());

        let now = now_ms();
        let one_week_ago = now - (7 * 24 * 60 * 60 * 1000);

        // Same content, different times
        let chunk_old = MemoryChunk::new(
            "agent-001".to_string(),
            "Important rust information".to_string(),
        )
        .with_created_at(one_week_ago);
        let chunk_new = MemoryChunk::new("agent-001".to_string(), "Another rust fact".to_string())
            .with_created_at(now);

        engine.storage().store_chunk(&chunk_old).unwrap();
        engine.storage().store_chunk(&chunk_new).unwrap();

        let query = MemorySearchQuery::new("agent-001".to_string())
            .with_query("rust".to_string())
            .with_mode(SearchMode::Keyword);

        let results = engine.search_ranked(&query).unwrap();

        assert_eq!(results.chunks.len(), 2);
        // Newer chunk should score higher with recency-focused config
        assert!(results.chunks[0].chunk.created_at > results.chunks[1].chunk.created_at);
    }

    #[test]
    fn test_search_ranked_pagination() {
        let engine = create_test_engine();

        // Store 10 chunks
        for i in 0..10 {
            let chunk = MemoryChunk::new(
                "agent-001".to_string(),
                format!("Rust content number {}", i),
            )
            .with_created_at(now_ms() - (i as i64 * 1000));
            engine.storage().store_chunk(&chunk).unwrap();
        }

        let query = MemorySearchQuery::new("agent-001".to_string())
            .with_query("rust".to_string())
            .with_mode(SearchMode::Keyword)
            .paginate(3, 0);

        let results = engine.search_ranked(&query).unwrap();

        assert_eq!(results.chunks.len(), 3);
        assert_eq!(results.total_count, 10);
        assert!(results.has_more);
    }

    #[test]
    fn test_search_ranked_min_score_filter() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = MemoryStorage::new(db).unwrap();

        let engine = SearchEngineBuilder::new(storage).min_score(50.0).build();

        // Store chunks with very different relevance
        let chunk_relevant = MemoryChunk::new(
            "agent-001".to_string(),
            "rust rust rust rust rust".to_string(),
        )
        .with_created_at(now_ms());
        let chunk_less_relevant = MemoryChunk::new(
            "agent-001".to_string(),
            "a very long text that mentions rust only once among many other words about programming languages and technology".to_string(),
        )
        .with_created_at(now_ms());

        engine.storage().store_chunk(&chunk_relevant).unwrap();
        engine.storage().store_chunk(&chunk_less_relevant).unwrap();

        let query = MemorySearchQuery::new("agent-001".to_string())
            .with_query("rust".to_string())
            .with_mode(SearchMode::Keyword);

        let results = engine.search_ranked(&query).unwrap();

        // Only high-scoring chunk should be returned
        for chunk in &results.chunks {
            assert!(chunk.score >= 50.0);
        }
    }

    #[test]
    fn test_score_breakdown() {
        let engine = create_test_engine();

        let chunk = MemoryChunk::new(
            "agent-001".to_string(),
            "Rust programming is great".to_string(),
        )
        .with_tags(vec!["rust".to_string()])
        .with_created_at(now_ms());

        engine.storage().store_chunk(&chunk).unwrap();

        let query = MemorySearchQuery::new("agent-001".to_string())
            .with_query("rust".to_string())
            .with_mode(SearchMode::Keyword)
            .with_tags(vec!["rust".to_string()]);

        let results = engine.search_ranked(&query).unwrap();

        assert_eq!(results.chunks.len(), 1);
        let scored = &results.chunks[0];

        // Check that breakdown components are present
        assert!(scored.score_breakdown.frequency_score > 0.0);
        assert!(scored.score_breakdown.recency_score > 0.0);
        assert!((scored.score_breakdown.tag_score - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_no_query_all_chunks_scored() {
        let engine = create_test_engine();

        let chunk1 = MemoryChunk::new("agent-001".to_string(), "Content 1".to_string())
            .with_created_at(now_ms());
        let chunk2 = MemoryChunk::new("agent-001".to_string(), "Content 2".to_string())
            .with_created_at(now_ms() - 1000);

        engine.storage().store_chunk(&chunk1).unwrap();
        engine.storage().store_chunk(&chunk2).unwrap();

        // Search without query text - should return all, scored by recency
        let query = MemorySearchQuery::new("agent-001".to_string());
        let results = engine.search_ranked(&query).unwrap();

        assert_eq!(results.chunks.len(), 2);
        // All should have frequency_score = 50 (neutral)
        for chunk in &results.chunks {
            assert!((chunk.score_breakdown.frequency_score - 50.0).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn test_case_insensitive_tag_matching() {
        let engine = create_test_engine();

        let chunk = MemoryChunk::new("agent-001".to_string(), "Test content".to_string())
            .with_tags(vec!["RUST".to_string(), "Async".to_string()])
            .with_created_at(now_ms());

        engine.storage().store_chunk(&chunk).unwrap();

        let query_tags = vec!["rust".to_string(), "ASYNC".to_string()];
        let score = engine.calculate_tag_score(&chunk.tags, &query_tags);

        assert!((score - 100.0).abs() < f64::EPSILON); // Case-insensitive match
    }
}
