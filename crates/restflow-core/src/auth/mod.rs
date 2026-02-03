//! Authentication Profile Management
//!
//! This module provides unified credential management for RestFlow with:
//! - Automatic credential discovery from various sources
//! - Profile storage and rotation
//! - Health tracking and cooldown management
//! - Secure storage for manual profiles

pub mod discoverer;
pub mod manager;
pub mod refresh;
pub mod resolver;
pub mod types;
pub mod writer;

pub use discoverer::*;
pub use manager::*;
pub use refresh::*;
pub use resolver::*;
pub use types::*;
pub use writer::*;
