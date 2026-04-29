//! # codocia
//!
//! Store owns backend-neutral repository contracts.
//!
//! ## Owns
//! - Store trait
//! - get/put/delete repository contract
//! - backend abstraction boundary
//!
//! ## Must Not
//! - own business decisions
//! - expose backend handles to runtime modules
//! - store UI overlay state
//!
//! ## Inputs
//! - record IDs
//! - typed records
//!
//! ## Outputs
//! - persisted records
//! - deletion status
//!
//! ## Used By
//! - auth
//! - chat
//! - run
//!
//! ## Verify
//! - cargo check -p store

use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait Store<T>: Send + Sync
where
    T: Send + Sync,
{
    async fn get(&self, id: &str) -> Result<Option<T>>;
    async fn put(&self, id: &str, value: T) -> Result<()>;
    async fn delete(&self, id: &str) -> Result<bool>;
}
