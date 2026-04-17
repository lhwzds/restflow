use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const ARTIFACT_FILE_NAME: &str = "artifact.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Skill,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecretMode {
    Runtime,
    Embedded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BuildProfile {
    Debug,
    Release,
}

impl BuildProfile {
    pub fn as_cargo_flag(&self) -> Option<&'static str> {
        match self {
            Self::Debug => None,
            Self::Release => Some("--release"),
        }
    }

    pub fn as_dir_name(&self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Release => "release",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillArtifactMetadata {
    pub kind: ArtifactKind,
    pub id: String,
    pub name: String,
    pub version: String,
    pub build_system: String,
    pub toolchain: String,
    pub target: String,
    pub entry_binary: String,
    pub secret_mode: SecretMode,
}

pub fn read_skill_artifact_metadata(skill_dir: &Path) -> Result<SkillArtifactMetadata> {
    let path = skill_dir.join(ARTIFACT_FILE_NAME);
    let content = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn write_skill_artifact_metadata(
    skill_dir: &Path,
    metadata: &SkillArtifactMetadata,
) -> Result<()> {
    let path = skill_dir.join(ARTIFACT_FILE_NAME);
    std::fs::write(path, serde_json::to_string_pretty(metadata)?)?;
    Ok(())
}
