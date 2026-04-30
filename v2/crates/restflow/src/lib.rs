//! # codocia
//!
//! Restflow is the compatibility facade for the V2 crates.
//!
//! ## Owns
//! - public module re-exports
//! - stable import shape for examples
//!
//! ## Must Not
//! - own runtime behavior
//! - own persistence
//! - duplicate module logic
//!
//! ## Inputs
//! - V2 crates
//!
//! ## Outputs
//! - unified Rust API surface
//!
//! ## Depends On
//! - agent
//! - auth
//! - bridge
//! - chat
//! - engine
//! - event
//! - model
//! - proto
//! - run
//! - server
//! - skill
//! - store
//! - tool
//!
//! ## Verify
//! - cargo check -p restflow-v2

pub use agent;
pub use auth;
pub use bridge;
pub use chat;
pub use engine::{Core, CoreStores};
pub use event;
pub use model;
pub use proto::{CoreCommand, CoreResponse, CoreSnapshot};
pub use run;
pub use server;
pub use skill;
pub use store;
pub use tool;
