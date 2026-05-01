//! # codocia
//!
//! CLI owns the minimal executable skill command loop.
//!
//! ## Owns
//! - skill artifact scaffolding command
//! - skill artifact build command
//! - skill artifact run command
//! - local skill install and list commands
//!
//! ## Must Not
//! - start the legacy daemon
//! - render TUI state
//! - own runtime skill execution
//! - own Python package behavior
//!
//! ## Inputs
//! - command line arguments
//! - JSON input strings
//! - local artifact directories
//!
//! ## Outputs
//! - human-readable command status
//! - JSON skill outputs
//!
//! ## Depends On
//! - runtime
//!
//! ## Verify
//! - cargo test -p cli

use anyhow::{Context, Result, bail};
use runtime::{
    ArtifactKind, BuildOptions, InstallOptions, RunOptions, ScaffoldOptions, build_skill,
    default_skills_dir, install_local_skill, list_installed_skills, run_skill, scaffold_skill,
};
use serde_json::Value;
use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

fn main() {
    if let Err(error) = run(env::args().skip(1).collect()) {
        eprintln!("Error: {error:#}");
        std::process::exit(1);
    }
}

fn run(args: Vec<String>) -> Result<()> {
    let command = parse(args)?;
    match command {
        Command::Help => {
            print_usage();
            Ok(())
        }
        Command::Skill(SkillCommand::New(options)) => {
            let artifact = scaffold_skill(&options.dir, options.scaffold)?;
            println!(
                "Created {} skill `{}` at {}",
                kind_label(&artifact.kind),
                artifact.id,
                options.dir.display()
            );
            Ok(())
        }
        Command::Skill(SkillCommand::Build(options)) => {
            let artifact = build_skill(&options.dir, &options.build)?;
            println!("Built skill `{}` at {}", artifact.id, options.dir.display());
            Ok(())
        }
        Command::Skill(SkillCommand::Run(options)) => {
            let output = run_skill(&options.dir, options.input, &options.run)?;
            println!("{}", serde_json::to_string_pretty(&output.value)?);
            Ok(())
        }
        Command::Skill(SkillCommand::InstallLocal(options)) => {
            let artifact = install_local_skill(&options.source, &options.install)?;
            println!(
                "Installed skill `{}` into {}",
                artifact.id,
                options.install.root.display()
            );
            Ok(())
        }
        Command::Skill(SkillCommand::List(options)) => {
            for artifact in list_installed_skills(&options.root)? {
                println!(
                    "{}\t{}\t{}\t{}",
                    artifact.id,
                    kind_label(&artifact.kind),
                    artifact.version,
                    artifact.name
                );
            }
            Ok(())
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Command {
    Help,
    Skill(SkillCommand),
}

#[derive(Debug, Clone, PartialEq)]
enum SkillCommand {
    New(NewCommand),
    Build(BuildCommand),
    Run(RunCommand),
    InstallLocal(InstallLocalCommand),
    List(ListCommand),
}

#[derive(Debug, Clone, PartialEq)]
struct NewCommand {
    dir: PathBuf,
    scaffold: ScaffoldOptions,
}

#[derive(Debug, Clone, PartialEq)]
struct BuildCommand {
    dir: PathBuf,
    build: BuildOptions,
}

#[derive(Debug, Clone, PartialEq)]
struct RunCommand {
    dir: PathBuf,
    input: Value,
    run: RunOptions,
}

#[derive(Debug, Clone, PartialEq)]
struct InstallLocalCommand {
    source: PathBuf,
    install: InstallOptions,
}

#[derive(Debug, Clone, PartialEq)]
struct ListCommand {
    root: PathBuf,
}

fn parse(args: Vec<String>) -> Result<Command> {
    if args.is_empty() || matches!(args[0].as_str(), "-h" | "--help" | "help") {
        return Ok(Command::Help);
    }
    let mut cursor = Cursor::new(args);
    match cursor.next_required("command")?.as_str() {
        "skill" => parse_skill(cursor).map(Command::Skill),
        other => bail!("unknown command `{other}`"),
    }
}

fn parse_skill(mut cursor: Cursor) -> Result<SkillCommand> {
    match cursor.next_required("skill command")?.as_str() {
        "new" => parse_skill_new(cursor).map(SkillCommand::New),
        "build" => parse_skill_build(cursor).map(SkillCommand::Build),
        "run" => parse_skill_run(cursor).map(SkillCommand::Run),
        "install-local" => parse_skill_install_local(cursor).map(SkillCommand::InstallLocal),
        "list" => parse_skill_list(cursor).map(SkillCommand::List),
        other => bail!("unknown skill command `{other}`"),
    }
}

fn parse_skill_new(mut cursor: Cursor) -> Result<NewCommand> {
    let mut kind = None;
    let mut id = None;
    let mut name = None;
    let mut version = "0.1.0".to_string();
    let mut dir = None;

    while let Some(arg) = cursor.next() {
        match arg.as_str() {
            "--kind" => kind = Some(parse_kind(&cursor.next_required("--kind value")?)?),
            "--id" => id = Some(cursor.next_required("--id value")?),
            "--name" => name = Some(cursor.next_required("--name value")?),
            "--version" => version = cursor.next_required("--version value")?,
            value if value.starts_with('-') => bail!("unknown option `{value}`"),
            value => dir = Some(PathBuf::from(value)),
        }
    }

    let kind = kind.ok_or_else(|| anyhow::anyhow!("missing --kind"))?;
    let id = id.ok_or_else(|| anyhow::anyhow!("missing --id"))?;
    let name = name.unwrap_or_else(|| title_from_id(&id));
    let dir = dir.ok_or_else(|| anyhow::anyhow!("missing skill directory"))?;
    let scaffold = match kind {
        ArtifactKind::RustBinary => ScaffoldOptions::rust_binary(id, name, version),
        ArtifactKind::PythonUv => ScaffoldOptions::python_uv(id, name, version),
    };

    Ok(NewCommand { dir, scaffold })
}

fn parse_skill_build(mut cursor: Cursor) -> Result<BuildCommand> {
    let mut build = BuildOptions::default();
    let mut dir = None;

    while let Some(arg) = cursor.next() {
        match arg.as_str() {
            "--profile" => build.profile = cursor.next_required("--profile value")?,
            "--target-dir" => {
                build.target_dir = Some(PathBuf::from(cursor.next_required("--target-dir value")?))
            }
            "--cargo" => build.cargo = OsString::from(cursor.next_required("--cargo value")?),
            "--uv" => build.uv = OsString::from(cursor.next_required("--uv value")?),
            value if value.starts_with('-') => bail!("unknown option `{value}`"),
            value => dir = Some(PathBuf::from(value)),
        }
    }

    Ok(BuildCommand {
        dir: dir.ok_or_else(|| anyhow::anyhow!("missing skill directory"))?,
        build,
    })
}

fn parse_skill_run(mut cursor: Cursor) -> Result<RunCommand> {
    let mut run = RunOptions::default();
    let mut input = serde_json::json!({});
    let mut dir = None;

    while let Some(arg) = cursor.next() {
        match arg.as_str() {
            "--input" => {
                input = serde_json::from_str(&cursor.next_required("--input value")?)
                    .context("parse --input JSON")?;
                if !input.is_object() {
                    bail!("--input must be a JSON object");
                }
            }
            "--timeout" => {
                let seconds = cursor
                    .next_required("--timeout value")?
                    .parse::<u64>()
                    .context("parse --timeout seconds")?;
                run.timeout = Duration::from_secs(seconds);
            }
            "--uv" => run.uv = OsString::from(cursor.next_required("--uv value")?),
            value if value.starts_with('-') => bail!("unknown option `{value}`"),
            value => dir = Some(PathBuf::from(value)),
        }
    }

    Ok(RunCommand {
        dir: dir.ok_or_else(|| anyhow::anyhow!("missing skill directory"))?,
        input,
        run,
    })
}

fn parse_skill_install_local(mut cursor: Cursor) -> Result<InstallLocalCommand> {
    let mut root = None;
    let mut skill_id = None;
    let mut overwrite = false;
    let mut source = None;

    while let Some(arg) = cursor.next() {
        match arg.as_str() {
            "--root" => root = Some(PathBuf::from(cursor.next_required("--root value")?)),
            "--id" => skill_id = Some(cursor.next_required("--id value")?),
            "--overwrite" => overwrite = true,
            value if value.starts_with('-') => bail!("unknown option `{value}`"),
            value => source = Some(PathBuf::from(value)),
        }
    }

    let root = root.unwrap_or(default_skills_dir()?);
    let mut install = InstallOptions::new(root).with_overwrite(overwrite);
    if let Some(skill_id) = skill_id {
        install = install.with_skill_id(skill_id);
    }

    Ok(InstallLocalCommand {
        source: source.ok_or_else(|| anyhow::anyhow!("missing source skill directory"))?,
        install,
    })
}

fn parse_skill_list(mut cursor: Cursor) -> Result<ListCommand> {
    let mut root = None;
    while let Some(arg) = cursor.next() {
        match arg.as_str() {
            "--root" => root = Some(PathBuf::from(cursor.next_required("--root value")?)),
            value => bail!("unknown argument `{value}`"),
        }
    }
    Ok(ListCommand {
        root: root.unwrap_or(default_skills_dir()?),
    })
}

fn parse_kind(value: &str) -> Result<ArtifactKind> {
    match value {
        "rust_binary" => Ok(ArtifactKind::RustBinary),
        "python_uv" => Ok(ArtifactKind::PythonUv),
        other => bail!("unknown skill kind `{other}`"),
    }
}

fn title_from_id(id: &str) -> String {
    id.split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn kind_label(kind: &ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::RustBinary => "rust_binary",
        ArtifactKind::PythonUv => "python_uv",
    }
}

#[derive(Debug, Clone)]
struct Cursor {
    args: Vec<String>,
    index: usize,
}

impl Cursor {
    fn new(args: Vec<String>) -> Self {
        Self { args, index: 0 }
    }

    fn next(&mut self) -> Option<String> {
        let value = self.args.get(self.index).cloned()?;
        self.index += 1;
        Some(value)
    }

    fn next_required(&mut self, label: &str) -> Result<String> {
        self.next()
            .ok_or_else(|| anyhow::anyhow!("missing {label}"))
    }
}

fn print_usage() {
    println!(
        r#"Usage:
  restflow skill new --kind rust_binary|python_uv --id <id> [--name <name>] [--version <version>] <dir>
  restflow skill build [--profile release] [--target-dir <dir>] <dir>
  restflow skill run [--input <json-object>] [--timeout <seconds>] <dir>
  restflow skill install-local [--root <dir>] [--id <id>] [--overwrite] <dir>
  restflow skill list [--root <dir>]
"#
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_skill_new_command() {
        let command = parse(vec![
            "skill".to_string(),
            "new".to_string(),
            "--kind".to_string(),
            "rust_binary".to_string(),
            "--id".to_string(),
            "regex-finder".to_string(),
            "skills/regex-finder".to_string(),
        ])
        .unwrap();

        let Command::Skill(SkillCommand::New(command)) = command else {
            panic!("expected skill new command");
        };
        assert_eq!(command.dir, PathBuf::from("skills/regex-finder"));
        assert_eq!(command.scaffold.id, "regex-finder");
        assert_eq!(command.scaffold.name, "Regex Finder");
        assert_eq!(command.scaffold.kind, ArtifactKind::RustBinary);
    }

    #[test]
    fn parses_skill_run_input() {
        let command = parse(vec![
            "skill".to_string(),
            "run".to_string(),
            "--input".to_string(),
            r#"{"ok":true}"#.to_string(),
            "skills/echo".to_string(),
        ])
        .unwrap();

        let Command::Skill(SkillCommand::Run(command)) = command else {
            panic!("expected skill run command");
        };
        assert_eq!(command.input, serde_json::json!({ "ok": true }));
    }
}
