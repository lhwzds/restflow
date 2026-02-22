//! Skill system - types re-exported from restflow-ai, implementations here.

pub mod loader;
pub mod tool;

// Re-export skill types from restflow-ai
pub use restflow_traits::skill::{
    SkillContent, SkillInfo, SkillProvider, SkillRecord, SkillUpdate,
};
