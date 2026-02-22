//! Minimal cross-platform OS sandbox for RestFlow command execution.
//!
//! Provides kernel-level file system and network restrictions.
//!
//! # Usage
//!
//! The sandbox works in two phases:
//! 1. **Command wrapping** (`wrap_command`): On macOS, replaces the program
//!    with `sandbox-exec`. On other platforms, returns the original program/args.
//! 2. **Pre-exec hooks** (`pre_exec_hook`): On Linux, sets up Landlock and
//!    seccomp in the child process. No-op on other platforms.

pub mod error;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

use std::path::PathBuf;

pub use error::SandboxError;

/// Policy controlling what the sandboxed process may access.
#[derive(Debug, Clone)]
pub enum SandboxPolicy {
    /// No restrictions (backward-compatible default).
    None,

    /// File system is read-only everywhere; network is denied.
    ReadOnly,

    /// Specified directories are writable, everything else is read-only;
    /// network is denied.
    WriteDir {
        /// Directories the child process may write to.
        writable_dirs: Vec<PathBuf>,
    },
}

/// Wrap a command's program and arguments for sandbox enforcement.
///
/// - **macOS**: Returns `("/usr/bin/sandbox-exec", ["-p", profile, "--", program, args...])`.
/// - **Other platforms**: Returns the original program and args unchanged.
///
/// After calling this, also call [`pre_exec_hook`] inside a `pre_exec` closure
/// on Linux for full enforcement.
pub fn wrap_command(
    policy: &SandboxPolicy,
    program: &str,
    args: &[&str],
) -> Result<(String, Vec<String>), SandboxError> {
    if matches!(policy, SandboxPolicy::None) {
        let args_owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        return Ok((program.to_string(), args_owned));
    }

    platform_wrap_command(policy, program, args)
}

/// Run sandbox setup inside a `pre_exec` closure (after fork, before exec).
///
/// - **Linux**: Sets `PR_SET_NO_NEW_PRIVS`, installs Landlock rules and seccomp BPF.
/// - **Other platforms**: No-op.
///
/// # Safety
/// This must only be called inside a `pre_exec` closure (async-signal-safe context).
pub fn pre_exec_hook(policy: &SandboxPolicy) -> Result<(), SandboxError> {
    if matches!(policy, SandboxPolicy::None) {
        return Ok(());
    }

    platform_pre_exec_hook(policy)
}

// ─── Platform dispatch ──────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn platform_wrap_command(
    policy: &SandboxPolicy,
    program: &str,
    args: &[&str],
) -> Result<(String, Vec<String>), SandboxError> {
    macos::wrap_command_macos(policy, program, args)
}

#[cfg(not(target_os = "macos"))]
fn platform_wrap_command(
    _policy: &SandboxPolicy,
    program: &str,
    args: &[&str],
) -> Result<(String, Vec<String>), SandboxError> {
    let args_owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    Ok((program.to_string(), args_owned))
}

#[cfg(target_os = "linux")]
fn platform_pre_exec_hook(policy: &SandboxPolicy) -> Result<(), SandboxError> {
    linux::pre_exec_hook_linux(policy)
}

#[cfg(not(target_os = "linux"))]
fn platform_pre_exec_hook(_policy: &SandboxPolicy) -> Result<(), SandboxError> {
    Ok(())
}
