//! Skill Registry module for managing skill sources and installation.
//!
//! This module provides the infrastructure for:
//! - Discovering skills from multiple sources (local, builtin, marketplace, GitHub)
//! - Installing and updating skills with dependency resolution
//! - Checking gating requirements before installation

mod provider;
#[allow(clippy::module_inception)]
mod registry;
mod gating;
mod resolver;
mod marketplace;
mod github;

pub use provider::{
    SkillProvider, SkillProviderError, SkillSearchQuery, SkillSearchResult,
    LocalSkillProvider, BuiltinSkillProvider, SkillSortOrder,
};
pub use registry::{SkillRegistry, SkillRegistryConfig};
pub use gating::GatingChecker;
pub use resolver::{DependencyResolver, DependencyError, InstallPlan, InstallAction};
pub use marketplace::{MarketplaceProvider, DEFAULT_MARKETPLACE_URL};
pub use github::GitHubProvider;
