use anyhow::{Context, Result, anyhow, bail};
use restflow_storage::paths::ensure_restflow_dir;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const DEFAULT_TOOLCHAIN_ID: &str = "1.85.0";

const BUILD_CARGO_ENV: &str = "RESTFLOW_BUILD_CARGO";
const BUILD_RUSTUP_ENV: &str = "RESTFLOW_BUILD_RUSTUP";
const BUILD_SKIP_INSTALL_ENV: &str = "RESTFLOW_BUILD_SKIP_INSTALL";

#[derive(Debug, Clone)]
pub enum ToolchainInvocation {
    DirectCargo {
        cargo_path: PathBuf,
        toolchain_id: String,
    },
    Rustup {
        rustup_path: PathBuf,
        toolchain_id: String,
    },
}

impl ToolchainInvocation {
    pub fn toolchain_id(&self) -> &str {
        match self {
            Self::DirectCargo { toolchain_id, .. } | Self::Rustup { toolchain_id, .. } => {
                toolchain_id.as_str()
            }
        }
    }

    pub fn build_command(&self) -> Command {
        match self {
            Self::DirectCargo { cargo_path, .. } => Command::new(cargo_path),
            Self::Rustup {
                rustup_path,
                toolchain_id,
            } => {
                let mut command = Command::new(rustup_path);
                command.arg("run").arg(toolchain_id).arg("cargo");
                command
            }
        }
    }
}

fn toolchains_root() -> Result<PathBuf> {
    Ok(ensure_restflow_dir()?.join("toolchains").join("rust"))
}

fn toolchain_marker_path(toolchain_id: &str) -> Result<PathBuf> {
    Ok(toolchains_root()?.join(toolchain_id).join("installed"))
}

fn build_cargo_override() -> Option<PathBuf> {
    std::env::var(BUILD_CARGO_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn rustup_path() -> PathBuf {
    std::env::var(BUILD_RUSTUP_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("rustup"))
}

fn should_skip_install() -> bool {
    matches!(
        std::env::var(BUILD_SKIP_INSTALL_ENV).ok().as_deref(),
        Some("1" | "true" | "yes")
    )
}

fn rustup_has_toolchain(rustup: &Path, toolchain_id: &str) -> Result<bool> {
    let output = Command::new(rustup)
        .arg("toolchain")
        .arg("list")
        .output()
        .with_context(|| format!("failed to run `{}`", rustup.display()))?;
    if !output.status.success() {
        bail!(
            "rustup toolchain list failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .any(|line| line.trim_start().starts_with(toolchain_id)))
}

fn install_toolchain(rustup: &Path, toolchain_id: &str) -> Result<()> {
    let status = Command::new(rustup)
        .arg("toolchain")
        .arg("install")
        .arg(toolchain_id)
        .arg("--profile")
        .arg("minimal")
        .status()
        .with_context(|| format!("failed to run `{}`", rustup.display()))?;
    if !status.success() {
        bail!("rustup toolchain install {toolchain_id} failed");
    }
    Ok(())
}

pub fn ensure_toolchain(requested: Option<&str>) -> Result<ToolchainInvocation> {
    let toolchain_id = requested.unwrap_or(DEFAULT_TOOLCHAIN_ID).trim();
    if toolchain_id.is_empty() {
        bail!("toolchain id cannot be empty");
    }

    if let Some(cargo_path) = build_cargo_override() {
        return Ok(ToolchainInvocation::DirectCargo {
            cargo_path,
            toolchain_id: toolchain_id.to_string(),
        });
    }

    let rustup = rustup_path();
    let marker = toolchain_marker_path(toolchain_id)?;
    if marker.exists() {
        return Ok(ToolchainInvocation::Rustup {
            rustup_path: rustup,
            toolchain_id: toolchain_id.to_string(),
        });
    }

    std::fs::create_dir_all(
        marker
            .parent()
            .ok_or_else(|| anyhow!("invalid toolchain marker path"))?,
    )?;

    if !rustup_has_toolchain(&rustup, toolchain_id)? {
        if should_skip_install() {
            bail!("toolchain `{toolchain_id}` is not available and install is disabled");
        }
        install_toolchain(&rustup, toolchain_id)?;
    }

    std::fs::write(&marker, b"installed")?;

    Ok(ToolchainInvocation::Rustup {
        rustup_path: rustup,
        toolchain_id: toolchain_id.to_string(),
    })
}
