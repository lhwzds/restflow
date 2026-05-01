//! Windows sandbox (no-op placeholder).

use std::process::Command;

use crate::SandboxError;
use crate::SandboxPolicy;

pub(crate) fn apply_windows_sandbox(
    _cmd: &mut Command,
    _policy: &SandboxPolicy,
) -> Result<(), SandboxError> {
    tracing::warn!("OS-level sandbox not implemented on Windows, running without isolation");
    Ok(())
}
