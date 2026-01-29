//! Memory management modules for agent memory storage.
//!
//! This module provides utilities for managing agent memory, including:
//! - Text chunking for efficient storage
//! - Search engine for memory retrieval (TODO)
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
//! │  │    (B3) ✓      │  │     (B4)       │  │     (B5)       │ │
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

mod chunker;

pub use chunker::{TextChunker, TextChunkerBuilder};
