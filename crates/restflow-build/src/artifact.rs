use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

pub const ARTIFACT_FILE_NAME: &str = "artifact.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Skill,
    SkillBinary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SecretMode {
    #[default]
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

fn default_schema_version() -> u32 {
    1
}

fn default_build_system() -> String {
    "rust".to_string()
}

fn default_protocol_transport() -> String {
    "stdio-json".to_string()
}

fn default_protocol_single_json_value() -> String {
    "single-json-value".to_string()
}

fn default_target() -> String {
    format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillArtifactProtocol {
    #[serde(default = "default_protocol_transport")]
    pub transport: String,
    #[serde(default = "default_protocol_single_json_value")]
    pub input: String,
    #[serde(default = "default_protocol_single_json_value")]
    pub output: String,
}

impl Default for SkillArtifactProtocol {
    fn default() -> Self {
        Self {
            transport: default_protocol_transport(),
            input: default_protocol_single_json_value(),
            output: default_protocol_single_json_value(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillArtifactDownload {
    pub repo: String,
    pub tag: String,
    pub asset: String,
    #[serde(default)]
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillArtifactMetadata {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub kind: ArtifactKind,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default = "default_build_system")]
    pub build_system: String,
    #[serde(default)]
    pub toolchain: String,
    #[serde(default = "default_target")]
    pub target: String,
    pub entry_binary: String,
    #[serde(default)]
    pub secret_mode: SecretMode,
    #[serde(default)]
    pub protocol: SkillArtifactProtocol,
    #[serde(default)]
    pub download: Option<SkillArtifactDownload>,
}

pub fn read_skill_artifact_metadata(skill_dir: &Path) -> Result<SkillArtifactMetadata> {
    let path = skill_dir.join(ARTIFACT_FILE_NAME);
    let content = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn resolve_skill_binary_entry_path(
    skill_dir: &Path,
    metadata: &SkillArtifactMetadata,
) -> Result<PathBuf> {
    if !matches!(&metadata.kind, ArtifactKind::SkillBinary) {
        bail!("artifact kind must be skill_binary");
    }

    let folder_id = skill_dir
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow::anyhow!("invalid skill folder name"))?;
    if metadata.id != folder_id {
        bail!(
            "artifact id '{}' does not match skill folder '{}'",
            metadata.id,
            folder_id
        );
    }

    let raw = Path::new(&metadata.entry_binary);
    if raw.is_absolute() {
        bail!("entry_binary must be relative: {}", metadata.entry_binary);
    }
    if raw
        .components()
        .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        bail!(
            "entry_binary must not contain traversal or root components: {}",
            metadata.entry_binary
        );
    }

    let candidate = skill_dir.join(raw);
    let metadata = std::fs::symlink_metadata(&candidate)
        .with_context(|| format!("failed to stat {}", candidate.display()))?;
    if metadata.file_type().is_symlink() {
        bail!(
            "entry_binary must not be a symlink: {}",
            candidate.display()
        );
    }
    if !metadata.is_file() {
        bail!("entry_binary must point to a file: {}", candidate.display());
    }

    let canonical_skill_dir = std::fs::canonicalize(skill_dir)
        .with_context(|| format!("failed to canonicalize {}", skill_dir.display()))?;
    let canonical_candidate = std::fs::canonicalize(&candidate)
        .with_context(|| format!("failed to canonicalize {}", candidate.display()))?;
    if !canonical_candidate.starts_with(&canonical_skill_dir) {
        bail!(
            "entry_binary resolves outside skill folder: {}",
            canonical_candidate.display()
        );
    }

    Ok(canonical_candidate)
}

pub fn write_skill_artifact_metadata(
    skill_dir: &Path,
    metadata: &SkillArtifactMetadata,
) -> Result<()> {
    let path = skill_dir.join(ARTIFACT_FILE_NAME);
    std::fs::write(path, serde_json::to_string_pretty(metadata)?)?;
    Ok(())
}

pub fn list_installed_skill_artifacts(
    root_dir: &Path,
) -> Result<Vec<(PathBuf, SkillArtifactMetadata)>> {
    if !root_dir.exists() {
        return Ok(Vec::new());
    }

    let mut artifacts = Vec::new();
    for entry in std::fs::read_dir(root_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let artifact_path = path.join(ARTIFACT_FILE_NAME);
        if !artifact_path.exists() {
            continue;
        }
        let metadata = read_skill_artifact_metadata(&path)?;
        if matches!(&metadata.kind, ArtifactKind::SkillBinary) {
            resolve_skill_binary_entry_path(&path, &metadata)?;
        }
        artifacts.push((path, metadata));
    }
    artifacts.sort_by(|left, right| left.1.id.cmp(&right.1.id));
    Ok(artifacts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_published_skill_binary_artifact_shape() {
        let metadata: SkillArtifactMetadata = serde_json::from_str(
            r#"{
                "schema_version": 1,
                "kind": "skill_binary",
                "id": "regex-finder",
                "name": "Regex Finder",
                "version": "0.1.2",
                "target": "aarch64-macos",
                "entry_binary": "regex-finder",
                "protocol": {
                    "transport": "stdio-json",
                    "input": "single-json-value",
                    "output": "single-json-value"
                },
                "download": {
                    "repo": "lhwzds/restflow-skills",
                    "tag": "regex-finder@0.1.2",
                    "asset": "regex-finder-aarch64-macos.tar.gz"
                }
            }"#,
        )
        .expect("metadata should parse");

        assert_eq!(metadata.kind, ArtifactKind::SkillBinary);
        assert_eq!(metadata.id, "regex-finder");
        assert_eq!(metadata.build_system, "rust");
        assert_eq!(metadata.secret_mode, SecretMode::Runtime);
        assert_eq!(metadata.protocol.transport, "stdio-json");
        assert_eq!(
            metadata.download.expect("download metadata").asset,
            "regex-finder-aarch64-macos.tar.gz"
        );
    }

    #[test]
    fn resolves_binary_entry_inside_matching_skill_dir() {
        let temp = tempfile::tempdir().expect("tempdir");
        let skill_dir = temp.path().join("regex-finder");
        std::fs::create_dir_all(&skill_dir).expect("skill dir");
        std::fs::write(skill_dir.join("regex-finder"), "#!/bin/sh\n").expect("binary");
        let metadata: SkillArtifactMetadata = serde_json::from_str(
            r#"{
                "kind": "skill_binary",
                "id": "regex-finder",
                "name": "Regex Finder",
                "version": "0.1.2",
                "entry_binary": "regex-finder"
            }"#,
        )
        .expect("metadata");

        let resolved =
            resolve_skill_binary_entry_path(&skill_dir, &metadata).expect("resolve entry");

        assert_eq!(
            resolved,
            std::fs::canonicalize(skill_dir.join("regex-finder")).expect("canonical")
        );
    }

    #[test]
    fn rejects_binary_entry_traversal() {
        let temp = tempfile::tempdir().expect("tempdir");
        let skill_dir = temp.path().join("regex-finder");
        std::fs::create_dir_all(&skill_dir).expect("skill dir");
        let metadata: SkillArtifactMetadata = serde_json::from_str(
            r#"{
                "kind": "skill_binary",
                "id": "regex-finder",
                "name": "Regex Finder",
                "version": "0.1.2",
                "entry_binary": "../tool"
            }"#,
        )
        .expect("metadata");

        let err = resolve_skill_binary_entry_path(&skill_dir, &metadata).unwrap_err();

        assert!(err.to_string().contains("traversal"));
    }

    #[test]
    fn rejects_binary_artifact_id_folder_mismatch() {
        let temp = tempfile::tempdir().expect("tempdir");
        let skill_dir = temp.path().join("regex-finder");
        std::fs::create_dir_all(&skill_dir).expect("skill dir");
        std::fs::write(skill_dir.join("regex-finder"), "#!/bin/sh\n").expect("binary");
        let metadata: SkillArtifactMetadata = serde_json::from_str(
            r#"{
                "kind": "skill_binary",
                "id": "other",
                "name": "Other",
                "version": "0.1.2",
                "entry_binary": "regex-finder"
            }"#,
        )
        .expect("metadata");

        let err = resolve_skill_binary_entry_path(&skill_dir, &metadata).unwrap_err();

        assert!(err.to_string().contains("does not match"));
    }
}
