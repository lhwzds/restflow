//! # codocia
//!
//! Tool owns the callable tool contract and registry.
//!
//! ## Owns
//! - Tool trait
//! - Registry
//! - tool lookup
//! - tool call boundary
//!
//! ## Must Not
//! - decide per-turn permissions alone
//! - own agent loop state
//! - write durable run state
//!
//! ## Inputs
//! - JSON tool input
//! - registered tool implementations
//!
//! ## Outputs
//! - JSON tool output
//! - tool registry names
//!
//! ## Used By
//! - agent
//! - skill
//!
//! ## Verify
//! - cargo check -p tool

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    async fn call(&self, input: Value) -> Result<Value>;
}

#[derive(Default, Clone)]
pub struct Registry {
    tools: BTreeMap<String, Arc<dyn Tool>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert<T>(&mut self, tool: T)
    where
        T: Tool + 'static,
    {
        self.tools.insert(tool.name().to_string(), Arc::new(tool));
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }
}
