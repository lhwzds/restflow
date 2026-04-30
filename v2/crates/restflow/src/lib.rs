//! # codocia
//!
//! Restflow is the minimal compatibility facade for the V2 engine boundary.
//!
//! ## Owns
//! - stable engine import shape for examples
//! - protocol entrypoint re-exports
//!
//! ## Must Not
//! - own runtime behavior
//! - own persistence
//! - duplicate module logic
//!
//! ## Inputs
//! - engine boundary
//! - protocol types
//!
//! ## Outputs
//! - minimal Rust API surface
//!
//! ## Depends On
//! - engine
//! - proto
//!
//! ## Verify
//! - cargo check -p restflow-v2

pub use engine::{Core, CoreStores};
pub use proto::{CoreCommand, CoreResponse, CoreSnapshot};
