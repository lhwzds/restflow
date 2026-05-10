//! Tool registry service for creating tool registries with storage access.
//!
//! Adapter implementations live in [`super::adapters`]. This module provides
//! the [`create_tool_registry`] function that wires adapters into tools.

use crate::services::adapters::*;
use crate::storage::ConfigStorage;
use crate::tools::ToolRegistryBuilder;
use crate::{AgentDefaults, SystemConfig};
use std::sync::Arc;
use tracing::warn;
use types::tool::SecurityGate;
use types::toolset::ToolRegistry;

const DEFAULT_SECURITY_AGENT_ID: &str = "unknown-agent";
const DEFAULT_SECURITY_TASK_ID: &str = "tool-registry";

mod assembly;
mod config;

use self::config::load_agent_defaults;

pub use self::assembly::{create_tool_registry, create_tool_registry_with_assessor};
