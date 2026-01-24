//! Shell command execution Tauri commands

use serde::Serialize;
use tokio::process::Command;

/// Shell command execution result
#[derive(Debug, Serialize)]
pub struct ShellOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Execute a shell command and return the output
#[tauri::command]
pub async fn execute_shell(command: String, cwd: Option<String>) -> Result<ShellOutput, String> {
    let working_dir = cwd.unwrap_or_else(|| ".".to_string());

    let output = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/C", &command])
            .current_dir(&working_dir)
            .output()
            .await
    } else {
        Command::new("sh")
            .args(["-c", &command])
            .current_dir(&working_dir)
            .output()
            .await
    };

    match output {
        Ok(out) => Ok(ShellOutput {
            stdout: String::from_utf8_lossy(&out.stdout).to_string(),
            stderr: String::from_utf8_lossy(&out.stderr).to_string(),
            exit_code: out.status.code().unwrap_or(-1),
        }),
        Err(e) => Err(format!("Failed to execute command: {}", e)),
    }
}
