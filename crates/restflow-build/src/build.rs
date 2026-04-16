use crate::artifact::{BuildProfile, read_skill_artifact_metadata, write_skill_artifact_metadata};
use crate::scaffold::skill_dir_for;
use crate::toolchain::ensure_toolchain;
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

const BUILD_ONLINE_ENV: &str = "RESTFLOW_BUILD_ONLINE";

#[derive(Debug, Clone)]
pub struct BuildBinaryOptions {
    pub skill_id: String,
    pub profile: BuildProfile,
    pub toolchain_override: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BuildBinaryResult {
    pub success: bool,
    pub binary_path: PathBuf,
    pub profile: BuildProfile,
    pub toolchain: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub fn build_skill_binary(options: &BuildBinaryOptions) -> Result<BuildBinaryResult> {
    let skill_dir = skill_dir_for(&options.skill_id)?;
    if !skill_dir.exists() {
        bail!("skill directory does not exist: {}", skill_dir.display());
    }

    let mut metadata = read_skill_artifact_metadata(&skill_dir)?;
    let toolchain = ensure_toolchain(options.toolchain_override.as_deref())?;
    let target_dir = skill_dir.join("target");

    let mut command = toolchain.build_command();
    command
        .current_dir(&skill_dir)
        .arg("build")
        .arg("--manifest-path")
        .arg(skill_dir.join("Cargo.toml"));
    if !should_build_online() {
        command.arg("--offline");
    }
    if let Some(flag) = options.profile.as_cargo_flag() {
        command.arg(flag);
    }
    command.arg("--target-dir").arg(&target_dir);

    let output = command.output().context("failed to execute cargo build")?;
    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let source_binary = target_dir
        .join(options.profile.as_dir_name())
        .join(binary_file_name(&resolve_binary_name(
            &skill_dir,
            &options.skill_id,
        )?));
    if !output.status.success() {
        return Ok(BuildBinaryResult {
            success: false,
            binary_path: source_binary,
            profile: options.profile.clone(),
            toolchain: toolchain.toolchain_id().to_string(),
            stdout,
            stderr,
            exit_code,
        });
    }

    let binary_name = resolve_binary_name(&skill_dir, &options.skill_id)?;
    let output_binary = skill_dir
        .join("bin")
        .join(options.profile.as_dir_name())
        .join(binary_file_name(&binary_name));
    std::fs::create_dir_all(
        output_binary
            .parent()
            .expect("binary output parent should exist"),
    )?;
    copy_binary(&source_binary, &output_binary)?;

    metadata.toolchain = toolchain.toolchain_id().to_string();
    metadata.entry_binary = pathdiff(&output_binary, &skill_dir);
    write_skill_artifact_metadata(&skill_dir, &metadata)?;

    Ok(BuildBinaryResult {
        success: true,
        binary_path: output_binary,
        profile: options.profile.clone(),
        toolchain: toolchain.toolchain_id().to_string(),
        stdout,
        stderr,
        exit_code,
    })
}

fn should_build_online() -> bool {
    matches!(
        std::env::var(BUILD_ONLINE_ENV).ok().as_deref(),
        Some("1" | "true" | "yes")
    )
}

fn resolve_binary_name(skill_dir: &Path, fallback: &str) -> Result<String> {
    let cargo_toml = std::fs::read_to_string(skill_dir.join("Cargo.toml"))?;
    let value: toml::Value = toml::from_str(&cargo_toml)?;
    Ok(value
        .get("package")
        .and_then(|pkg| pkg.get("name"))
        .and_then(toml::Value::as_str)
        .unwrap_or(fallback)
        .to_string())
}

fn copy_binary(source: &Path, dest: &Path) -> Result<()> {
    std::fs::copy(source, dest).with_context(|| {
        format!(
            "failed to copy built binary from {} to {}",
            source.display(),
            dest.display()
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(dest)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(dest, perms)?;
    }
    Ok(())
}

fn pathdiff(path: &Path, base: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn binary_file_name(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scaffold::{CreateSkillProjectOptions, create_skill_project};

    #[test]
    fn build_skill_binary_creates_runnable_output() {
        let _lock = crate::test_env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        unsafe { std::env::set_var("RESTFLOW_DIR", temp.path()) };
        unsafe {
            std::env::set_var(
                "RESTFLOW_BUILD_CARGO",
                std::env::var("CARGO").expect("cargo env"),
            )
        };

        let scaffold = create_skill_project(&CreateSkillProjectOptions {
            id: "echo-skill".to_string(),
            name: None,
            toolchain: None,
            cargo_toml: None,
            main_rs: None,
            skill_markdown: None,
        })
        .expect("scaffold skill");
        let result = build_skill_binary(&BuildBinaryOptions {
            skill_id: "echo-skill".to_string(),
            profile: BuildProfile::Debug,
            toolchain_override: None,
        })
        .expect("build skill");

        assert!(result.success);
        assert!(result.binary_path.exists());
        let metadata =
            read_skill_artifact_metadata(&scaffold.skill_dir).expect("artifact metadata");
        assert!(metadata.entry_binary.contains("bin/debug/echo-skill"));

        unsafe { std::env::remove_var("RESTFLOW_BUILD_CARGO") };
        unsafe { std::env::remove_var("RESTFLOW_DIR") };
    }
}
