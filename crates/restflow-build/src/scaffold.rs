use crate::artifact::{
    ArtifactKind, BuildProfile, SecretMode, SkillArtifactMetadata, write_skill_artifact_metadata,
};
use crate::toolchain::DEFAULT_TOOLCHAIN_ID;
use anyhow::{Result, anyhow, bail};
use restflow_storage::paths::ensure_restflow_dir;
use std::path::PathBuf;

pub const SKILL_FILE_NAME: &str = "SKILL.md";

#[derive(Debug, Clone)]
pub struct CreateSkillProjectOptions {
    pub id: String,
    pub name: Option<String>,
    pub toolchain: Option<String>,
    pub cargo_toml: Option<String>,
    pub main_rs: Option<String>,
    pub skill_markdown: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CreateSkillProjectResult {
    pub skill_dir: PathBuf,
    pub artifact_path: PathBuf,
    pub manifest_path: PathBuf,
    pub source_path: PathBuf,
    pub skill_markdown_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ReadSkillProjectResult {
    pub skill_dir: PathBuf,
    pub artifact_path: PathBuf,
    pub manifest_path: PathBuf,
    pub source_path: PathBuf,
    pub skill_markdown_path: PathBuf,
    pub cargo_toml: String,
    pub main_rs: String,
    pub skill_markdown: String,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateSkillProjectOptions {
    pub id: String,
    pub cargo_toml: Option<String>,
    pub main_rs: Option<String>,
    pub skill_markdown: Option<String>,
}

pub fn skill_root_dir() -> Result<PathBuf> {
    Ok(ensure_restflow_dir()?.join("skills"))
}

pub fn skill_dir_for(id: &str) -> Result<PathBuf> {
    let id = id.trim();
    if id.is_empty() {
        bail!("skill id cannot be empty");
    }
    Ok(skill_root_dir()?.join(id))
}

pub fn create_skill_project(
    options: &CreateSkillProjectOptions,
) -> Result<CreateSkillProjectResult> {
    let skill_id = options.id.trim();
    if skill_id.is_empty() {
        bail!("skill id cannot be empty");
    }

    let skill_dir = skill_dir_for(skill_id)?;
    if skill_dir.exists() {
        bail!("skill directory already exists: {}", skill_dir.display());
    }

    let name = options
        .name
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| skill_id.to_string());
    let toolchain = options
        .toolchain
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_TOOLCHAIN_ID.to_string());

    std::fs::create_dir_all(skill_dir.join("src"))?;
    std::fs::create_dir_all(
        skill_dir
            .join("bin")
            .join(BuildProfile::Debug.as_dir_name()),
    )?;

    let cargo_toml = options
        .cargo_toml
        .clone()
        .unwrap_or(render_cargo_toml(skill_id, &name)?);
    let source = options
        .main_rs
        .clone()
        .unwrap_or_else(|| render_main_rs(skill_id));
    let markdown = options
        .skill_markdown
        .clone()
        .unwrap_or_else(|| render_skill_markdown(skill_id, &name));
    let metadata = SkillArtifactMetadata {
        kind: ArtifactKind::Skill,
        id: skill_id.to_string(),
        name: name.clone(),
        version: "0.1.0".to_string(),
        build_system: "rust".to_string(),
        toolchain: toolchain.clone(),
        target: format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
        entry_binary: format!("bin/{}/{}", BuildProfile::Debug.as_dir_name(), skill_id),
        secret_mode: SecretMode::Runtime,
    };

    let manifest_path = skill_dir.join("Cargo.toml");
    let source_path = skill_dir.join("src").join("main.rs");
    let skill_markdown_path = skill_dir.join(SKILL_FILE_NAME);
    std::fs::write(&manifest_path, cargo_toml)?;
    std::fs::write(&source_path, source)?;
    std::fs::write(&skill_markdown_path, markdown)?;
    write_skill_artifact_metadata(&skill_dir, &metadata)?;

    Ok(CreateSkillProjectResult {
        artifact_path: skill_dir.join(crate::artifact::ARTIFACT_FILE_NAME),
        manifest_path,
        source_path,
        skill_markdown_path,
        skill_dir,
    })
}

pub fn read_skill_project(id: &str) -> Result<ReadSkillProjectResult> {
    let skill_dir = skill_dir_for(id)?;
    if !skill_dir.exists() {
        bail!("skill directory does not exist: {}", skill_dir.display());
    }

    let artifact_path = skill_dir.join(crate::artifact::ARTIFACT_FILE_NAME);
    let manifest_path = skill_dir.join("Cargo.toml");
    let source_path = skill_dir.join("src").join("main.rs");
    let skill_markdown_path = skill_dir.join(SKILL_FILE_NAME);

    Ok(ReadSkillProjectResult {
        cargo_toml: std::fs::read_to_string(&manifest_path)?,
        main_rs: std::fs::read_to_string(&source_path)?,
        skill_markdown: std::fs::read_to_string(&skill_markdown_path)?,
        skill_dir,
        artifact_path,
        manifest_path,
        source_path,
        skill_markdown_path,
    })
}

pub fn update_skill_project(options: &UpdateSkillProjectOptions) -> Result<ReadSkillProjectResult> {
    let skill_id = options.id.trim();
    if skill_id.is_empty() {
        bail!("skill id cannot be empty");
    }

    let skill_dir = skill_dir_for(skill_id)?;
    if !skill_dir.exists() {
        bail!("skill directory does not exist: {}", skill_dir.display());
    }

    if let Some(cargo_toml) = &options.cargo_toml {
        std::fs::write(skill_dir.join("Cargo.toml"), cargo_toml)?;
    }
    if let Some(main_rs) = &options.main_rs {
        std::fs::write(skill_dir.join("src").join("main.rs"), main_rs)?;
    }
    if let Some(skill_markdown) = &options.skill_markdown {
        std::fs::write(skill_dir.join(SKILL_FILE_NAME), skill_markdown)?;
    }

    read_skill_project(skill_id)
}

fn render_cargo_toml(skill_id: &str, name: &str) -> Result<String> {
    let package_name = skill_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    if package_name.is_empty() {
        return Err(anyhow!("invalid skill id"));
    }

    Ok(format!(
        r#"[package]
name = "{package_name}"
version = "0.1.0"
edition = "2024"
description = "{name}"

[dependencies]
serde_json = "1"
"#
    ))
}

fn render_main_rs(skill_id: &str) -> String {
    format!(
        r#"use std::io::Read;

fn main() {{
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .expect("read stdin");

    let parsed = if input.trim().is_empty() {{
        serde_json::json!({{}})
    }} else {{
        serde_json::from_str::<serde_json::Value>(&input).expect("parse json stdin")
    }};

    let output = serde_json::json!({{
        "skill_id": "{skill_id}",
        "ok": true,
        "input": parsed
    }});

    println!("{{}}", serde_json::to_string_pretty(&output).expect("serialize output"));
}}
"#
    )
}

fn render_skill_markdown(skill_id: &str, name: &str) -> String {
    format!(
        r#"---
id: {skill_id}
name: {name}
kind: skill_binary
---

# {name}

This skill binary is under active development.

## Input

Reads JSON from stdin.

## Output

Writes JSON to stdout.
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::read_skill_artifact_metadata;

    #[test]
    fn create_skill_project_writes_expected_layout() {
        let _lock = crate::test_env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        unsafe { std::env::set_var("RESTFLOW_DIR", temp.path()) };

        let result = create_skill_project(&CreateSkillProjectOptions {
            id: "pdf-reader".to_string(),
            name: Some("PDF Reader".to_string()),
            toolchain: None,
            cargo_toml: None,
            main_rs: None,
            skill_markdown: None,
        })
        .expect("create skill project");

        assert!(result.skill_dir.exists());
        assert!(result.artifact_path.exists());
        assert!(result.manifest_path.exists());
        assert!(result.source_path.exists());
        assert!(result.skill_markdown_path.exists());

        let metadata = read_skill_artifact_metadata(&result.skill_dir).expect("artifact metadata");
        assert_eq!(metadata.id, "pdf-reader");
        assert_eq!(metadata.kind, ArtifactKind::Skill);
        assert_eq!(metadata.toolchain, DEFAULT_TOOLCHAIN_ID);

        unsafe { std::env::remove_var("RESTFLOW_DIR") };
    }

    #[test]
    fn create_skill_project_accepts_custom_files() {
        let _lock = crate::test_env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        unsafe { std::env::set_var("RESTFLOW_DIR", temp.path()) };

        let result = create_skill_project(&CreateSkillProjectOptions {
            id: "pdf-reader".to_string(),
            name: Some("PDF Reader".to_string()),
            toolchain: None,
            cargo_toml: Some(
                r#"[package]
name = "pdf-reader"
version = "0.1.0"
edition = "2024"

[dependencies]
lopdf = "0.35"
serde_json = "1"
"#
                .to_string(),
            ),
            main_rs: Some("fn main() { println!(\"{\\\"ok\\\":true}\"); }\n".to_string()),
            skill_markdown: Some("# PDF Reader\n".to_string()),
        })
        .expect("create customized skill project");

        let cargo = std::fs::read_to_string(&result.manifest_path).expect("read Cargo.toml");
        let main = std::fs::read_to_string(&result.source_path).expect("read main.rs");
        let skill_md =
            std::fs::read_to_string(&result.skill_markdown_path).expect("read skill markdown");

        assert!(cargo.contains("lopdf"));
        assert!(main.contains("println!"));
        assert_eq!(skill_md, "# PDF Reader\n");

        unsafe { std::env::remove_var("RESTFLOW_DIR") };
    }

    #[test]
    fn read_and_update_skill_project_round_trip() {
        let _lock = crate::test_env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        unsafe { std::env::set_var("RESTFLOW_DIR", temp.path()) };

        create_skill_project(&CreateSkillProjectOptions {
            id: "fixable-skill".to_string(),
            name: None,
            toolchain: None,
            cargo_toml: None,
            main_rs: None,
            skill_markdown: None,
        })
        .expect("create skill project");

        let before = read_skill_project("fixable-skill").expect("read project");
        assert!(before.cargo_toml.contains("serde_json"));

        let after = update_skill_project(&UpdateSkillProjectOptions {
            id: "fixable-skill".to_string(),
            cargo_toml: Some("[package]\nname = \"fixable-skill\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nserde = { version = \"1\", features = [\"derive\"] }\nserde_json = \"1\"\n".to_string()),
            main_rs: Some("fn main() { println!(\"{\\\"ok\\\":true}\"); }\n".to_string()),
            skill_markdown: Some("# Fixed Skill\n".to_string()),
        })
        .expect("update project");

        assert!(after.cargo_toml.contains("serde ="));
        assert!(after.main_rs.contains("println!"));
        assert_eq!(after.skill_markdown, "# Fixed Skill\n");

        unsafe { std::env::remove_var("RESTFLOW_DIR") };
    }
}
