use anyhow::{Result, bail};
use restflow_build::{
    BuildBinaryOptions, BuildProfile, CreateSkillProjectOptions, RunBinaryOptions,
    build_skill_binary, create_skill_project, run_skill_binary,
};

use crate::cli::{BinaryCommands, BinarySkillCommands, OutputFormat};
use crate::output::json::print_json;

pub async fn run(command: BinaryCommands, format: OutputFormat) -> Result<()> {
    match command {
        BinaryCommands::Skill { command } => run_skill(command, format).await,
    }
}

async fn run_skill(command: BinarySkillCommands, format: OutputFormat) -> Result<()> {
    match command {
        BinarySkillCommands::New {
            id,
            name,
            toolchain,
        } => {
            let result = create_skill_project(&CreateSkillProjectOptions {
                id,
                name,
                toolchain,
            })?;
            if format.is_json() {
                return print_json(&serde_json::json!({
                    "skill_dir": result.skill_dir,
                    "artifact_path": result.artifact_path,
                    "manifest_path": result.manifest_path,
                    "source_path": result.source_path,
                    "skill_markdown_path": result.skill_markdown_path,
                }));
            }

            println!(
                "Created skill binary project at {}",
                result.skill_dir.display()
            );
            Ok(())
        }
        BinarySkillCommands::Build {
            id,
            release,
            toolchain,
        } => {
            let result = build_skill_binary(&BuildBinaryOptions {
                skill_id: id.clone(),
                profile: if release {
                    BuildProfile::Release
                } else {
                    BuildProfile::Debug
                },
                toolchain_override: toolchain,
            })?;
            if format.is_json() {
                return print_json(&serde_json::json!({
                    "success": result.success,
                    "binary_path": result.binary_path,
                    "profile": result.profile,
                    "toolchain": result.toolchain,
                    "stdout": result.stdout,
                    "stderr": result.stderr,
                    "exit_code": result.exit_code,
                }));
            }
            if !result.success {
                bail!(
                    "build failed for {} (exit code {}):\n{}",
                    id,
                    result.exit_code,
                    result.stderr
                );
            }
            println!("Built skill binary: {}", result.binary_path.display());
            Ok(())
        }
        BinarySkillCommands::Run {
            id,
            input,
            input_file,
        } => {
            let stdin_json = match (input, input_file) {
                (Some(_), Some(_)) => bail!("use either --input or --input-file, not both"),
                (Some(value), None) => Some(value),
                (None, Some(path)) => Some(std::fs::read_to_string(path)?),
                (None, None) => None,
            };

            let result = run_skill_binary(&RunBinaryOptions {
                skill_id: id,
                stdin_json,
            })?;
            if format.is_json() {
                return print_json(&serde_json::json!({
                    "success": result.success,
                    "binary_path": result.binary_path,
                    "stdout": result.stdout,
                    "stderr": result.stderr,
                    "exit_code": result.exit_code,
                }));
            }

            if !result.stdout.is_empty() {
                println!("{}", result.stdout.trim_end());
            }
            if !result.success {
                bail!(
                    "skill binary failed (exit code {}):\n{}",
                    result.exit_code,
                    result.stderr
                );
            }
            Ok(())
        }
    }
}
