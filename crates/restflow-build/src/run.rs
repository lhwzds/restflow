use crate::artifact::read_skill_artifact_metadata;
use crate::scaffold::skill_dir_for;
use anyhow::{Context, Result, bail};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Default)]
pub struct RunBinaryOptions {
    pub skill_id: String,
    pub stdin_json: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RunBinaryResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub binary_path: PathBuf,
}

pub fn run_skill_binary(options: &RunBinaryOptions) -> Result<RunBinaryResult> {
    let skill_dir = skill_dir_for(&options.skill_id)?;
    let metadata = read_skill_artifact_metadata(&skill_dir)?;
    let binary_path = skill_dir.join(&metadata.entry_binary);
    if !binary_path.exists() {
        bail!(
            "compiled binary not found at {}. Run `restflow binary skill build {}` first.",
            binary_path.display(),
            options.skill_id
        );
    }

    let mut child = Command::new(&binary_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn {}", binary_path.display()))?;

    if let Some(stdin_json) = &options.stdin_json
        && let Some(stdin) = child.stdin.as_mut()
    {
        stdin.write_all(stdin_json.as_bytes())?;
    }

    let output = child.wait_with_output()?;
    Ok(RunBinaryResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
        binary_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::BuildProfile;
    use crate::build::{BuildBinaryOptions, build_skill_binary};
    use crate::scaffold::{CreateSkillProjectOptions, create_skill_project};

    #[test]
    fn run_skill_binary_executes_generated_program() {
        let _lock = crate::test_env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        unsafe { std::env::set_var("RESTFLOW_DIR", temp.path()) };
        unsafe {
            std::env::set_var(
                "RESTFLOW_BUILD_CARGO",
                std::env::var("CARGO").expect("cargo env"),
            )
        };

        create_skill_project(&CreateSkillProjectOptions {
            id: "runner-skill".to_string(),
            name: None,
            toolchain: None,
            cargo_toml: None,
            main_rs: None,
            skill_markdown: None,
        })
        .expect("scaffold skill");
        let build = build_skill_binary(&BuildBinaryOptions {
            skill_id: "runner-skill".to_string(),
            profile: BuildProfile::Debug,
            toolchain_override: None,
        })
        .expect("build skill");
        assert!(build.success);

        let result = run_skill_binary(&RunBinaryOptions {
            skill_id: "runner-skill".to_string(),
            stdin_json: Some("{\"hello\":\"world\"}".to_string()),
        })
        .expect("run skill");
        assert!(result.success);
        assert!(result.stdout.contains("\"skill_id\": \"runner-skill\""));
        assert!(result.stdout.contains("\"hello\": \"world\""));

        unsafe { std::env::remove_var("RESTFLOW_BUILD_CARGO") };
        unsafe { std::env::remove_var("RESTFLOW_DIR") };
    }
}
