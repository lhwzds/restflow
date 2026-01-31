//! Chat streaming module for real-time AI response streaming.
//!
//! This module provides infrastructure for streaming chat responses from LLM providers
//! to the frontend, enabling a real-time typing experience.

pub mod events;
pub mod stream;

pub use events::*;
pub use stream::*;
