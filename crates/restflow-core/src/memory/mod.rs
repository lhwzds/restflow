//! Memory management modules for agent memory storage.
//!
//! This module provides utilities for managing agent memory, including:
//! - Text chunking for efficient storage
//! - Search engine with relevance scoring for memory retrieval
//! - Markdown export for human-readable output (TODO)
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Memory Module                             │
//! │                                                              │
//! │  ┌────────────────┐  ┌────────────────┐  ┌────────────────┐ │
//! │  │   TextChunker  │  │  SearchEngine  │  │    Exporter    │ │
//! │  │    (B3) ✓      │  │    (B4) ✓      │  │     (B5)       │ │
//! │  └────────────────┘  └────────────────┘  └────────────────┘ │
//! │          │                   │                   │          │
//! │          └───────────────────┼───────────────────┘          │
//! │                              │                               │
//! │                    ┌─────────▼─────────┐                    │
//! │                    │   MemoryStorage   │                    │
//! │                    │      (B2)         │                    │
//! │                    └───────────────────┘                    │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Search Engine
//!
//! The [`SearchEngine`] provides relevance-scored search with:
//!
//! - **Frequency scoring**: Higher scores for content with more keyword matches
//! - **Recency scoring**: More recent memories get higher scores
//! - **Tag matching**: Bonus for matching tags
//! - **Configurable weights**: Customize the balance between factors
//!
//! ## Example
//!
//! ```ignore
//! use restflow_core::memory::{SearchEngine, SearchEngineBuilder};
//!
//! // Create with default config (60% frequency, 30% recency, 10% tags)
//! let engine = SearchEngine::new(storage);
//!
//! // Or customize with builder
//! let engine = SearchEngineBuilder::new(storage)
//!     .frequency_focused()  // 80% frequency, 10% recency
//!     .min_score(10.0)      // Filter low-relevance results
//!     .build();
//!
//! // Search and get scored results
//! let results = engine.search_ranked(&query)?;
//! for result in results.chunks {
//!     println!("Score: {:.1} | {}", result.score, result.chunk.content);
//! }
//! ```

mod chunker;
mod export;
mod mirror;
mod search;
mod unified_search;

pub use chunker::{TextChunker, TextChunkerBuilder};
pub use export::{ExportOptions, ExportResult, MemoryExporter, MemoryExporterBuilder};
pub use mirror::{ChatSessionMirror, MessageMirror, NoopMirror};
pub use search::{
    RankedSearchResult, ScoreBreakdown, ScoredChunk, SearchConfig, SearchEngine,
    SearchEngineBuilder,
};
pub use unified_search::{
    SearchResultSource, SourceCounts, UnifiedSearchConfig, UnifiedSearchEngine,
    UnifiedSearchResult, UnifiedSearchResults,
};
