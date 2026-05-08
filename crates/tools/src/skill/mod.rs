//! Skill system - types re-exported from ai, implementations here.

pub mod loader;
pub mod tool;

// Re-export skill types from ai
pub use types::skill::{SkillContent, SkillInfo, SkillProvider, SkillSource};
