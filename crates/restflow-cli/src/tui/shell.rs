use anyhow::Result;
use std::path::PathBuf;
use tokio::process::Command;

pub struct ShellOutput {
    pub stdout: String,
    pub stderr: String,
    pub status: Option<i32>,
}

pub async fn run_shell_command(command: &str) -> Result<ShellOutput> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .output()
        .await?;

    Ok(ShellOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        status: output.status.code(),
    })
}
