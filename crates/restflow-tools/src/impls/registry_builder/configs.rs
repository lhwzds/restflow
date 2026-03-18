use std::path::PathBuf;
use std::sync::Arc;

use crate::impls::file_tracker::FileTracker;
use crate::impls::secrets::SecretGetPolicy;
use crate::impls::{BashTool, FileTool};
use crate::security::bash_security::BashSecurityConfig;

/// Configuration for bash tool security.
#[derive(Debug, Clone)]
pub struct BashConfig {
    /// Working directory for commands.
    pub working_dir: Option<String>,
    /// Command timeout in seconds.
    pub timeout_secs: u64,
    /// Blocked commands (security).
    pub blocked_commands: Vec<String>,
    /// Whether to allow sudo.
    pub allow_sudo: bool,
    /// Maximum total bytes for stdout/stderr output payload.
    pub max_output_bytes: usize,
}

impl Default for BashConfig {
    fn default() -> Self {
        let security = BashSecurityConfig::default();
        Self {
            working_dir: None,
            timeout_secs: 300,
            blocked_commands: security.blocked_commands,
            allow_sudo: security.allow_sudo,
            max_output_bytes: 1_000_000,
        }
    }
}

impl BashConfig {
    /// Convert into a [`BashTool`].
    pub fn into_bash_tool(self) -> BashTool {
        let mut tool = BashTool::new()
            .with_timeout(self.timeout_secs)
            .with_max_output(self.max_output_bytes);
        if let Some(workdir) = self.working_dir {
            tool = tool.with_workdir(workdir);
        }
        tool
    }
}

/// Configuration for file tool.
#[derive(Debug, Clone)]
pub struct FileConfig {
    /// Allowed paths (security).
    pub allowed_paths: Vec<PathBuf>,
    /// Whether write operations are allowed.
    pub allow_write: bool,
    /// Maximum bytes allowed for a single file read.
    pub max_read_bytes: usize,
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            allowed_paths: Vec::new(),
            allow_write: true,
            max_read_bytes: 1_000_000,
        }
    }
}

impl FileConfig {
    pub fn for_workspace_root(workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            allowed_paths: vec![workspace_root.into()],
            ..Self::default()
        }
    }

    /// Convert into a [`FileTool`] with a new internal tracker.
    pub fn into_file_tool(self) -> FileTool {
        let require_base_dir = self.allowed_paths.is_empty();
        let mut tool = FileTool::new().with_max_read(self.max_read_bytes);
        if let Some(base) = self.allowed_paths.into_iter().next() {
            tool = tool.with_base_dir(base);
        } else if require_base_dir {
            tool = tool.require_base_dir();
        }
        tool
    }

    /// Convert into a [`FileTool`] using a shared [`FileTracker`].
    pub fn into_file_tool_with_tracker(self, tracker: Arc<FileTracker>) -> FileTool {
        let require_base_dir = self.allowed_paths.is_empty();
        let mut tool = FileTool::with_tracker(tracker).with_max_read(self.max_read_bytes);
        if let Some(base) = self.allowed_paths.into_iter().next() {
            tool = tool.with_base_dir(base);
        } else if require_base_dir {
            tool = tool.require_base_dir();
        }
        tool
    }
}

/// Configuration for manage_secrets tool behavior.
#[derive(Debug, Clone, Copy)]
pub struct SecretsConfig {
    /// Whether write operations are allowed.
    pub allow_write: bool,
    /// Policy for the `get` operation response payload.
    pub get_policy: SecretGetPolicy,
}

impl Default for SecretsConfig {
    fn default() -> Self {
        Self {
            allow_write: false,
            get_policy: SecretGetPolicy::Open,
        }
    }
}
