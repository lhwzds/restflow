mod runner;
mod telegram;

use anyhow::Result;
use std::path::PathBuf;

pub use runner::CliTaskRunner;
pub use telegram::{TelegramAgent, TelegramAgentHandle};

pub fn pid_file() -> PathBuf {
    let base = dirs::runtime_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    let path = base.join("restflow-daemon.pid");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    path
}

pub fn read_pid() -> Option<i32> {
    let pid_text = std::fs::read_to_string(pid_file()).ok()?;
    pid_text.trim().parse::<i32>().ok()
}

pub fn is_daemon_running() -> bool {
    if let Some(pid) = read_pid() {
        is_process_running(pid)
    } else {
        false
    }
}

pub fn cleanup_stale_pid() -> Result<()> {
    if let Some(pid) = read_pid() && !is_process_running(pid) {
        let _ = std::fs::remove_file(pid_file());
    }
    Ok(())
}

fn is_process_running(pid: i32) -> bool {
    #[cfg(unix)]
    {
        use nix::sys::signal::kill;
        use nix::unistd::Pid;

        kill(Pid::from_raw(pid), None).is_ok()
    }

    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}
