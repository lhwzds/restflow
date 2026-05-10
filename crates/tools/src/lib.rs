// Unified tool and skill system for RestFlow.
//
// This crate provides:
// - Core tool implementations for local agent execution
// - Security implementations for shell and filesystem operations
//
// Core abstractions (Tool trait, ToolError, ToolRegistry, SecurityGate, etc.)
// are defined in `types` and re-exported here for convenience.

pub mod impls {
    pub(crate) mod path_utils {
        // Shared path resolution and normalization utilities for file-based tools.
        //
        // This module centralizes `normalize_path` and `resolve_path` so that every
        // file-oriented tool (`FileTool`, `EditTool`, `MultiEditTool`, `PatchTool`)
        // uses the same logic for base-directory enforcement and symlink-safe
        // canonicalization.

        use std::path::{Path, PathBuf};

        /// Normalize a path without canonicalizing (for non-existent paths).
        ///
        /// Resolves `.` and `..` components purely lexically, which is useful when
        /// the path (or parts of it) does not yet exist on disk.
        pub(crate) fn normalize_path(path: &Path) -> PathBuf {
            let mut result = PathBuf::new();
            for component in path.components() {
                match component {
                    std::path::Component::ParentDir => {
                        result.pop();
                    }
                    std::path::Component::CurDir => {}
                    c => result.push(c),
                }
            }
            result
        }

        /// Resolve and validate a path against an optional base directory.
        ///
        /// When `base_dir` is `Some`, the resolved path is checked to ensure it does
        /// not escape the base directory.  For paths that already exist on disk the
        /// check uses `canonicalize`; for paths that do not yet exist it falls back to
        /// lexical normalization (via [`normalize_path`]) combined with ancestor
        /// canonicalization when possible.
        ///
        /// When `base_dir` is `None`, absolute paths are accepted as-is and relative
        /// paths are rejected. Callers that require a workspace root can use
        /// [`resolve_path_with_policy`] with `require_base_dir = true`.
        #[cfg(test)]
        pub(crate) fn resolve_path(path: &str, base_dir: Option<&Path>) -> Result<PathBuf, String> {
            resolve_path_with_policy(path, base_dir, false)
        }

        pub(crate) fn resolve_path_with_policy(
            path: &str,
            base_dir: Option<&Path>,
            require_base_dir: bool,
        ) -> Result<PathBuf, String> {
            let path = PathBuf::from(path);

            if let Some(base) = base_dir {
                let resolved = if path.is_absolute() {
                    path
                } else {
                    base.join(&path)
                };

                // Compute canonical base early so every branch shares it.
                let canonical_base = if base.exists() {
                    base.canonicalize().map_err(|e| e.to_string())?
                } else {
                    normalize_path(base)
                };

                if resolved.exists() {
                    let canonical = resolved.canonicalize().map_err(|e| e.to_string())?;
                    if !canonical.starts_with(&canonical_base) {
                        return Err(format!(
                            "Path '{}' escapes allowed base directory '{}'. All file operations must be within this directory.",
                            canonical.display(),
                            canonical_base.display()
                        ));
                    }
                    return Ok(canonical);
                }

                // The resolved path does not exist yet.  If the base itself exists we
                // try to find a real ancestor so that symlinks in existing prefixes are
                // resolved correctly.
                if base.exists() {
                    let Some((ancestor, suffix)) = find_existing_ancestor(&resolved) else {
                        return Err(format!(
                            "Path '{}' escapes allowed base directory '{}'. All file operations must be within this directory.",
                            resolved.display(),
                            canonical_base.display()
                        ));
                    };
                    let canonical_parent = ancestor.canonicalize().map_err(|e| e.to_string())?;
                    let candidate = normalize_path(&canonical_parent.join(suffix));
                    if !candidate.starts_with(&canonical_base) {
                        return Err(format!(
                            "Path '{}' escapes allowed base directory '{}'. All file operations must be within this directory.",
                            candidate.display(),
                            canonical_base.display()
                        ));
                    }
                    return Ok(candidate);
                }

                let normalized = normalize_path(&resolved);
                if !normalized.starts_with(&canonical_base) {
                    return Err(format!(
                        "Path '{}' escapes allowed base directory '{}'. All file operations must be within this directory.",
                        normalized.display(),
                        canonical_base.display()
                    ));
                }

                Ok(normalized)
            } else {
                if require_base_dir {
                    return Err(
                        "This tool requires an explicit workspace root or base directory."
                            .to_string(),
                    );
                }

                // No base directory restriction.
                if path.is_absolute() {
                    Ok(path)
                } else {
                    Err(
                        "Relative paths require an explicit workspace root or base directory."
                            .to_string(),
                    )
                }
            }
        }

        /// Walk up from `path` until we find an existing ancestor directory.
        ///
        /// Returns `(existing_ancestor, remaining_suffix)` so the caller can
        /// canonicalize the ancestor and re-attach the suffix.
        fn find_existing_ancestor(path: &Path) -> Option<(PathBuf, PathBuf)> {
            let mut ancestor = path.to_path_buf();
            loop {
                if ancestor.exists() {
                    let suffix = path
                        .strip_prefix(&ancestor)
                        .unwrap_or_else(|_| Path::new(""))
                        .to_path_buf();
                    return Some((ancestor, suffix));
                }

                if !ancestor.pop() {
                    return None;
                }
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            #[test]
            fn test_resolve_path_requires_base_dir_for_relative_paths() {
                let error = resolve_path("relative.txt", None).unwrap_err();
                assert!(error.contains("Relative paths require an explicit workspace root"));
            }

            #[test]
            fn test_resolve_path_with_policy_requires_base_dir() {
                let error = resolve_path_with_policy("/tmp/file.txt", None, true).unwrap_err();
                assert!(error.contains("explicit workspace root or base directory"));
            }
        }
    }

    pub(crate) mod shared {
        /// Common generated/build directories to skip during recursive traversal.
        pub(crate) const COMMON_SKIP_DIRS: &[&str] = &[
            ".git",
            ".hg",
            ".svn",
            "node_modules",
            "__pycache__",
            ".mypy_cache",
            ".pytest_cache",
            ".tox",
            "target",
            "dist",
            "build",
            ".next",
            ".nuxt",
            ".venv",
            "venv",
        ];

        /// Additional skip directories used by glob traversal.
        const GLOB_EXTRA_SKIP_DIRS: &[&str] = &[".node_modules"];

        /// Returns true when a directory name should be skipped by grep traversal.
        pub(crate) fn should_skip_grep_dir(name: &str) -> bool {
            name.starts_with('.') || COMMON_SKIP_DIRS.contains(&name)
        }

        /// Returns true when a directory name should be skipped by glob traversal.
        pub(crate) fn should_skip_glob_dir(name: &str) -> bool {
            name.starts_with('.')
                || COMMON_SKIP_DIRS.contains(&name)
                || GLOB_EXTRA_SKIP_DIRS.contains(&name)
        }

        /// Check if a file is likely binary based on extension.
        pub(crate) fn is_likely_binary(name: &str) -> bool {
            let binary_extensions = [
                ".exe", ".dll", ".so", ".dylib", ".a", ".o", ".obj", ".png", ".jpg", ".jpeg",
                ".gif", ".bmp", ".ico", ".webp", ".mp3", ".mp4", ".avi", ".mov", ".mkv", ".wav",
                ".flac", ".zip", ".tar", ".gz", ".bz2", ".xz", ".7z", ".rar", ".pdf", ".doc",
                ".docx", ".xls", ".xlsx", ".ppt", ".pptx", ".wasm", ".pyc", ".pyo", ".class",
                ".jar", ".ttf", ".otf", ".woff", ".woff2", ".eot",
            ];

            let lower = name.to_lowercase();
            binary_extensions.iter().any(|ext| lower.ends_with(ext))
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            #[test]
            fn test_should_skip_grep_dir() {
                assert!(should_skip_grep_dir(".git"));
                assert!(should_skip_grep_dir("target"));
                assert!(should_skip_grep_dir(".hidden"));
                assert!(!should_skip_grep_dir("src"));
            }

            #[test]
            fn test_should_skip_glob_dir() {
                assert!(should_skip_glob_dir(".git"));
                assert!(should_skip_glob_dir(".node_modules"));
                assert!(should_skip_glob_dir(".hidden"));
                assert!(!should_skip_glob_dir("src"));
            }

            #[test]
            fn test_is_likely_binary() {
                assert!(is_likely_binary("image.png"));
                assert!(is_likely_binary("archive.zip"));
                assert!(is_likely_binary("video.MP4"));
                assert!(!is_likely_binary("code.rs"));
                assert!(!is_likely_binary("readme.md"));
            }
        }
    }

    pub(crate) mod subagent_read_capability {
        use std::sync::Arc;

        use types::{SubagentCompletion, SubagentManager, SubagentState};

        use crate::{Result, ToolError};

        #[derive(Clone)]
        pub(crate) struct SubagentReadCapabilityService {
            manager: Arc<dyn SubagentManager>,
        }

        impl SubagentReadCapabilityService {
            pub(crate) fn new(manager: Arc<dyn SubagentManager>) -> Self {
                Self { manager }
            }

            pub(crate) fn list_running_for_parent(
                &self,
                parent_run_id: Option<&str>,
            ) -> Vec<SubagentState> {
                let Some(parent_run_id) = parent_run_id
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                else {
                    return Vec::new();
                };

                self.manager.list_running_for_parent(parent_run_id)
            }

            pub(crate) fn running_count_for_parent(&self, parent_run_id: Option<&str>) -> usize {
                self.list_running_for_parent(parent_run_id).len()
            }

            pub(crate) async fn wait_for_parent_owned_task(
                &self,
                task_id: &str,
                parent_run_id: Option<&str>,
            ) -> Result<Option<SubagentCompletion>> {
                let Some(parent_run_id) = parent_run_id
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                else {
                    return Err(ToolError::Tool(
                        "parent_run_id is required for wait_subagents.".to_string(),
                    ));
                };

                Ok(self
                    .manager
                    .wait_for_parent_owned_task(task_id, parent_run_id)
                    .await)
            }
        }
    }

    mod bash {
        // Bash command execution tool for AI agents.

        use async_trait::async_trait;
        use serde::{Deserialize, Serialize};
        use serde_json::Value;
        use std::path::PathBuf;
        use std::process::Stdio;
        use std::sync::Arc;
        use std::time::Instant;
        use tokio::process::Command;
        use tokio::time::{Duration, sleep, timeout};

        #[cfg(unix)]
        use nix::sys::signal::{Signal, killpg};
        #[cfg(unix)]
        use nix::unistd::Pid;

        use crate::Result;
        use crate::SecurityGate;
        use crate::{Tool, ToolErrorCategory, ToolOutput};

        /// Default timeout for command execution in seconds.
        const DEFAULT_TIMEOUT_SECS: u64 = 300;

        /// Maximum output size in bytes (100KB).
        const DEFAULT_MAX_OUTPUT_BYTES: usize = 100_000;

        #[cfg(unix)]
        struct ProcessGroupGuard {
            pgid: Option<Pid>,
        }

        #[cfg(unix)]
        impl ProcessGroupGuard {
            fn new(process_group_id: Option<i32>) -> Self {
                Self {
                    pgid: process_group_id.map(Pid::from_raw),
                }
            }

            fn disarm(&mut self) {
                self.pgid = None;
            }

            async fn terminate(&mut self) {
                let Some(pgid) = self.pgid.take() else {
                    return;
                };
                let _ = killpg(pgid, Signal::SIGTERM);
                sleep(Duration::from_millis(500)).await;
                let _ = killpg(pgid, Signal::SIGKILL);
            }
        }

        #[cfg(unix)]
        impl Drop for ProcessGroupGuard {
            fn drop(&mut self) {
                if let Some(pgid) = self.pgid.take() {
                    let _ = killpg(pgid, Signal::SIGTERM);
                    let _ = killpg(pgid, Signal::SIGKILL);
                }
            }
        }

        /// Bash command execution tool.
        #[derive(Clone)]
        pub struct BashTool {
            default_workdir: Option<String>,
            timeout_secs: u64,
            max_output_bytes: usize,
            security_gate: Option<Arc<dyn SecurityGate>>,
            agent_id: Option<String>,
            task_id: Option<String>,
        }

        impl Default for BashTool {
            fn default() -> Self {
                Self::new()
            }
        }

        impl BashTool {
            pub fn new() -> Self {
                Self {
                    default_workdir: None,
                    timeout_secs: DEFAULT_TIMEOUT_SECS,
                    max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
                    security_gate: None,
                    agent_id: None,
                    task_id: None,
                }
            }

            pub fn with_workdir(mut self, workdir: impl Into<String>) -> Self {
                self.default_workdir = Some(workdir.into());
                self
            }

            pub fn with_timeout(mut self, secs: u64) -> Self {
                self.timeout_secs = secs;
                self
            }

            pub fn with_max_output(mut self, bytes: usize) -> Self {
                self.max_output_bytes = bytes;
                self
            }

            pub fn with_security(
                mut self,
                security_gate: Arc<dyn SecurityGate>,
                agent_id: impl Into<String>,
                task_id: impl Into<String>,
            ) -> Self {
                self.security_gate = Some(security_gate);
                self.agent_id = Some(agent_id.into());
                self.task_id = Some(task_id.into());
                self
            }

            async fn run_command(
                &self,
                command: &str,
                workdir: &str,
                timeout_secs: u64,
            ) -> std::result::Result<(i32, String, String, bool), std::io::Error> {
                let (program, args) = (
                    "sh".to_string(),
                    vec!["-c".to_string(), command.to_string()],
                );

                let mut cmd = Command::new(&program);
                cmd.args(&args)
                    .current_dir(workdir)
                    .kill_on_drop(true)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());

                #[cfg(unix)]
                {
                    cmd.process_group(0);
                }

                let child = cmd.spawn()?;
                #[cfg(unix)]
                let process_group_id = child.id().map(|pid| pid as i32);
                #[cfg(unix)]
                let mut process_group_guard = ProcessGroupGuard::new(process_group_id);

                let output = match timeout(
                    Duration::from_secs(timeout_secs),
                    child.wait_with_output(),
                )
                .await
                {
                    Ok(result) => {
                        #[cfg(unix)]
                        process_group_guard.disarm();
                        result?
                    }
                    Err(_) => {
                        #[cfg(unix)]
                        process_group_guard.terminate().await;

                        return Err(std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            format!("Timeout after {timeout_secs} seconds"),
                        ));
                    }
                };

                let exit_code = output.status.code().unwrap_or(-1);
                let (stdout, stdout_truncated) = self.truncate_output(&output.stdout);
                let (stderr, stderr_truncated) = self.truncate_output(&output.stderr);

                Ok((
                    exit_code,
                    stdout,
                    stderr,
                    stdout_truncated || stderr_truncated,
                ))
            }

            fn truncate_output(&self, bytes: &[u8]) -> (String, bool) {
                let total_len = bytes.len();
                let truncated = total_len > self.max_output_bytes;
                let slice = if truncated {
                    let mut end = self.max_output_bytes;
                    while end > 0 && (bytes[end] & 0xC0) == 0x80 {
                        end -= 1;
                    }
                    &bytes[..end]
                } else {
                    bytes
                };

                let text = String::from_utf8_lossy(slice).to_string();
                if truncated {
                    (
                        format!("{}...\n[Output truncated, {} bytes total]", text, total_len),
                        true,
                    )
                } else {
                    (text, false)
                }
            }

            fn classify_command_failure(stderr: &str) -> (ToolErrorCategory, bool) {
                let normalized = stderr.to_ascii_lowercase();
                let shell_not_found = (normalized.contains("sh:") || normalized.contains("bash:"))
                    && normalized.contains("not found");

                if normalized.contains("command not found")
                    || normalized.contains("no such file or directory")
                    || shell_not_found
                {
                    return (ToolErrorCategory::Config, false);
                }

                if normalized.contains("permission denied")
                    || normalized.contains("operation not permitted")
                    || normalized.contains("unauthorized")
                {
                    return (ToolErrorCategory::Auth, false);
                }

                if normalized.contains("connection refused")
                    || normalized.contains("connection reset")
                    || normalized.contains("timed out")
                    || normalized.contains("timeout")
                    || normalized.contains("temporary failure in name resolution")
                    || normalized.contains("name or service not known")
                    || normalized.contains("network is unreachable")
                    || normalized.contains("no route to host")
                {
                    return (ToolErrorCategory::Network, true);
                }

                (ToolErrorCategory::Execution, false)
            }

            fn resolve_workdir(
                &self,
                workdir: Option<String>,
            ) -> std::result::Result<String, ToolOutput> {
                let explicit = workdir.map(PathBuf::from);
                let default = self.default_workdir.as_deref().map(PathBuf::from);

                let resolved = match (explicit, default) {
                    (Some(path), Some(base)) => {
                        if path.is_absolute() {
                            path
                        } else {
                            base.join(path)
                        }
                    }
                    (Some(path), None) => {
                        if path.is_absolute() {
                            path
                        } else {
                            return Err(ToolOutput::non_retryable_error(
                                "Relative workdir values require an explicit workspace root.",
                                ToolErrorCategory::Config,
                            ));
                        }
                    }
                    (None, Some(base)) => base,
                    (None, None) => {
                        return Err(ToolOutput::non_retryable_error(
                            "This tool requires an explicit workspace root or workdir.",
                            ToolErrorCategory::Config,
                        ));
                    }
                };

                if !resolved.is_absolute() {
                    return Err(ToolOutput::non_retryable_error(
                        "Workdir values must resolve to an absolute workspace path.",
                        ToolErrorCategory::Config,
                    ));
                }

                Ok(resolved.to_string_lossy().into_owned())
            }
        }

        /// Input parameters for bash command execution.
        #[derive(Debug, Deserialize)]
        pub struct BashInput {
            pub command: String,
            #[serde(default)]
            pub workdir: Option<String>,
            #[serde(default)]
            pub timeout: Option<u64>,
            #[serde(default)]
            pub yolo_mode: bool,
        }

        /// Output from bash command execution.
        #[derive(Debug, Serialize, Deserialize)]
        pub struct BashOutput {
            pub exit_code: i32,
            pub stdout: String,
            pub stderr: String,
            pub truncated: bool,
            pub duration_ms: u64,
        }

        #[async_trait]
        impl Tool for BashTool {
            fn name(&self) -> &str {
                "bash"
            }

            fn description(&self) -> &str {
                "Run shell commands in the local environment and return stdout, stderr, and exit status. Use this for command execution; for file content operations, prefer the file tool."
            }

            fn parameters_schema(&self) -> Value {
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The shell command to execute"
                        },
                        "workdir": {
                            "type": "string",
                            "description": "Working directory for command execution"
                        },
                        "timeout": {
                            "type": "integer",
                            "description": "Timeout in seconds (default: 300)"
                        }
                    },
                    "required": ["command"]
                })
            }

            async fn execute(&self, input: Value) -> Result<ToolOutput> {
                let input: BashInput = serde_json::from_value(input)?;

                let workdir = match self.resolve_workdir(input.workdir) {
                    Ok(workdir) => workdir,
                    Err(output) => return Ok(output),
                };

                let timeout_secs = input.timeout.unwrap_or(self.timeout_secs);

                // yolo_mode is an explicit caller opt-out for policy checks.
                // Keep default as gated when a security gate is present.
                if !input.yolo_mode
                    && let Some(security_gate) = &self.security_gate
                {
                    let agent_id = self
                        .agent_id
                        .as_deref()
                        .ok_or_else(|| crate::ToolError::Tool("Missing agent_id".into()))?;
                    let task_id = self
                        .task_id
                        .as_deref()
                        .ok_or_else(|| crate::ToolError::Tool("Missing task_id".into()))?;

                    let decision = security_gate
                        .check_command(&input.command, task_id, agent_id, Some(&workdir))
                        .await?;

                    if !decision.allowed {
                        if decision.requires_approval {
                            return Ok(ToolOutput {
                                success: false,
                                result: serde_json::json!({
                                    "pending_approval": true,
                                    "approval_id": decision.approval_id,
                                }),
                                error: decision.reason,
                                error_category: Some(ToolErrorCategory::Auth),
                                retryable: Some(false),
                                retry_after_ms: None,
                            });
                        }

                        return Ok(ToolOutput {
                            success: false,
                            result: serde_json::json!({
                                "blocked": true,
                            }),
                            error: decision.reason,
                            error_category: Some(ToolErrorCategory::Config),
                            retryable: Some(false),
                            retry_after_ms: None,
                        });
                    }
                }

                let start = Instant::now();

                let result = self
                    .run_command(&input.command, &workdir, timeout_secs)
                    .await;

                let duration_ms = start.elapsed().as_millis() as u64;

                match result {
                    Ok((exit_code, stdout, stderr, truncated)) => {
                        let failure_meta =
                            (exit_code != 0).then(|| Self::classify_command_failure(&stderr));
                        let output = BashOutput {
                            exit_code,
                            stdout,
                            stderr,
                            truncated,
                            duration_ms,
                        };

                        Ok(ToolOutput {
                            success: exit_code == 0,
                            result: serde_json::to_value(&output)?,
                            error: (exit_code != 0)
                                .then(|| format!("Command exited with code {}", exit_code)),
                            error_category: failure_meta
                                .as_ref()
                                .map(|(category, _)| category.clone()),
                            retryable: failure_meta.map(|(_, retryable)| retryable),
                            retry_after_ms: None,
                        })
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::TimedOut => Ok(ToolOutput {
                        success: false,
                        result: serde_json::json!({
                            "error": "Command timed out",
                            "timeout_secs": timeout_secs,
                        }),
                        error: Some(format!("Timeout after {} seconds", timeout_secs)),
                        error_category: Some(ToolErrorCategory::Network),
                        retryable: Some(true),
                        retry_after_ms: None,
                    }),
                    Err(e) => Ok(ToolOutput {
                        success: false,
                        result: serde_json::json!({"error": e.to_string()}),
                        error: Some(e.to_string()),
                        error_category: Some(ToolErrorCategory::Execution),
                        retryable: Some(false),
                        retry_after_ms: None,
                    }),
                }
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            #[test]
            fn test_bash_tool_new() {
                let tool = BashTool::new();
                assert_eq!(tool.timeout_secs, DEFAULT_TIMEOUT_SECS);
                assert_eq!(tool.max_output_bytes, DEFAULT_MAX_OUTPUT_BYTES);
                assert!(tool.default_workdir.is_none());
            }

            #[test]
            fn test_bash_tool_name() {
                let tool = BashTool::new();
                assert_eq!(tool.name(), "bash");
            }

            #[test]
            fn test_classify_command_failure_shell_not_found() {
                let (category, retryable) = BashTool::classify_command_failure(
                    "sh: 1: nonexistent_command_12345: not found",
                );
                assert_eq!(category, ToolErrorCategory::Config);
                assert!(!retryable);
            }

            #[test]
            fn test_truncate_output_no_truncation() {
                let tool = BashTool::new();
                let data = b"hello world";
                let (text, truncated) = tool.truncate_output(data);
                assert_eq!(text, "hello world");
                assert!(!truncated);
            }

            #[test]
            fn test_truncate_output_with_truncation() {
                let tool = BashTool::new().with_max_output(10);
                let data = b"hello world this is a long string";
                let (text, truncated) = tool.truncate_output(data);
                assert!(truncated);
                assert!(text.contains("[Output truncated"));
            }

            #[tokio::test]
            #[cfg(unix)]
            async fn test_bash_tool_execute_simple() {
                let temp = tempfile::tempdir().unwrap();
                let tool = BashTool::new().with_workdir(temp.path().to_string_lossy().into_owned());
                let output = tool
                    .execute(serde_json::json!({
                        "command": "echo hello"
                    }))
                    .await
                    .unwrap();

                assert!(output.success);
                let result: BashOutput = serde_json::from_value(output.result).unwrap();
                assert_eq!(result.exit_code, 0);
                assert!(result.stdout.contains("hello"));
            }

            #[tokio::test]
            #[cfg(unix)]
            async fn test_bash_tool_execute_timeout() {
                let temp = tempfile::tempdir().unwrap();
                let tool = BashTool::new()
                    .with_workdir(temp.path().to_string_lossy().into_owned())
                    .with_timeout(1);
                let output = tool
                    .execute(serde_json::json!({
                        "command": "sleep 10"
                    }))
                    .await
                    .unwrap();

                assert!(!output.success);
                assert!(output.error.as_ref().unwrap().contains("Timeout"));
            }

            #[tokio::test]
            #[cfg(unix)]
            async fn test_bash_tool_abort_kills_process_group_side_effects() {
                let temp = tempfile::tempdir().unwrap();
                let target = temp.path().join("cancel_result.txt");
                let tool = BashTool::new().with_workdir(temp.path().to_string_lossy().into_owned());
                let handle = tokio::spawn(async move {
                    tool.execute(serde_json::json!({
                        "command": "sleep 1; echo SHOULD_NOT_FINISH > cancel_result.txt"
                    }))
                    .await
                });

                sleep(Duration::from_millis(100)).await;
                handle.abort();
                let error = handle.await.expect_err("bash task should be aborted");
                assert!(error.is_cancelled());

                sleep(Duration::from_secs(2)).await;
                assert!(
                    !target.exists(),
                    "aborted bash command must not keep running after cancellation"
                );
            }

            #[tokio::test]
            async fn test_bash_tool_requires_explicit_workdir() {
                let tool = BashTool::new();
                let output = tool
                    .execute(serde_json::json!({
                        "command": "echo hello"
                    }))
                    .await
                    .unwrap();

                assert!(!output.success);
                assert_eq!(output.error_category, Some(ToolErrorCategory::Config));
                assert!(
                    output
                        .error
                        .as_deref()
                        .unwrap_or_default()
                        .contains("workspace root or workdir")
                );
            }

            #[tokio::test]
            async fn test_bash_tool_rejects_relative_workdir_without_workspace_root() {
                let tool = BashTool::new();
                let output = tool
                    .execute(serde_json::json!({
                        "command": "echo hello",
                        "workdir": "subdir"
                    }))
                    .await
                    .unwrap();

                assert!(!output.success);
                assert_eq!(output.error_category, Some(ToolErrorCategory::Config));
                assert!(
                    output
                        .error
                        .as_deref()
                        .unwrap_or_default()
                        .contains("Relative workdir values")
                );
            }
        }
    }

    mod file {
        // File operations tool for AI agents
        //
        // Provides file system operations with:
        // - Read files with line numbers and pagination
        // - Write/append files with auto-creation of parent directories
        // - List directory contents with glob pattern matching
        // - Search files with regex
        // - Delete files
        // - Check file existence
        // - Optional base directory restriction for security
        //
        // # Example
        //
        // ```ignore
        // let tool = FileTool::new();
        // let output = tool.execute(serde_json::json!({
        //     "action": "read",
        //     "path": "/tmp/test.txt"
        // })).await?;
        // ```

        use async_trait::async_trait;
        use futures::StreamExt;
        use futures::stream;
        use regex::Regex;
        use serde::{Deserialize, Serialize};
        use serde_json::Value;
        use std::path::{Path, PathBuf};
        use std::sync::Arc;
        use tokio::fs;
        use tokio::io::AsyncWriteExt;

        use super::file_tracker::FileTracker;
        use super::shared::is_likely_binary;
        use crate::Result;
        use crate::SecurityGate;
        use crate::ToolAction;
        use crate::check_security;
        use crate::{Tool, ToolErrorCategory, ToolOutput};
        use ::types::cache::{AgentCache, CachedSearchResult, SearchMatch as CachedSearchMatch};
        /// Maximum file size to read (1MB)
        const DEFAULT_MAX_READ_BYTES: usize = 1_000_000;

        /// Default number of lines to read
        const DEFAULT_LINE_LIMIT: usize = 2000;

        /// Maximum entries to return in directory listing
        const MAX_LIST_ENTRIES: usize = 1000;

        /// Maximum search matches to return
        const MAX_SEARCH_MATCHES: usize = 100;

        /// Maximum files allowed in a batch read
        const MAX_BATCH_READ_FILES: usize = 20;

        /// Maximum paths allowed in a batch exists check
        const MAX_BATCH_EXISTS_PATHS: usize = 50;

        /// Maximum locations allowed in a batch search
        const MAX_BATCH_SEARCH_LOCATIONS: usize = 10;

        /// Maximum parallel workers for file batch operations
        const BATCH_IO_CONCURRENCY: usize = 8;

        /// Default max lines per file in batch read
        const DEFAULT_BATCH_LINE_LIMIT: usize = 500;

        /// Default max file size per file in batch read
        const DEFAULT_BATCH_MAX_FILE_SIZE: usize = 500_000;

        /// Default max matches for batch search
        const DEFAULT_BATCH_MAX_MATCHES: usize = 100;

        /// Default context lines for batch search
        const DEFAULT_BATCH_CONTEXT_LINES: usize = 2;

        fn default_batch_line_limit() -> usize {
            DEFAULT_BATCH_LINE_LIMIT
        }

        fn default_batch_max_size() -> usize {
            DEFAULT_BATCH_MAX_FILE_SIZE
        }

        fn default_batch_max_matches() -> usize {
            DEFAULT_BATCH_MAX_MATCHES
        }

        fn default_context_lines() -> usize {
            DEFAULT_BATCH_CONTEXT_LINES
        }

        fn default_continue_on_error() -> bool {
            true
        }

        fn file_parameters_schema() -> Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["read", "write", "list", "search", "delete", "exists", "batch_read", "batch_exists", "batch_search"],
                        "description": "The file operation to perform"
                    },
                    "path": {
                        "type": "string",
                        "description": "File or directory path (for single-file operations)"
                    },
                    "paths": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "List of file paths (for batch_read, batch_exists)"
                    },
                    "locations": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "List of directories or globs to search (for batch_search)"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write (for write action)"
                    },
                    "pattern": {
                        "type": "string",
                        "description": "Search pattern (regex for search/batch_search, glob for list)"
                    },
                    "file_pattern": {
                        "type": "string",
                        "description": "Filter files by glob pattern (for search action)"
                    },
                    "append": {
                        "type": "boolean",
                        "description": "Append to file instead of overwrite"
                    },
                    "recursive": {
                        "type": "boolean",
                        "description": "List directories recursively"
                    },
                    "offset": {
                        "type": "integer",
                        "description": "Start reading from this line number (0-indexed)"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum lines to read"
                    },
                    "line_limit": {
                        "type": "integer",
                        "description": "Max lines per file in batch_read (default: 500)"
                    },
                    "max_file_size": {
                        "type": "integer",
                        "description": "Skip files larger than this in batch_read (default: 500KB)"
                    },
                    "max_matches": {
                        "type": "integer",
                        "description": "Max total matches in batch_search (default: 100)"
                    },
                    "context_lines": {
                        "type": "integer",
                        "description": "Context lines before/after matches in batch_search (default: 2)"
                    },
                    "continue_on_error": {
                        "type": "boolean",
                        "description": "Continue batch on individual errors (default: true)"
                    }
                },
                "required": ["action"]
            })
        }

        #[derive(Debug, Deserialize)]
        #[serde(tag = "action", rename_all = "snake_case")]
        pub enum FileAction {
            Read {
                path: String,
                #[serde(default)]
                offset: usize,
                #[serde(default)]
                limit: Option<usize>,
            },
            Write {
                path: String,
                content: String,
                #[serde(default)]
                append: bool,
            },
            List {
                path: String,
                #[serde(default)]
                recursive: bool,
                #[serde(default)]
                pattern: Option<String>,
            },
            Search {
                path: String,
                pattern: String,
                #[serde(default)]
                file_pattern: Option<String>,
            },
            Delete {
                path: String,
            },
            Exists {
                path: String,
            },
            BatchRead {
                paths: Vec<String>,
                #[serde(default = "default_batch_line_limit")]
                line_limit: usize,
                #[serde(default = "default_batch_max_size")]
                max_file_size: usize,
                #[serde(default = "default_continue_on_error")]
                continue_on_error: bool,
            },
            BatchExists {
                paths: Vec<String>,
            },
            BatchSearch {
                pattern: String,
                locations: Vec<String>,
                #[serde(default = "default_batch_max_matches")]
                max_matches: usize,
                #[serde(default = "default_context_lines")]
                context_lines: usize,
            },
        }

        /// File operations tool
        #[derive(Clone)]
        pub struct FileTool {
            /// Base directory for file operations (security boundary)
            base_dir: Option<PathBuf>,
            /// Whether file operations require an explicit base directory.
            require_base_dir: bool,
            /// Maximum file size to read in bytes
            max_read_bytes: usize,
            /// Track file reads/writes for external modification detection
            tracker: Arc<FileTracker>,
            /// Optional cache manager for file/search operations
            cache_manager: Option<Arc<dyn AgentCache>>,
            /// Optional security gate
            security_gate: Option<Arc<dyn SecurityGate>>,
            /// Agent identifier for security checks
            agent_id: Option<String>,
            /// Task identifier for security checks
            task_id: Option<String>,
        }

        impl Default for FileTool {
            fn default() -> Self {
                Self::new()
            }
        }

        impl FileTool {
            /// Create a new FileTool with default settings
            pub fn new() -> Self {
                Self::with_tracker(Arc::new(FileTracker::new()))
            }

            pub fn with_tracker(tracker: Arc<FileTracker>) -> Self {
                Self {
                    base_dir: None,
                    require_base_dir: false,
                    max_read_bytes: DEFAULT_MAX_READ_BYTES,
                    tracker,
                    cache_manager: None,
                    security_gate: None,
                    agent_id: None,
                    task_id: None,
                }
            }

            /// Set base directory for file operations (security boundary)
            /// All paths will be resolved relative to this directory
            pub fn with_base_dir(mut self, base: impl Into<PathBuf>) -> Self {
                self.base_dir = Some(base.into());
                self
            }

            pub fn require_base_dir(mut self) -> Self {
                self.require_base_dir = true;
                self
            }

            /// Set maximum read size in bytes
            pub fn with_max_read(mut self, bytes: usize) -> Self {
                self.max_read_bytes = bytes;
                self
            }

            /// Attach a cache manager for file and search operations
            pub fn with_cache_manager(mut self, cache_manager: Arc<dyn AgentCache>) -> Self {
                self.cache_manager = Some(cache_manager);
                self
            }
            pub fn with_security(
                mut self,
                security_gate: Arc<dyn SecurityGate>,
                agent_id: impl Into<String>,
                task_id: impl Into<String>,
            ) -> Self {
                self.security_gate = Some(security_gate);
                self.agent_id = Some(agent_id.into());
                self.task_id = Some(task_id.into());
                self
            }

            /// Resolve and validate a path against the base directory
            fn resolve_path(&self, path: &str) -> std::result::Result<PathBuf, String> {
                super::path_utils::resolve_path_with_policy(
                    path,
                    self.base_dir.as_deref(),
                    self.require_base_dir,
                )
            }

            /// Read file with line numbers
            async fn read_file(
                &self,
                path: &str,
                offset: usize,
                limit: Option<usize>,
            ) -> ToolOutput {
                let path = match self.resolve_path(path) {
                    Ok(p) => p,
                    Err(e) => return ToolOutput::error(e),
                };

                // Single syscall: get metadata without following symlinks
                let metadata = match std::fs::symlink_metadata(&path) {
                    Ok(m) => m,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        return ToolOutput::error(format!("File not found: {}", path.display()));
                    }
                    Err(e) => return ToolOutput::error(format!("Cannot read metadata: {}", e)),
                };

                if metadata.file_type().is_symlink() {
                    return ToolOutput::error(format!(
                        "Symlinks are not allowed: {}",
                        path.display()
                    ));
                }

                if !metadata.file_type().is_file() {
                    return ToolOutput::error(format!("Not a file: {}", path.display()));
                }

                if metadata.len() as usize > self.max_read_bytes {
                    return ToolOutput::error(format!(
                        "File too large ({} bytes). Maximum: {} bytes. Use offset/limit for partial reads.",
                        metadata.len(),
                        self.max_read_bytes
                    ));
                }

                if let Some(cache) = &self.cache_manager
                    && let Some(content) = cache.get_file(&path, &metadata).await
                {
                    return Self::format_file_output(&path, &content, offset, limit);
                }

                let content = match fs::read_to_string(&path).await {
                    Ok(c) => c,
                    Err(e) => return ToolOutput::error(format!("Cannot read file: {}", e)),
                };

                self.tracker.record_read(&path);
                if let Some(cache) = &self.cache_manager {
                    cache.put_file(&path, content.clone(), &metadata).await;
                }

                Self::format_file_output(&path, &content, offset, limit)
            }

            fn format_file_output(
                path: &Path,
                content: &str,
                offset: usize,
                limit: Option<usize>,
            ) -> ToolOutput {
                let lines: Vec<&str> = content.lines().collect();
                let total_lines = lines.len();

                let line_limit = limit.unwrap_or(DEFAULT_LINE_LIMIT);
                let start = offset.min(total_lines);
                let end = (start + line_limit).min(total_lines);

                let selected: Vec<String> = lines[start..end]
                    .iter()
                    .enumerate()
                    .map(|(i, line)| format!("{:4} | {}", start + i + 1, line))
                    .collect();

                ToolOutput::success(serde_json::json!({
                    "path": path.display().to_string(),
                    "total_lines": total_lines,
                    "showing": format!("{}-{}", start + 1, end),
                    "content": selected.join("\n"),
                }))
            }

            fn format_search_output(
                search_path: &str,
                pattern: &str,
                result: CachedSearchResult,
            ) -> ToolOutput {
                let matches: Vec<Value> = result
                    .matches
                    .iter()
                    .map(|entry| {
                        serde_json::json!({
                            "file": entry.file.clone(),
                            "line": entry.line,
                            "content": entry.content.clone(),
                        })
                    })
                    .collect();

                ToolOutput::success(serde_json::json!({
                    "pattern": pattern,
                    "search_path": search_path,
                    "match_count": matches.len(),
                    "truncated": result.truncated,
                    "total_files_searched": result.total_files_searched,
                    "matches": matches,
                }))
            }

            /// Write or append to a file
            async fn write_file(&self, path: &str, content: &str, append: bool) -> ToolOutput {
                let path = match self.resolve_path(path) {
                    Ok(p) => p,
                    Err(e) => return ToolOutput::error(e),
                };

                if std::fs::symlink_metadata(&path).is_ok() && !self.tracker.has_been_read(&path) {
                    return ToolOutput::error(format!(
                        "You must read {} before writing to it. Read the file first to understand its current content.",
                        path.display()
                    ));
                }

                match self.tracker.check_external_modification(&path).await {
                    Ok(true) => {
                        return ToolOutput::error(format!(
                            "File {} has been modified externally since it was read. Read it again before writing.",
                            path.display()
                        ));
                    }
                    Ok(false) => {}
                    Err(e) => {
                        return ToolOutput::error(format!(
                            "Cannot check file modification time: {}",
                            e
                        ));
                    }
                }

                // Create parent directories if needed
                if let Some(parent) = path.parent()
                    && std::fs::symlink_metadata(parent).is_err()
                    && let Err(e) = fs::create_dir_all(parent).await
                {
                    return ToolOutput::error(format!("Cannot create directory: {}", e));
                }

                let result = if append {
                    let mut file = match fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&path)
                        .await
                    {
                        Ok(f) => f,
                        Err(e) => return ToolOutput::error(format!("Cannot open file: {}", e)),
                    };
                    file.write_all(content.as_bytes()).await
                } else {
                    fs::write(&path, content).await
                };

                match result {
                    Ok(()) => {
                        self.tracker.record_write(&path);

                        if let Some(cache) = &self.cache_manager {
                            cache.invalidate_file(&path).await;
                            let mut current = path.parent();
                            while let Some(directory) = current {
                                cache
                                    .invalidate_search_dir(&directory.to_string_lossy())
                                    .await;
                                current = directory.parent();
                            }
                        }

                        ToolOutput::success(serde_json::json!({
                            "path": path.display().to_string(),
                            "bytes_written": content.len(),
                            "action": if append { "appended" } else { "written" },
                        }))
                    }
                    Err(e) => ToolOutput::error(format!("Cannot write file: {}", e)),
                }
            }

            /// List directory contents
            async fn list_dir(
                &self,
                path: &str,
                recursive: bool,
                pattern: Option<&str>,
            ) -> ToolOutput {
                let path = match self.resolve_path(path) {
                    Ok(p) => p,
                    Err(e) => return ToolOutput::error(e),
                };

                if !path.exists() {
                    return ToolOutput::error(format!("Directory not found: {}", path.display()));
                }

                if !path.is_dir() {
                    return ToolOutput::error(format!("Not a directory: {}", path.display()));
                }

                let mut entries: Vec<Value> = Vec::new();

                if recursive {
                    self.list_recursive(&path, &mut entries, pattern, &path)
                        .await;
                } else {
                    let mut read_dir = match fs::read_dir(&path).await {
                        Ok(rd) => rd,
                        Err(e) => {
                            return ToolOutput::error(format!("Cannot read directory: {}", e));
                        }
                    };

                    while let Ok(Some(entry)) = read_dir.next_entry().await {
                        if entries.len() >= MAX_LIST_ENTRIES {
                            break;
                        }

                        let name = entry.file_name().to_string_lossy().to_string();
                        if let Some(p) = pattern
                            && !glob_match(p, &name)
                        {
                            continue;
                        }

                        let file_type = match entry.file_type().await {
                            Ok(ft) => {
                                if ft.is_dir() {
                                    "dir"
                                } else if ft.is_symlink() {
                                    "symlink"
                                } else {
                                    "file"
                                }
                            }
                            Err(_) => "unknown",
                        };

                        let size = match entry.metadata().await {
                            Ok(m) => Some(m.len()),
                            Err(_) => None,
                        };

                        entries.push(serde_json::json!({
                            "name": name,
                            "type": file_type,
                            "size": size,
                        }));
                    }
                }

                let truncated = entries.len() >= MAX_LIST_ENTRIES;

                ToolOutput::success(serde_json::json!({
                    "path": path.display().to_string(),
                    "count": entries.len(),
                    "truncated": truncated,
                    "entries": entries,
                }))
            }

            /// Recursively list directory contents
            #[allow(clippy::only_used_in_recursion)]
            fn list_recursive<'a>(
                &'a self,
                dir: &'a Path,
                entries: &'a mut Vec<Value>,
                pattern: Option<&'a str>,
                base: &'a Path,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
                Box::pin(async move {
                    if entries.len() >= MAX_LIST_ENTRIES {
                        return;
                    }

                    let mut read_dir = match fs::read_dir(dir).await {
                        Ok(rd) => rd,
                        Err(_) => return,
                    };

                    while let Ok(Some(entry)) = read_dir.next_entry().await {
                        if entries.len() >= MAX_LIST_ENTRIES {
                            break;
                        }

                        let name = entry.file_name().to_string_lossy().to_string();
                        let entry_path = entry.path();
                        let relative_path = entry_path
                            .strip_prefix(base)
                            .unwrap_or(&entry_path)
                            .to_string_lossy()
                            .to_string();

                        let file_type = match entry.file_type().await {
                            Ok(ft) => {
                                if ft.is_dir() {
                                    "dir"
                                } else if ft.is_symlink() {
                                    "symlink"
                                } else {
                                    "file"
                                }
                            }
                            Err(_) => "unknown",
                        };

                        // Apply pattern filter
                        if let Some(p) = pattern
                            && !glob_match(p, &name)
                            && !glob_match(p, &relative_path)
                        {
                            // Still recurse into directories even if they don't match
                            if file_type == "dir" {
                                self.list_recursive(&entry_path, entries, pattern, base)
                                    .await;
                            }
                            continue;
                        }

                        let size = match entry.metadata().await {
                            Ok(m) => Some(m.len()),
                            Err(_) => None,
                        };

                        entries.push(serde_json::json!({
                            "path": relative_path,
                            "name": name,
                            "type": file_type,
                            "size": size,
                        }));

                        // Recurse into directories
                        if file_type == "dir" {
                            self.list_recursive(&entry_path, entries, pattern, base)
                                .await;
                        }
                    }
                })
            }

            /// Search for text in files
            async fn search_files(
                &self,
                path: &str,
                pattern: &str,
                file_pattern: Option<&str>,
            ) -> ToolOutput {
                let path = match self.resolve_path(path) {
                    Ok(p) => p,
                    Err(e) => return ToolOutput::error(e),
                };

                let search_path = path.display().to_string();
                if let Some(cache) = &self.cache_manager
                    && let Some(cached) =
                        cache.get_search(pattern, &search_path, file_pattern).await
                {
                    return Self::format_search_output(&search_path, pattern, cached);
                }

                let regex = match Regex::new(pattern) {
                    Ok(r) => r,
                    Err(e) => return ToolOutput::error(format!("Invalid regex pattern: {}", e)),
                };

                let mut matches: Vec<CachedSearchMatch> = Vec::new();
                let mut truncated = false;
                let mut total_files_searched = 0;
                self.search_recursive(
                    &path,
                    &regex,
                    file_pattern,
                    &mut matches,
                    &mut truncated,
                    &mut total_files_searched,
                    &path,
                )
                .await;

                let result = CachedSearchResult {
                    matches,
                    total_files_searched,
                    truncated,
                };

                if let Some(cache) = &self.cache_manager {
                    cache
                        .put_search(pattern, &search_path, file_pattern, result.clone())
                        .await;
                }

                Self::format_search_output(&search_path, pattern, result)
            }

            /// Recursively search for text in files
            #[allow(clippy::too_many_arguments)]
            fn search_recursive<'a>(
                &'a self,
                dir: &'a Path,
                regex: &'a Regex,
                file_pattern: Option<&'a str>,
                matches: &'a mut Vec<CachedSearchMatch>,
                truncated: &'a mut bool,
                total_files_searched: &'a mut usize,
                base: &'a Path,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
                Box::pin(async move {
                    if matches.len() >= MAX_SEARCH_MATCHES {
                        *truncated = true;
                        return;
                    }

                    if dir.is_file() {
                        // Search in single file
                        let name = dir
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                        if let Some(p) = file_pattern
                            && !glob_match(p, &name)
                        {
                            return;
                        }
                        self.search_in_file(
                            dir,
                            regex,
                            matches,
                            truncated,
                            total_files_searched,
                            base,
                        )
                        .await;
                        return;
                    }

                    let mut read_dir = match fs::read_dir(dir).await {
                        Ok(rd) => rd,
                        Err(_) => return,
                    };

                    while let Ok(Some(entry)) = read_dir.next_entry().await {
                        if matches.len() >= MAX_SEARCH_MATCHES {
                            *truncated = true;
                            break;
                        }

                        let entry_path = entry.path();
                        let file_type = match entry.file_type().await {
                            Ok(ft) => ft,
                            Err(_) => continue,
                        };

                        if file_type.is_dir() {
                            // Skip hidden directories
                            let name = entry.file_name().to_string_lossy().to_string();
                            if name.starts_with('.') {
                                continue;
                            }
                            self.search_recursive(
                                &entry_path,
                                regex,
                                file_pattern,
                                matches,
                                truncated,
                                total_files_searched,
                                base,
                            )
                            .await;
                        } else if file_type.is_file() {
                            let name = entry.file_name().to_string_lossy().to_string();

                            // Skip binary-looking files
                            if is_likely_binary(&name) {
                                continue;
                            }

                            // Apply file pattern filter
                            if let Some(p) = file_pattern
                                && !glob_match(p, &name)
                            {
                                continue;
                            }

                            self.search_in_file(
                                &entry_path,
                                regex,
                                matches,
                                truncated,
                                total_files_searched,
                                base,
                            )
                            .await;
                        }
                    }
                })
            }

            /// Search for pattern in a single file
            async fn search_in_file(
                &self,
                file: &Path,
                regex: &Regex,
                matches: &mut Vec<CachedSearchMatch>,
                truncated: &mut bool,
                total_files_searched: &mut usize,
                base: &Path,
            ) {
                let content = match fs::read_to_string(file).await {
                    Ok(c) => c,
                    Err(_) => return, // Skip files that can't be read as text
                };

                *total_files_searched += 1;

                let relative_path = file
                    .strip_prefix(base)
                    .unwrap_or(file)
                    .to_string_lossy()
                    .to_string();

                for (line_num, line) in content.lines().enumerate() {
                    if matches.len() >= MAX_SEARCH_MATCHES {
                        *truncated = true;
                        break;
                    }

                    if regex.is_match(line) {
                        matches.push(CachedSearchMatch {
                            file: relative_path.clone(),
                            line: line_num + 1,
                            content: line.chars().take(200).collect::<String>(),
                        });
                    }
                }
            }

            /// Delete a file
            async fn delete_file(&self, path: &str) -> ToolOutput {
                let path = match self.resolve_path(path) {
                    Ok(p) => p,
                    Err(e) => return ToolOutput::error(e),
                };

                // Single syscall: get metadata without following symlinks
                let metadata = match std::fs::symlink_metadata(&path) {
                    Ok(m) => m,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        return ToolOutput::error(format!("File not found: {}", path.display()));
                    }
                    Err(e) => return ToolOutput::error(format!("Cannot read metadata: {}", e)),
                };

                if metadata.file_type().is_symlink() {
                    return ToolOutput::error(format!(
                        "Symlinks are not allowed: {}",
                        path.display()
                    ));
                }

                // Enforce read-before-write: must read file before deleting
                if metadata.file_type().is_file() && !self.tracker.has_been_read(&path) {
                    return ToolOutput::error(format!(
                        "You must read {} before deleting it. Read the file first to understand what you are deleting.",
                        path.display()
                    ));
                }

                if metadata.file_type().is_dir() {
                    match fs::remove_dir_all(&path).await {
                        Ok(()) => ToolOutput::success(serde_json::json!({
                            "path": path.display().to_string(),
                            "deleted": true,
                            "type": "directory",
                        })),
                        Err(e) => ToolOutput::error(format!("Cannot delete directory: {}", e)),
                    }
                } else {
                    match fs::remove_file(&path).await {
                        Ok(()) => ToolOutput::success(serde_json::json!({
                            "path": path.display().to_string(),
                            "deleted": true,
                            "type": "file",
                        })),
                        Err(e) => ToolOutput::error(format!("Cannot delete file: {}", e)),
                    }
                }
            }

            /// Check if a path exists
            async fn check_exists(&self, path: &str) -> ToolOutput {
                let path = match self.resolve_path(path) {
                    Ok(p) => p,
                    Err(e) => return ToolOutput::error(e),
                };

                // Single syscall: get metadata without following symlinks
                let (exists, file_type, size) = match fs::symlink_metadata(&path).await {
                    Ok(meta) => {
                        let ft = meta.file_type();
                        let type_str = if ft.is_symlink() {
                            "symlink"
                        } else if ft.is_dir() {
                            "directory"
                        } else {
                            "file"
                        };
                        let size = if ft.is_file() { Some(meta.len()) } else { None };
                        (true, type_str, size)
                    }
                    Err(_) => (false, "none", None),
                };

                ToolOutput::success(serde_json::json!({
                    "path": path.display().to_string(),
                    "exists": exists,
                    "type": file_type,
                    "size": size,
                }))
            }
        }

        /// Batch read parameters
        #[derive(Debug, Clone, Deserialize)]
        pub struct BatchReadParams {
            /// List of file paths to read
            pub paths: Vec<String>,
            /// Maximum lines per file
            #[serde(default = "default_batch_line_limit")]
            pub line_limit: usize,
            /// Skip files larger than this size (bytes)
            #[serde(default = "default_batch_max_size")]
            pub max_file_size: usize,
            /// Continue on errors and return partial results
            #[serde(default = "default_continue_on_error")]
            pub continue_on_error: bool,
        }

        /// Batch exists parameters
        #[derive(Debug, Clone, Deserialize)]
        pub struct BatchExistsParams {
            /// List of paths to check
            pub paths: Vec<String>,
        }

        /// Batch search parameters
        #[derive(Debug, Clone, Deserialize)]
        pub struct BatchSearchParams {
            /// Search pattern (regex)
            pub pattern: String,
            /// List of directories or globs to search
            pub locations: Vec<String>,
            /// Maximum total matches to return
            #[serde(default = "default_batch_max_matches")]
            pub max_matches: usize,
            /// Context lines to include before/after matches
            #[serde(default = "default_context_lines")]
            pub context_lines: usize,
        }

        /// Result for a single file in batch read
        #[derive(Debug, Clone, Serialize)]
        pub struct BatchReadResult {
            pub path: String,
            pub success: bool,
            pub content: Option<String>,
            pub error: Option<String>,
            pub line_count: Option<usize>,
            pub truncated: bool,
        }

        /// Result for a single path in batch exists
        #[derive(Debug, Clone, Serialize)]
        pub struct BatchExistsResult {
            pub path: String,
            pub exists: bool,
            pub is_file: bool,
            pub is_dir: bool,
            pub size: Option<u64>,
            pub error: Option<String>,
        }

        /// Aggregated search result per location
        #[derive(Debug, Clone, Serialize)]
        pub struct BatchSearchResult {
            pub location: String,
            pub matches: Vec<SearchMatch>,
            pub match_count: usize,
            pub error: Option<String>,
        }

        /// Single search match with context
        #[derive(Debug, Clone, Serialize)]
        pub struct SearchMatch {
            pub file: String,
            pub line_number: usize,
            pub content: String,
            pub context_before: Vec<String>,
            pub context_after: Vec<String>,
        }

        impl FileTool {
            /// Execute batch read operation
            async fn batch_read(&self, params: BatchReadParams) -> ToolOutput {
                let BatchReadParams {
                    paths,
                    line_limit,
                    max_file_size,
                    continue_on_error,
                } = params;

                if paths.len() > MAX_BATCH_READ_FILES {
                    return ToolOutput::error(format!(
                        "Batch size {} exceeds maximum of {}",
                        paths.len(),
                        MAX_BATCH_READ_FILES
                    ));
                }

                let indexed_paths: Vec<(usize, String)> = paths.into_iter().enumerate().collect();

                let mut indexed_results: Vec<(usize, BatchReadResult)> =
                    stream::iter(indexed_paths.into_iter().map(|(idx, path)| async move {
                        (
                            idx,
                            self.read_single_for_batch(&path, line_limit, max_file_size)
                                .await,
                        )
                    }))
                    .buffer_unordered(BATCH_IO_CONCURRENCY)
                    .collect()
                    .await;
                indexed_results.sort_by_key(|(idx, _)| *idx);
                let results: Vec<BatchReadResult> = indexed_results
                    .into_iter()
                    .map(|(_, result)| result)
                    .collect();

                let successful = results.iter().filter(|r| r.success).count();
                let failed = results.len() - successful;

                let mut summary = format!(
                    "Read {} files ({} successful, {} failed)",
                    results.len(),
                    successful,
                    failed
                );

                if failed > 0 && continue_on_error {
                    summary.push_str(". Returned partial results.");
                }

                ToolOutput {
                    success: failed == 0 || continue_on_error,
                    result: serde_json::json!({
                        "summary": summary,
                        "total": results.len(),
                        "successful": successful,
                        "failed": failed,
                        "results": results,
                    }),
                    error: if failed > 0 && !continue_on_error {
                        Some(format!("{} files failed to read", failed))
                    } else {
                        None
                    },
                    error_category: None,
                    retryable: None,
                    retry_after_ms: None,
                }
            }

            async fn read_single_for_batch(
                &self,
                path: &str,
                line_limit: usize,
                max_file_size: usize,
            ) -> BatchReadResult {
                let resolved = match self.resolve_path(path) {
                    Ok(p) => p,
                    Err(e) => {
                        return BatchReadResult {
                            path: path.to_string(),
                            success: false,
                            content: None,
                            error: Some(e),
                            line_count: None,
                            truncated: false,
                        };
                    }
                };

                let metadata = match fs::symlink_metadata(&resolved).await {
                    Ok(m) => m,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        return BatchReadResult {
                            path: resolved.display().to_string(),
                            success: false,
                            content: None,
                            error: Some(format!("File not found: {}", resolved.display())),
                            line_count: None,
                            truncated: false,
                        };
                    }
                    Err(e) => {
                        return BatchReadResult {
                            path: resolved.display().to_string(),
                            success: false,
                            content: None,
                            error: Some(format!("Cannot read metadata: {}", e)),
                            line_count: None,
                            truncated: false,
                        };
                    }
                };

                if !metadata.file_type().is_file() {
                    return BatchReadResult {
                        path: resolved.display().to_string(),
                        success: false,
                        content: None,
                        error: Some(format!("Not a file: {}", resolved.display())),
                        line_count: None,
                        truncated: false,
                    };
                }

                if metadata.len() as usize > max_file_size {
                    return BatchReadResult {
                        path: resolved.display().to_string(),
                        success: false,
                        content: None,
                        error: Some(format!(
                            "File too large: {} bytes (max: {} bytes). Use offset and limit parameters for partial reads.",
                            metadata.len(),
                            max_file_size
                        )),
                        line_count: None,
                        truncated: false,
                    };
                }

                match fs::read_to_string(&resolved).await {
                    Ok(content) => {
                        self.tracker.record_read(&resolved);
                        let lines: Vec<&str> = content.lines().collect();
                        let line_count = lines.len();
                        let truncated = line_count > line_limit;
                        let content = if truncated {
                            lines[..line_limit].join("\n")
                        } else {
                            content
                        };

                        BatchReadResult {
                            path: resolved.display().to_string(),
                            success: true,
                            content: Some(content),
                            error: None,
                            line_count: Some(line_count),
                            truncated,
                        }
                    }
                    Err(e) => BatchReadResult {
                        path: resolved.display().to_string(),
                        success: false,
                        content: None,
                        error: Some(format!("Cannot read file: {}", e)),
                        line_count: None,
                        truncated: false,
                    },
                }
            }

            async fn check_exists_for_batch(&self, path: &str) -> BatchExistsResult {
                let resolved = match self.resolve_path(path) {
                    Ok(p) => p,
                    Err(e) => {
                        return BatchExistsResult {
                            path: path.to_string(),
                            exists: false,
                            is_file: false,
                            is_dir: false,
                            size: None,
                            error: Some(e),
                        };
                    }
                };

                match fs::symlink_metadata(&resolved).await {
                    Ok(meta) => {
                        let ft = meta.file_type();
                        BatchExistsResult {
                            path: resolved.display().to_string(),
                            exists: true,
                            is_file: ft.is_file(),
                            is_dir: ft.is_dir(),
                            size: if ft.is_file() { Some(meta.len()) } else { None },
                            error: None,
                        }
                    }
                    Err(_) => BatchExistsResult {
                        path: resolved.display().to_string(),
                        exists: false,
                        is_file: false,
                        is_dir: false,
                        size: None,
                        error: None,
                    },
                }
            }

            /// Execute batch exists operation
            async fn batch_exists(&self, params: BatchExistsParams) -> ToolOutput {
                if params.paths.len() > MAX_BATCH_EXISTS_PATHS {
                    return ToolOutput::error(format!(
                        "Batch size {} exceeds maximum of {}",
                        params.paths.len(),
                        MAX_BATCH_EXISTS_PATHS
                    ));
                }

                let indexed_paths: Vec<(usize, String)> =
                    params.paths.into_iter().enumerate().collect();

                let mut indexed_results: Vec<(usize, BatchExistsResult)> =
                    stream::iter(indexed_paths.into_iter().map(|(idx, path)| async move {
                        (idx, self.check_exists_for_batch(&path).await)
                    }))
                    .buffer_unordered(BATCH_IO_CONCURRENCY)
                    .collect()
                    .await;
                indexed_results.sort_by_key(|(idx, _)| *idx);
                let results: Vec<BatchExistsResult> = indexed_results
                    .into_iter()
                    .map(|(_, result)| result)
                    .collect();

                let existing = results.iter().filter(|r| r.exists).count();

                ToolOutput::success(serde_json::json!({
                    "total": results.len(),
                    "existing": existing,
                    "results": results,
                }))
            }

            /// Execute batch search operation
            async fn batch_search(&self, params: BatchSearchParams) -> ToolOutput {
                if params.locations.len() > MAX_BATCH_SEARCH_LOCATIONS {
                    return ToolOutput::error(format!(
                        "Location count {} exceeds maximum of {}",
                        params.locations.len(),
                        MAX_BATCH_SEARCH_LOCATIONS
                    ));
                }

                let regex = match Regex::new(&params.pattern) {
                    Ok(r) => r,
                    Err(e) => return ToolOutput::error(format!("Invalid regex: {}", e)),
                };

                let mut results: Vec<BatchSearchResult> = Vec::new();
                let mut total_matches = 0usize;

                for location in &params.locations {
                    if total_matches >= params.max_matches {
                        break;
                    }

                    let remaining = params.max_matches - total_matches;
                    let result = self
                        .search_location(location, &regex, remaining, params.context_lines)
                        .await;
                    total_matches += result.match_count;
                    results.push(result);
                }

                ToolOutput::success(serde_json::json!({
                    "pattern": params.pattern,
                    "total_matches": total_matches,
                    "locations_searched": results.len(),
                    "truncated": total_matches >= params.max_matches,
                    "results": results,
                }))
            }

            async fn search_location(
                &self,
                location: &str,
                regex: &Regex,
                max_matches: usize,
                context_lines: usize,
            ) -> BatchSearchResult {
                let mut matches: Vec<SearchMatch> = Vec::new();

                let error = if has_glob(location) {
                    (self
                        .search_glob_location(
                            location,
                            regex,
                            max_matches,
                            context_lines,
                            &mut matches,
                        )
                        .await)
                        .err()
                } else {
                    match self.resolve_path(location) {
                        Ok(path) => {
                            self.search_path_with_context(
                                &path,
                                regex,
                                max_matches,
                                context_lines,
                                &mut matches,
                            )
                            .await;
                            None
                        }
                        Err(e) => Some(e),
                    }
                };

                BatchSearchResult {
                    location: location.to_string(),
                    matches: matches.clone(),
                    match_count: matches.len(),
                    error,
                }
            }

            async fn search_glob_location(
                &self,
                location: &str,
                regex: &Regex,
                max_matches: usize,
                context_lines: usize,
                matches: &mut Vec<SearchMatch>,
            ) -> std::result::Result<(), String> {
                let (base, pattern) = split_glob_base(location);
                let base = if base.is_empty() { "." } else { base };
                let base_path = self.resolve_path(base)?;
                let pattern = if pattern.is_empty() { "*" } else { pattern };

                self.search_path_with_context_filtered(
                    &base_path,
                    regex,
                    max_matches,
                    context_lines,
                    matches,
                    Some(pattern),
                    &base_path,
                )
                .await;

                Ok(())
            }

            fn search_path_with_context<'a>(
                &'a self,
                path: &'a Path,
                regex: &'a Regex,
                max_matches: usize,
                context_lines: usize,
                matches: &'a mut Vec<SearchMatch>,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
                self.search_path_with_context_filtered(
                    path,
                    regex,
                    max_matches,
                    context_lines,
                    matches,
                    None,
                    path,
                )
            }

            #[allow(clippy::too_many_arguments)]
            fn search_path_with_context_filtered<'a>(
                &'a self,
                path: &'a Path,
                regex: &'a Regex,
                max_matches: usize,
                context_lines: usize,
                matches: &'a mut Vec<SearchMatch>,
                path_glob: Option<&'a str>,
                base: &'a Path,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
                Box::pin(async move {
                    if matches.len() >= max_matches {
                        return;
                    }

                    if path.is_file() {
                        if let Some(glob) = path_glob {
                            let rel = normalize_path_for_glob(path, base);
                            if !glob_match(glob, &rel) {
                                return;
                            }
                        }

                        self.search_in_file_with_context(
                            path,
                            regex,
                            max_matches,
                            context_lines,
                            matches,
                            base,
                        )
                        .await;
                        return;
                    }

                    let mut read_dir = match fs::read_dir(path).await {
                        Ok(rd) => rd,
                        Err(_) => return,
                    };

                    while let Ok(Some(entry)) = read_dir.next_entry().await {
                        if matches.len() >= max_matches {
                            break;
                        }

                        let entry_path = entry.path();
                        let file_type = match entry.file_type().await {
                            Ok(ft) => ft,
                            Err(_) => continue,
                        };

                        if file_type.is_dir() {
                            let name = entry.file_name().to_string_lossy().to_string();
                            if name.starts_with('.') {
                                continue;
                            }
                            self.search_path_with_context_filtered(
                                &entry_path,
                                regex,
                                max_matches,
                                context_lines,
                                matches,
                                path_glob,
                                base,
                            )
                            .await;
                        } else if file_type.is_file() {
                            let name = entry.file_name().to_string_lossy().to_string();
                            if is_likely_binary(&name) {
                                continue;
                            }

                            if let Some(glob) = path_glob {
                                let rel = normalize_path_for_glob(&entry_path, base);
                                if !glob_match(glob, &rel) {
                                    continue;
                                }
                            }

                            self.search_in_file_with_context(
                                &entry_path,
                                regex,
                                max_matches,
                                context_lines,
                                matches,
                                base,
                            )
                            .await;
                        }
                    }
                })
            }

            async fn search_in_file_with_context(
                &self,
                file: &Path,
                regex: &Regex,
                max_matches: usize,
                context_lines: usize,
                matches: &mut Vec<SearchMatch>,
                base: &Path,
            ) {
                let content = match fs::read_to_string(file).await {
                    Ok(c) => c,
                    Err(_) => return,
                };

                let relative_path = file
                    .strip_prefix(base)
                    .unwrap_or(file)
                    .to_string_lossy()
                    .to_string();

                let lines: Vec<&str> = content.lines().collect();

                for (line_index, line) in lines.iter().enumerate() {
                    if matches.len() >= max_matches {
                        break;
                    }

                    if regex.is_match(line) {
                        let start = line_index.saturating_sub(context_lines);
                        let end = (line_index + 1 + context_lines).min(lines.len());
                        let context_before = lines[start..line_index]
                            .iter()
                            .map(|line| line.to_string())
                            .collect();
                        let context_after = lines[(line_index + 1)..end]
                            .iter()
                            .map(|line| line.to_string())
                            .collect();

                        matches.push(SearchMatch {
                            file: relative_path.clone(),
                            line_number: line_index + 1,
                            content: line.to_string(),
                            context_before,
                            context_after,
                        });
                    }
                }
            }
        }

        #[async_trait]
        impl Tool for FileTool {
            fn name(&self) -> &str {
                "file"
            }

            fn description(&self) -> &str {
                "Perform file and directory operations: read, write, list, search, delete, exists, and batch variants. Use this for file content workflows; for shell command execution, use bash."
            }

            fn parameters_schema(&self) -> Value {
                file_parameters_schema()
            }

            async fn execute(&self, input: Value) -> Result<ToolOutput> {
                let action: FileAction = serde_json::from_value(input)?;

                async fn check_paths_inner(
                    security_gate: Option<&dyn SecurityGate>,
                    agent_id: Option<&str>,
                    task_id: Option<&str>,
                    operation: &str,
                    paths: &[String],
                ) -> Result<Option<String>> {
                    for path in paths {
                        let action = ToolAction {
                            tool_name: "file".to_string(),
                            operation: operation.to_string(),
                            target: path.clone(),
                            summary: format!("File {} {}", operation, path),
                        };
                        if let Some(message) =
                            check_security(security_gate, action, agent_id, task_id).await?
                        {
                            return Ok(Some(message));
                        }
                    }
                    Ok(None)
                }

                match &action {
                    FileAction::Read { path, .. } => {
                        if let Some(message) = check_paths_inner(
                            self.security_gate.as_deref(),
                            self.agent_id.as_deref(),
                            self.task_id.as_deref(),
                            "read",
                            std::slice::from_ref(path),
                        )
                        .await?
                        {
                            return Ok(ToolOutput::error(message));
                        }
                    }
                    FileAction::Write { path, .. } => {
                        if let Some(message) = check_paths_inner(
                            self.security_gate.as_deref(),
                            self.agent_id.as_deref(),
                            self.task_id.as_deref(),
                            "write",
                            std::slice::from_ref(path),
                        )
                        .await?
                        {
                            return Ok(ToolOutput::error(message));
                        }
                    }
                    FileAction::List { path, .. } => {
                        if let Some(message) = check_paths_inner(
                            self.security_gate.as_deref(),
                            self.agent_id.as_deref(),
                            self.task_id.as_deref(),
                            "list",
                            std::slice::from_ref(path),
                        )
                        .await?
                        {
                            return Ok(ToolOutput::error(message));
                        }
                    }
                    FileAction::Search { path, .. } => {
                        if let Some(message) = check_paths_inner(
                            self.security_gate.as_deref(),
                            self.agent_id.as_deref(),
                            self.task_id.as_deref(),
                            "search",
                            std::slice::from_ref(path),
                        )
                        .await?
                        {
                            return Ok(ToolOutput::error(message));
                        }
                    }
                    FileAction::Delete { path } => {
                        if let Some(message) = check_paths_inner(
                            self.security_gate.as_deref(),
                            self.agent_id.as_deref(),
                            self.task_id.as_deref(),
                            "delete",
                            std::slice::from_ref(path),
                        )
                        .await?
                        {
                            return Ok(ToolOutput::error(message));
                        }
                    }
                    FileAction::Exists { path } => {
                        if let Some(message) = check_paths_inner(
                            self.security_gate.as_deref(),
                            self.agent_id.as_deref(),
                            self.task_id.as_deref(),
                            "exists",
                            std::slice::from_ref(path),
                        )
                        .await?
                        {
                            return Ok(ToolOutput::error(message));
                        }
                    }
                    FileAction::BatchRead { paths, .. } => {
                        if let Some(message) = check_paths_inner(
                            self.security_gate.as_deref(),
                            self.agent_id.as_deref(),
                            self.task_id.as_deref(),
                            "read",
                            paths,
                        )
                        .await?
                        {
                            return Ok(ToolOutput::error(message));
                        }
                    }
                    FileAction::BatchExists { paths } => {
                        if let Some(message) = check_paths_inner(
                            self.security_gate.as_deref(),
                            self.agent_id.as_deref(),
                            self.task_id.as_deref(),
                            "exists",
                            paths,
                        )
                        .await?
                        {
                            return Ok(ToolOutput::error(message));
                        }
                    }
                    FileAction::BatchSearch { locations, .. } => {
                        if let Some(message) = check_paths_inner(
                            self.security_gate.as_deref(),
                            self.agent_id.as_deref(),
                            self.task_id.as_deref(),
                            "search",
                            locations,
                        )
                        .await?
                        {
                            return Ok(ToolOutput::error(message));
                        }
                    }
                }

                let output = match action {
                    FileAction::Read {
                        path,
                        offset,
                        limit,
                    } => self.read_file(&path, offset, limit).await,
                    FileAction::Write {
                        path,
                        content,
                        append,
                    } => self.write_file(&path, &content, append).await,
                    FileAction::List {
                        path,
                        recursive,
                        pattern,
                    } => self.list_dir(&path, recursive, pattern.as_deref()).await,
                    FileAction::Search {
                        path,
                        pattern,
                        file_pattern,
                    } => {
                        self.search_files(&path, &pattern, file_pattern.as_deref())
                            .await
                    }
                    FileAction::Delete { path } => self.delete_file(&path).await,
                    FileAction::Exists { path } => self.check_exists(&path).await,
                    FileAction::BatchRead {
                        paths,
                        line_limit,
                        max_file_size,
                        continue_on_error,
                    } => {
                        self.batch_read(BatchReadParams {
                            paths,
                            line_limit,
                            max_file_size,
                            continue_on_error,
                        })
                        .await
                    }
                    FileAction::BatchExists { paths } => {
                        self.batch_exists(BatchExistsParams { paths }).await
                    }
                    FileAction::BatchSearch {
                        pattern,
                        locations,
                        max_matches,
                        context_lines,
                    } => {
                        self.batch_search(BatchSearchParams {
                            pattern,
                            locations,
                            max_matches,
                            context_lines,
                        })
                        .await
                    }
                };

                Ok(output.classify_if_error(classify_file_error_message))
            }
        }

        fn classify_file_error_message(message: &str) -> (ToolErrorCategory, bool, Option<u64>) {
            let normalized = message.to_ascii_lowercase();

            if normalized.contains("not found")
                || normalized.contains("no such file")
                || normalized.contains("no such directory")
            {
                return (ToolErrorCategory::NotFound, false, None);
            }

            if normalized.contains("permission denied")
                || normalized.contains("operation not permitted")
                || normalized.contains("access denied")
            {
                return (ToolErrorCategory::Auth, false, None);
            }

            if normalized.contains("invalid regex")
                || normalized.contains("invalid path")
                || normalized.contains("escapes allowed base directory")
                || normalized.contains("too many")
                || normalized.contains("invalid")
            {
                return (ToolErrorCategory::Config, false, None);
            }

            (ToolErrorCategory::Execution, false, None)
        }

        /// Simple glob matching (supports * and ?)
        fn glob_match(pattern: &str, text: &str) -> bool {
            let pattern_chars: Vec<char> = pattern.chars().collect();
            let text_chars: Vec<char> = text.chars().collect();

            glob_match_helper(&pattern_chars, &text_chars)
        }

        fn glob_match_helper(pattern: &[char], text: &[char]) -> bool {
            match (pattern.first(), text.first()) {
                (None, None) => true,
                (Some('*'), _) => {
                    // * matches zero or more characters
                    glob_match_helper(&pattern[1..], text)
                        || (!text.is_empty() && glob_match_helper(pattern, &text[1..]))
                }
                (Some('?'), Some(_)) => {
                    // ? matches exactly one character
                    glob_match_helper(&pattern[1..], &text[1..])
                }
                (Some(p), Some(t)) if *p == *t => glob_match_helper(&pattern[1..], &text[1..]),
                (Some(_), None) => {
                    // Check if remaining pattern is all *
                    pattern.iter().all(|c| *c == '*')
                }
                _ => false,
            }
        }

        /// Determine if a string contains glob characters
        fn has_glob(value: &str) -> bool {
            value.contains('*') || value.contains('?')
        }

        /// Split a glob pattern into its base directory and the glob pattern
        fn split_glob_base(value: &str) -> (&str, &str) {
            let mut split_index = value.len();
            for (idx, ch) in value.char_indices() {
                if ch == '*' || ch == '?' {
                    split_index = idx;
                    break;
                }
            }

            if split_index == value.len() {
                return (value, "");
            }

            let base = &value[..split_index];
            let base = base.trim_end_matches('/');
            let pattern = value.trim_start_matches(base).trim_start_matches('/');
            (base, pattern)
        }

        fn normalize_path_for_glob(path: &Path, base: &Path) -> String {
            let relative = path.strip_prefix(base).unwrap_or(path);
            relative
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/")
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use tempfile::TempDir;

            #[test]
            fn test_file_tool_new() {
                let tool = FileTool::new();
                assert!(tool.base_dir.is_none());
                assert_eq!(tool.max_read_bytes, DEFAULT_MAX_READ_BYTES);
            }

            #[test]
            fn test_file_tool_with_base_dir() {
                let tool = FileTool::new().with_base_dir("/tmp");
                assert_eq!(tool.base_dir, Some(PathBuf::from("/tmp")));
            }

            #[test]
            fn test_file_tool_can_require_base_dir() {
                let tool = FileTool::new().require_base_dir();
                let error = tool.resolve_path("relative.txt").unwrap_err();
                assert!(error.contains("workspace root or base directory"));
            }

            #[test]
            fn test_file_tool_with_max_read() {
                let tool = FileTool::new().with_max_read(50_000);
                assert_eq!(tool.max_read_bytes, 50_000);
            }

            #[test]
            fn test_file_tool_name() {
                let tool = FileTool::new();
                assert_eq!(tool.name(), "file");
            }

            #[test]
            fn test_file_tool_description() {
                let tool = FileTool::new();
                assert!(tool.description().contains("file and directory operations"));
                assert!(tool.description().contains("use bash"));
            }

            #[test]
            fn test_file_tool_schema() {
                let tool = FileTool::new();
                let schema = tool.parameters_schema();
                assert_eq!(schema["type"], "object");
                assert!(schema["properties"]["action"].is_object());
                assert!(schema["properties"]["path"].is_object());
            }

            #[test]
            fn test_file_error_classification() {
                assert_eq!(
                    classify_file_error_message("File not found: foo.txt"),
                    (ToolErrorCategory::NotFound, false, None)
                );
                assert_eq!(
                    classify_file_error_message("Cannot open file: Permission denied"),
                    (ToolErrorCategory::Auth, false, None)
                );
            }

            #[test]
            fn test_glob_match_exact() {
                assert!(glob_match("hello", "hello"));
                assert!(!glob_match("hello", "world"));
            }

            #[test]
            fn test_glob_match_wildcard() {
                assert!(glob_match("*.rs", "main.rs"));
                assert!(glob_match("*.rs", "test.rs"));
                assert!(!glob_match("*.rs", "main.txt"));
            }

            #[test]
            fn test_glob_match_question() {
                assert!(glob_match("test?.rs", "test1.rs"));
                assert!(glob_match("test?.rs", "testa.rs"));
                assert!(!glob_match("test?.rs", "test12.rs"));
            }

            #[test]
            fn test_glob_match_complex() {
                assert!(glob_match("src/*.rs", "src/main.rs"));
                assert!(glob_match("**/test.rs", "src/test.rs"));
                assert!(glob_match("*.?s", "file.rs"));
            }

            #[test]
            fn test_is_likely_binary() {
                assert!(is_likely_binary("image.png"));
                assert!(is_likely_binary("archive.zip"));
                assert!(is_likely_binary("video.MP4"));
                assert!(!is_likely_binary("code.rs"));
                assert!(!is_likely_binary("readme.md"));
            }

            #[test]
            fn test_file_action_read_deserialization() {
                let action: FileAction = serde_json::from_value(serde_json::json!({
                    "action": "read",
                    "path": "/tmp/test.txt"
                }))
                .unwrap();

                match action {
                    FileAction::Read {
                        path,
                        offset,
                        limit,
                    } => {
                        assert_eq!(path, "/tmp/test.txt");
                        assert_eq!(offset, 0);
                        assert!(limit.is_none());
                    }
                    _ => panic!("Expected Read action"),
                }
            }

            #[test]
            fn test_file_action_write_deserialization() {
                let action: FileAction = serde_json::from_value(serde_json::json!({
                    "action": "write",
                    "path": "/tmp/test.txt",
                    "content": "hello world"
                }))
                .unwrap();

                match action {
                    FileAction::Write {
                        path,
                        content,
                        append,
                    } => {
                        assert_eq!(path, "/tmp/test.txt");
                        assert_eq!(content, "hello world");
                        assert!(!append);
                    }
                    _ => panic!("Expected Write action"),
                }
            }

            #[test]
            fn test_file_action_list_deserialization() {
                let action: FileAction = serde_json::from_value(serde_json::json!({
                    "action": "list",
                    "path": "/tmp",
                    "recursive": true,
                    "pattern": "*.rs"
                }))
                .unwrap();

                match action {
                    FileAction::List {
                        path,
                        recursive,
                        pattern,
                    } => {
                        assert_eq!(path, "/tmp");
                        assert!(recursive);
                        assert_eq!(pattern, Some("*.rs".to_string()));
                    }
                    _ => panic!("Expected List action"),
                }
            }

            #[tokio::test]
            async fn test_write_and_read_file() {
                let temp_dir = TempDir::new().unwrap();
                let tool = FileTool::new();

                let file_path = temp_dir.path().join("test.txt").display().to_string();

                // Write file
                let output = tool
                    .execute(serde_json::json!({
                        "action": "write",
                        "path": &file_path,
                        "content": "line 1\nline 2\nline 3"
                    }))
                    .await
                    .unwrap();

                assert!(output.success);

                // Read file
                let output = tool
                    .execute(serde_json::json!({
                        "action": "read",
                        "path": &file_path
                    }))
                    .await
                    .unwrap();

                assert!(output.success);
                assert!(output.result["total_lines"].as_u64().unwrap() == 3);
            }

            #[tokio::test]
            async fn test_write_append() {
                let temp_dir = TempDir::new().unwrap();
                let tool = FileTool::new();

                let file_path = temp_dir.path().join("append.txt").display().to_string();

                // Write initial content
                tool.execute(serde_json::json!({
                    "action": "write",
                    "path": &file_path,
                    "content": "first\n"
                }))
                .await
                .unwrap();

                tool.execute(serde_json::json!({
                    "action": "read",
                    "path": &file_path
                }))
                .await
                .unwrap();

                // Append more content
                tool.execute(serde_json::json!({
                    "action": "write",
                    "path": &file_path,
                    "content": "second\n",
                    "append": true
                }))
                .await
                .unwrap();

                // Read and verify
                let output = tool
                    .execute(serde_json::json!({
                        "action": "read",
                        "path": &file_path
                    }))
                    .await
                    .unwrap();

                let content = output.result["content"].as_str().unwrap();
                assert!(content.contains("first"));
                assert!(content.contains("second"));
            }

            #[tokio::test]
            async fn test_write_existing_file_requires_read_first() {
                let temp_dir = TempDir::new().unwrap();
                let file_path = temp_dir.path().join("existing.txt");
                fs::write(&file_path, "initial").await.unwrap();

                let tool = FileTool::new();
                let output = tool
                    .execute(serde_json::json!({
                        "action": "write",
                        "path": file_path.display().to_string(),
                        "content": "updated"
                    }))
                    .await
                    .unwrap();

                assert!(!output.success);
                assert!(
                    output
                        .error
                        .as_deref()
                        .unwrap_or_default()
                        .contains("You must read")
                );
            }

            #[tokio::test]
            async fn test_write_new_file_without_read_succeeds() {
                let temp_dir = TempDir::new().unwrap();
                let file_path = temp_dir.path().join("new.txt");
                let tool = FileTool::new();

                let output = tool
                    .execute(serde_json::json!({
                        "action": "write",
                        "path": file_path.display().to_string(),
                        "content": "created"
                    }))
                    .await
                    .unwrap();

                assert!(output.success);
                assert!(file_path.exists());
            }

            #[tokio::test]
            async fn test_write_does_not_count_as_read_for_existing_file() {
                let temp_dir = TempDir::new().unwrap();
                let file_path = temp_dir.path().join("new.txt");
                let tool = FileTool::new();

                // First write creates a new file and is allowed without prior read.
                let first_write = tool
                    .execute(serde_json::json!({
                        "action": "write",
                        "path": file_path.display().to_string(),
                        "content": "v1"
                    }))
                    .await
                    .unwrap();
                assert!(first_write.success);

                // Second write targets an existing file and must still require a read.
                let second_write = tool
                    .execute(serde_json::json!({
                        "action": "write",
                        "path": file_path.display().to_string(),
                        "content": "v2"
                    }))
                    .await
                    .unwrap();
                assert!(!second_write.success);
                assert!(
                    second_write
                        .error
                        .as_deref()
                        .unwrap_or_default()
                        .contains("You must read")
                );
            }

            #[tokio::test]
            async fn test_list_directory() {
                let temp_dir = TempDir::new().unwrap();
                let tool = FileTool::new();

                // Create some files
                fs::write(temp_dir.path().join("file1.txt"), "content")
                    .await
                    .unwrap();
                fs::write(temp_dir.path().join("file2.rs"), "content")
                    .await
                    .unwrap();
                fs::create_dir(temp_dir.path().join("subdir"))
                    .await
                    .unwrap();

                let output = tool
                    .execute(serde_json::json!({
                        "action": "list",
                        "path": temp_dir.path().display().to_string()
                    }))
                    .await
                    .unwrap();

                assert!(output.success);
                assert!(output.result["count"].as_u64().unwrap() >= 3);
            }

            #[tokio::test]
            async fn test_list_with_pattern() {
                let temp_dir = TempDir::new().unwrap();
                let tool = FileTool::new();

                // Create files
                fs::write(temp_dir.path().join("file1.txt"), "content")
                    .await
                    .unwrap();
                fs::write(temp_dir.path().join("file2.rs"), "content")
                    .await
                    .unwrap();
                fs::write(temp_dir.path().join("file3.txt"), "content")
                    .await
                    .unwrap();

                let output = tool
                    .execute(serde_json::json!({
                        "action": "list",
                        "path": temp_dir.path().display().to_string(),
                        "pattern": "*.txt"
                    }))
                    .await
                    .unwrap();

                assert!(output.success);
                assert_eq!(output.result["count"].as_u64().unwrap(), 2);
            }

            #[tokio::test]
            async fn test_search_files() {
                let temp_dir = TempDir::new().unwrap();
                let tool = FileTool::new();

                // Create files with content
                fs::write(
                    temp_dir.path().join("file1.txt"),
                    "hello world\ngoodbye world",
                )
                .await
                .unwrap();
                fs::write(temp_dir.path().join("file2.txt"), "no match here")
                    .await
                    .unwrap();

                let output = tool
                    .execute(serde_json::json!({
                        "action": "search",
                        "path": temp_dir.path().display().to_string(),
                        "pattern": "world"
                    }))
                    .await
                    .unwrap();

                assert!(output.success);
                assert!(output.result["match_count"].as_u64().unwrap() >= 2);
            }

            #[tokio::test]
            async fn test_exists() {
                let temp_dir = TempDir::new().unwrap();
                let tool = FileTool::new();

                let file_path = temp_dir.path().join("exists.txt");
                fs::write(&file_path, "content").await.unwrap();

                // Check existing file
                let output = tool
                    .execute(serde_json::json!({
                        "action": "exists",
                        "path": file_path.display().to_string()
                    }))
                    .await
                    .unwrap();

                assert!(output.success);
                assert!(output.result["exists"].as_bool().unwrap());
                assert_eq!(output.result["type"].as_str().unwrap(), "file");

                // Check non-existing file
                let output = tool
                    .execute(serde_json::json!({
                        "action": "exists",
                        "path": temp_dir.path().join("nonexistent.txt").display().to_string()
                    }))
                    .await
                    .unwrap();

                assert!(output.success);
                assert!(!output.result["exists"].as_bool().unwrap());
            }

            #[tokio::test]
            async fn test_delete_file() {
                let temp_dir = TempDir::new().unwrap();
                let tool = FileTool::new();

                let file_path = temp_dir.path().join("delete_me.txt");
                fs::write(&file_path, "content").await.unwrap();
                assert!(file_path.exists());

                // Read first to satisfy read-before-delete guard
                let read_output = tool
                    .execute(serde_json::json!({
                        "action": "read",
                        "path": file_path.display().to_string()
                    }))
                    .await
                    .unwrap();
                assert!(read_output.success);

                let output = tool
                    .execute(serde_json::json!({
                        "action": "delete",
                        "path": file_path.display().to_string()
                    }))
                    .await
                    .unwrap();

                assert!(output.success);
                assert!(!file_path.exists());
            }

            #[tokio::test]
            async fn test_delete_file_requires_read_first() {
                let temp_dir = TempDir::new().unwrap();
                let tool = FileTool::new();

                let file_path = temp_dir.path().join("delete_requires_read.txt");
                fs::write(&file_path, "content").await.unwrap();

                let output = tool
                    .execute(serde_json::json!({
                        "action": "delete",
                        "path": file_path.display().to_string()
                    }))
                    .await
                    .unwrap();

                assert!(!output.success);
                assert!(
                    output
                        .error
                        .as_deref()
                        .unwrap_or_default()
                        .contains("must read")
                );
                assert!(file_path.exists());
            }

            #[tokio::test]
            async fn test_read_with_offset_and_limit() {
                let temp_dir = TempDir::new().unwrap();
                let tool = FileTool::new();

                let file_path = temp_dir.path().join("lines.txt");
                fs::write(&file_path, "line 0\nline 1\nline 2\nline 3\nline 4")
                    .await
                    .unwrap();

                let output = tool
                    .execute(serde_json::json!({
                        "action": "read",
                        "path": file_path.display().to_string(),
                        "offset": 1,
                        "limit": 2
                    }))
                    .await
                    .unwrap();

                assert!(output.success);
                let content = output.result["content"].as_str().unwrap();
                assert!(content.contains("line 1"));
                assert!(content.contains("line 2"));
                assert!(!content.contains("line 0"));
                assert!(!content.contains("line 3"));
            }

            #[tokio::test]
            async fn test_base_dir_restriction() {
                let temp_dir = TempDir::new().unwrap();
                let tool = FileTool::new().with_base_dir(temp_dir.path());

                // Try to escape base directory
                let output = tool
                    .execute(serde_json::json!({
                        "action": "read",
                        "path": "../../../etc/passwd"
                    }))
                    .await
                    .unwrap();

                assert!(!output.success);
                assert!(
                    output
                        .error
                        .as_ref()
                        .unwrap()
                        .contains("escapes allowed base directory")
                );
            }

            #[tokio::test]
            #[cfg(unix)]
            async fn test_base_dir_symlink_escape_blocked() {
                use std::os::unix::fs::symlink;

                let base_dir = TempDir::new().unwrap();
                let outside_dir = TempDir::new().unwrap();
                let tool = FileTool::new().with_base_dir(base_dir.path());

                let link_path = base_dir.path().join("link");
                symlink(outside_dir.path(), &link_path).unwrap();

                let output = tool
                    .execute(serde_json::json!({
                        "action": "write",
                        "path": "link/newfile.txt",
                        "content": "nope"
                    }))
                    .await
                    .unwrap();

                assert!(!output.success);
                assert!(
                    output
                        .error
                        .as_ref()
                        .unwrap()
                        .contains("escapes allowed base directory")
                );
            }

            #[tokio::test]
            async fn test_read_nonexistent_file() {
                let tool = FileTool::new();

                let output = tool
                    .execute(serde_json::json!({
                        "action": "read",
                        "path": "/nonexistent/path/file.txt"
                    }))
                    .await
                    .unwrap();

                assert!(!output.success);
                assert!(output.error.as_ref().unwrap().contains("not found"));
            }

            #[tokio::test]
            async fn test_write_creates_parent_dirs() {
                let temp_dir = TempDir::new().unwrap();
                let tool = FileTool::new();

                let deep_path = temp_dir.path().join("a/b/c/file.txt");

                let output = tool
                    .execute(serde_json::json!({
                        "action": "write",
                        "path": deep_path.display().to_string(),
                        "content": "nested content"
                    }))
                    .await
                    .unwrap();

                assert!(output.success);
                assert!(deep_path.exists());
            }

            #[tokio::test]
            async fn test_batch_read_multiple_files() {
                let temp_dir = TempDir::new().unwrap();
                let tool = FileTool::new();

                // Create test files
                fs::write(temp_dir.path().join("file1.txt"), "content 1")
                    .await
                    .unwrap();
                fs::write(temp_dir.path().join("file2.txt"), "content 2")
                    .await
                    .unwrap();
                fs::write(temp_dir.path().join("file3.txt"), "content 3")
                    .await
                    .unwrap();

                let output = tool
                    .execute(serde_json::json!({
                        "action": "batch_read",
                        "paths": [
                            temp_dir.path().join("file1.txt").display().to_string(),
                            temp_dir.path().join("file2.txt").display().to_string(),
                            temp_dir.path().join("file3.txt").display().to_string()
                        ]
                    }))
                    .await
                    .unwrap();

                assert!(output.success);
                assert_eq!(output.result["total"].as_u64().unwrap(), 3);
                assert_eq!(output.result["successful"].as_u64().unwrap(), 3);
                assert_eq!(output.result["failed"].as_u64().unwrap(), 0);
            }

            #[tokio::test]
            async fn test_batch_read_partial_failure() {
                let temp_dir = TempDir::new().unwrap();
                let tool = FileTool::new();

                // Create one file, leave others missing
                fs::write(temp_dir.path().join("exists.txt"), "content")
                    .await
                    .unwrap();

                let output = tool
                    .execute(serde_json::json!({
                        "action": "batch_read",
                        "paths": [
                            temp_dir.path().join("exists.txt").display().to_string(),
                            temp_dir.path().join("missing.txt").display().to_string()
                        ],
                        "continue_on_error": true
                    }))
                    .await
                    .unwrap();

                assert!(output.success); // continue_on_error = true
                assert_eq!(output.result["total"].as_u64().unwrap(), 2);
                assert_eq!(output.result["successful"].as_u64().unwrap(), 1);
                assert_eq!(output.result["failed"].as_u64().unwrap(), 1);
            }

            #[tokio::test]
            async fn test_batch_read_missing_file_error_includes_path() {
                let temp_dir = TempDir::new().unwrap();
                let tool = FileTool::new();
                let missing_path = temp_dir.path().join("missing.txt");

                let output = tool
                    .execute(serde_json::json!({
                        "action": "batch_read",
                        "paths": [missing_path.display().to_string()],
                        "continue_on_error": true
                    }))
                    .await
                    .unwrap();

                assert!(output.success);
                let error = output.result["results"][0]["error"].as_str().unwrap();
                assert!(error.contains("File not found:"));
                assert!(error.contains(missing_path.display().to_string().as_str()));
            }

            #[tokio::test]
            async fn test_batch_read_large_file_error_has_partial_read_hint() {
                let temp_dir = TempDir::new().unwrap();
                let tool = FileTool::new();
                let large_file = temp_dir.path().join("large.txt");
                fs::write(&large_file, "0123456789").await.unwrap();

                let output = tool
                    .execute(serde_json::json!({
                        "action": "batch_read",
                        "paths": [large_file.display().to_string()],
                        "max_file_size": 5,
                        "continue_on_error": true
                    }))
                    .await
                    .unwrap();

                assert!(output.success);
                let error = output.result["results"][0]["error"].as_str().unwrap();
                assert!(error.contains("Use offset and limit parameters for partial reads."));
            }

            #[tokio::test]
            async fn test_batch_read_exceeds_limit() {
                let tool = FileTool::new();

                // Try to read more files than allowed
                let paths: Vec<String> = (0..25).map(|i| format!("/tmp/file{}.txt", i)).collect();

                let output = tool
                    .execute(serde_json::json!({
                        "action": "batch_read",
                        "paths": paths
                    }))
                    .await
                    .unwrap();

                assert!(!output.success);
                assert!(output.error.as_ref().unwrap().contains("exceeds maximum"));
            }

            #[tokio::test]
            async fn test_batch_exists_mixed() {
                let temp_dir = TempDir::new().unwrap();
                let tool = FileTool::new();

                // Create some paths
                fs::write(temp_dir.path().join("file.txt"), "content")
                    .await
                    .unwrap();
                fs::create_dir(temp_dir.path().join("subdir"))
                    .await
                    .unwrap();

                let output = tool
                    .execute(serde_json::json!({
                        "action": "batch_exists",
                        "paths": [
                            temp_dir.path().join("file.txt").display().to_string(),
                            temp_dir.path().join("subdir").display().to_string(),
                            temp_dir.path().join("missing.txt").display().to_string()
                        ]
                    }))
                    .await
                    .unwrap();

                assert!(output.success);
                assert_eq!(output.result["total"].as_u64().unwrap(), 3);
                assert_eq!(output.result["existing"].as_u64().unwrap(), 2);

                let results = output.result["results"].as_array().unwrap();
                assert!(results[0]["exists"].as_bool().unwrap());
                assert!(results[0]["is_file"].as_bool().unwrap());
                assert!(results[1]["exists"].as_bool().unwrap());
                assert!(results[1]["is_dir"].as_bool().unwrap());
                assert!(!results[2]["exists"].as_bool().unwrap());
            }

            #[tokio::test]
            async fn test_batch_search_single_location() {
                let temp_dir = TempDir::new().unwrap();
                let tool = FileTool::new();

                // Create files with searchable content
                fs::write(temp_dir.path().join("file1.txt"), "hello world\ntest line")
                    .await
                    .unwrap();
                fs::write(temp_dir.path().join("file2.txt"), "no match here")
                    .await
                    .unwrap();
                fs::write(temp_dir.path().join("file3.txt"), "another hello")
                    .await
                    .unwrap();

                let output = tool
                    .execute(serde_json::json!({
                        "action": "batch_search",
                        "pattern": "hello",
                        "locations": [temp_dir.path().display().to_string()]
                    }))
                    .await
                    .unwrap();

                assert!(output.success);
                assert_eq!(output.result["total_matches"].as_u64().unwrap(), 2);
            }

            #[tokio::test]
            async fn test_batch_search_with_context() {
                let temp_dir = TempDir::new().unwrap();
                let tool = FileTool::new();

                fs::write(
                    temp_dir.path().join("test.txt"),
                    "line 1\nline 2\nTARGET\nline 4\nline 5",
                )
                .await
                .unwrap();

                let output = tool
                    .execute(serde_json::json!({
                        "action": "batch_search",
                        "pattern": "TARGET",
                        "locations": [temp_dir.path().display().to_string()],
                        "context_lines": 2
                    }))
                    .await
                    .unwrap();

                assert!(output.success);
                let results = output.result["results"].as_array().unwrap();
                let matches = results[0]["matches"].as_array().unwrap();
                assert_eq!(matches.len(), 1);

                let m = &matches[0];
                assert_eq!(m["line_number"].as_u64().unwrap(), 3);
                assert_eq!(m["content"].as_str().unwrap(), "TARGET");
                assert_eq!(m["context_before"].as_array().unwrap().len(), 2);
                assert_eq!(m["context_after"].as_array().unwrap().len(), 2);
            }
        }
    }

    mod skrun {
        // skrun executable skill runtime tool.

        use async_trait::async_trait;
        use serde::Deserialize;
        use serde_json::{Value, json};
        use std::path::PathBuf;
        use std::sync::Arc;
        use std::time::Duration;
        use tokio::task;

        use crate::{Result, Tool, ToolOutput, check_security};
        use crate::{SecurityGate, ToolAction};

        fn validate_skill_id(skill_id: &str) -> anyhow::Result<()> {
            if skill_id.is_empty() {
                anyhow::bail!("skill id cannot be empty");
            }
            if !skill_id
                .chars()
                .all(|item| item.is_ascii_alphanumeric() || item == '-' || item == '_')
            {
                anyhow::bail!("skill id must contain only ASCII letters, numbers, '-' or '_'");
            }
            if !skill_id
                .chars()
                .next()
                .is_some_and(|item| item.is_ascii_alphanumeric())
            {
                anyhow::bail!("skill id must start with an ASCII letter or number");
            }
            Ok(())
        }

        fn resolve_catalog_skill(root: PathBuf, skill_id: &str) -> anyhow::Result<PathBuf> {
            validate_skill_id(skill_id)?;
            let skills_root = root
                .canonicalize()
                .map_err(|error| anyhow::anyhow!("resolve skill catalog root: {error}"))?;
            let skill_root = skills_root.join(skill_id);
            if !skill_root.exists() {
                anyhow::bail!("skill '{}' is not installed", skill_id);
            }
            let skill_root = skill_root
                .canonicalize()
                .map_err(|error| anyhow::anyhow!("resolve skill '{}': {error}", skill_id))?;
            if !skill_root.starts_with(&skills_root) {
                anyhow::bail!("skill '{}' resolves outside the skill catalog", skill_id);
            }

            let artifact = skrun::load_artifact(&skill_root)?;
            if artifact.id != skill_id {
                anyhow::bail!(
                    "skill artifact id mismatch: requested '{}', found '{}'",
                    skill_id,
                    artifact.id
                );
            }
            if !artifact.executable || artifact.kind == skrun::ArtifactKind::Markdown {
                anyhow::bail!("skill '{}' is guidance-only and cannot be run", skill_id);
            }

            Ok(skill_root)
        }

        #[derive(Debug, Deserialize)]
        struct RunSkillInput {
            id: String,
            #[serde(default)]
            input: Option<Value>,
        }

        #[derive(Clone)]
        pub struct RunSkillTool {
            root: Option<PathBuf>,
            timeout: Duration,
            security_gate: Option<Arc<dyn SecurityGate>>,
            agent_id: Option<String>,
            task_id: Option<String>,
        }

        impl Default for RunSkillTool {
            fn default() -> Self {
                Self::new()
            }
        }

        impl RunSkillTool {
            pub fn new() -> Self {
                Self {
                    root: None,
                    timeout: Duration::from_secs(60),
                    security_gate: None,
                    agent_id: None,
                    task_id: None,
                }
            }

            pub fn with_root(mut self, root: impl Into<PathBuf>) -> Self {
                self.root = Some(root.into());
                self
            }

            pub fn with_timeout(mut self, timeout: Duration) -> Self {
                self.timeout = timeout;
                self
            }

            pub fn with_security(
                mut self,
                security_gate: Arc<dyn SecurityGate>,
                agent_id: impl Into<String>,
                task_id: impl Into<String>,
            ) -> Self {
                self.security_gate = Some(security_gate);
                self.agent_id = Some(agent_id.into());
                self.task_id = Some(task_id.into());
                self
            }
        }

        #[async_trait]
        impl Tool for RunSkillTool {
            fn name(&self) -> &str {
                "run_skill"
            }

            fn description(&self) -> &str {
                "Run an installed skrun executable skill by id. Input is passed as one JSON object."
            }

            fn parameters_schema(&self) -> Value {
                json!({
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "Installed skrun skill id."
                        },
                        "input": {
                            "type": "object",
                            "description": "JSON object passed to the executable skill.",
                            "additionalProperties": true
                        }
                    },
                    "required": ["id"],
                    "additionalProperties": false
                })
            }

            async fn execute(&self, input: Value) -> Result<ToolOutput> {
                let params: RunSkillInput = serde_json::from_value(input)?;
                let skill_input = params.input.unwrap_or_else(|| json!({}));
                if !skill_input.is_object() {
                    return Ok(ToolOutput::error("skrun skill input must be a JSON object"));
                }

                if let Some(message) = check_security(
                    self.security_gate.as_deref(),
                    ToolAction {
                        tool_name: self.name().to_string(),
                        operation: "run".to_string(),
                        target: params.id.clone(),
                        summary: format!("Run skrun skill '{}'", params.id),
                    },
                    self.agent_id.as_deref(),
                    self.task_id.as_deref(),
                )
                .await?
                {
                    return Ok(ToolOutput::error(message));
                }

                let skill_id = params.id.clone();
                let root = self.root.clone();
                let timeout = self.timeout;
                let output = match task::spawn_blocking(move || {
                    let skills_root = match root {
                        Some(root) => root,
                        None => skrun::default_skills_dir()?,
                    };
                    let skill_root = resolve_catalog_skill(skills_root, &skill_id)?;
                    let options = skrun::RunOptions {
                        timeout,
                        ..Default::default()
                    };
                    skrun::run_skill(skill_root, skill_input, &options)
                })
                .await
                {
                    Ok(Ok(output)) => output,
                    Ok(Err(error)) => {
                        return Ok(ToolOutput::error(format!(
                            "skrun skill '{}' failed: {error:#}",
                            params.id
                        )));
                    }
                    Err(error) => {
                        return Ok(ToolOutput::error(format!(
                            "skrun skill '{}' task failed: {error}",
                            params.id
                        )));
                    }
                };

                Ok(ToolOutput::success(json!({
                    "skill_id": params.id,
                    "output": output.value,
                    "stderr": output.stderr,
                    "exit_code": output.exit_code,
                })))
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use serde_json::json;

            #[test]
            fn schema_requires_skill_id() {
                let schema = RunSkillTool::new().parameters_schema();
                assert_eq!(schema["required"][0], "id");
            }

            #[tokio::test]
            async fn missing_skill_returns_tool_error() {
                let tool = RunSkillTool::new().with_root("/path/to/missing/skills");

                let output = tool
                    .execute(json!({
                        "id": "missing",
                        "input": {}
                    }))
                    .await
                    .unwrap();

                assert!(!output.success);
                assert!(output.error.unwrap().contains("failed"));
            }

            #[tokio::test]
            async fn rejects_path_like_skill_id() {
                let tool = RunSkillTool::new().with_root("/path/to/missing/skills");

                let output = tool
                    .execute(json!({
                        "id": "../outside",
                        "input": {}
                    }))
                    .await
                    .unwrap();

                assert!(!output.success);
                assert!(
                    output
                        .error
                        .unwrap()
                        .contains("must contain only ASCII letters")
                );
            }

            #[tokio::test]
            async fn rejects_markdown_only_skill() {
                let dir = tempfile::tempdir().unwrap();
                let artifact = skrun::SkillArtifact::markdown(
                    "team",
                    "Team",
                    "0.1.0",
                    "# Team\n\nCoordinate work.",
                );
                skrun::save_artifact(dir.path().join("team"), &artifact).unwrap();
                let tool = RunSkillTool::new().with_root(dir.path());

                let output = tool
                    .execute(json!({
                        "id": "team",
                        "input": {}
                    }))
                    .await
                    .unwrap();

                assert!(!output.success);
                assert!(output.error.unwrap().contains("guidance-only"));
            }
        }
    }

    pub mod edit {
        // Edit tool for precise string replacement in files.
        //
        // Provides old_string/new_string replacement with 3-level fallback matching.

        use std::fmt;
        use std::path::{Path, PathBuf};
        use std::sync::Arc;

        use async_trait::async_trait;
        use serde_json::{Value, json};
        use tokio::fs;

        use super::file_tracker::FileTracker;
        use crate::{Result, Tool, ToolOutput};
        use types::cache::AgentCache;

        // ── Error types ─────────────────────────────────────────────────────

        #[derive(Debug)]
        pub enum EditError {
            IdenticalStrings,
            NotFound,
            MultipleMatches { count: usize },
        }

        impl fmt::Display for EditError {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    Self::IdenticalStrings => write!(f, "old_string and new_string are identical"),
                    Self::NotFound => write!(
                        f,
                        "old_string not found in file. Make sure the string matches exactly, including whitespace and indentation."
                    ),
                    Self::MultipleMatches { count } => write!(
                        f,
                        "old_string matched {count} locations. Provide more surrounding context to make the match unique, or set replace_all to true."
                    ),
                }
            }
        }

        // ── Replacement engine ──────────────────────────────────────────────

        /// 3-level fallback replacement.
        ///
        /// 1. Exact match
        /// 2. Line-level whitespace normalization (trim each line before compare)
        /// 3. Indentation-flexible (strip minimum common indent before compare)
        ///
        /// Returns the replaced content or an error.
        pub fn replace(
            content: &str,
            old: &str,
            new: &str,
            replace_all: bool,
        ) -> std::result::Result<String, EditError> {
            if old == new {
                return Err(EditError::IdenticalStrings);
            }

            // Level 1: exact match
            if let Some(result) = try_exact(content, old, new, replace_all) {
                return result;
            }

            // Level 2: whitespace-normalized
            if let Some(result) = try_whitespace_normalized(content, old, new, replace_all) {
                return result;
            }

            // Level 3: indentation-flexible
            if let Some(result) = try_indent_flexible(content, old, new, replace_all) {
                return result;
            }

            Err(EditError::NotFound)
        }

        pub(crate) fn count_changed_lines(old_content: &str, new_content: &str) -> usize {
            let old_lines: Vec<&str> = old_content.lines().collect();
            let new_lines: Vec<&str> = new_content.lines().collect();
            if old_lines.is_empty() || new_lines.is_empty() {
                return old_lines.len().max(new_lines.len());
            }

            let mut previous = vec![0; new_lines.len() + 1];
            let mut current = vec![0; new_lines.len() + 1];
            for old_line in &old_lines {
                for (new_index, new_line) in new_lines.iter().enumerate() {
                    current[new_index + 1] = if old_line == new_line {
                        previous[new_index] + 1
                    } else {
                        previous[new_index + 1].max(current[new_index])
                    };
                }
                std::mem::swap(&mut previous, &mut current);
                current.fill(0);
            }

            old_lines.len().max(new_lines.len()) - previous[new_lines.len()]
        }

        /// Level 1: exact substring match.
        fn try_exact(
            content: &str,
            old: &str,
            new: &str,
            replace_all: bool,
        ) -> Option<std::result::Result<String, EditError>> {
            let first = content.find(old)?;

            if replace_all {
                return Some(Ok(content.replace(old, new)));
            }

            // Uniqueness check
            if content[first + 1..].contains(old) {
                let count = content.matches(old).count();
                return Some(Err(EditError::MultipleMatches { count }));
            }

            let mut result = String::with_capacity(content.len() - old.len() + new.len());
            result.push_str(&content[..first]);
            result.push_str(new);
            result.push_str(&content[first + old.len()..]);
            Some(Ok(result))
        }

        /// Level 2: compare lines after trimming trailing whitespace on each line.
        fn try_whitespace_normalized(
            content: &str,
            old: &str,
            new: &str,
            replace_all: bool,
        ) -> Option<std::result::Result<String, EditError>> {
            let old_lines: Vec<&str> = old.lines().collect();
            if old_lines.is_empty() {
                return None;
            }

            let content_lines: Vec<&str> = content.lines().collect();
            let mut match_positions = Vec::new();

            'outer: for i in 0..content_lines.len().saturating_sub(old_lines.len() - 1) {
                for (j, old_line) in old_lines.iter().enumerate() {
                    if content_lines[i + j].trim_end() != old_line.trim_end() {
                        continue 'outer;
                    }
                }
                match_positions.push(i);
            }

            if match_positions.is_empty() {
                return None;
            }

            if !replace_all && match_positions.len() > 1 {
                return Some(Err(EditError::MultipleMatches {
                    count: match_positions.len(),
                }));
            }

            // Apply replacement on the original content using byte offsets
            Some(Ok(apply_line_replacements(
                content,
                &content_lines,
                &old_lines,
                new,
                &match_positions,
            )))
        }

        /// Level 3: strip minimum common indentation from both old_string and content
        /// window before comparing.
        fn try_indent_flexible(
            content: &str,
            old: &str,
            new: &str,
            replace_all: bool,
        ) -> Option<std::result::Result<String, EditError>> {
            let old_lines: Vec<&str> = old.lines().collect();
            if old_lines.is_empty() {
                return None;
            }

            let old_deindented = deindent_lines(&old_lines);
            let content_lines: Vec<&str> = content.lines().collect();
            let mut match_positions = Vec::new();

            for i in 0..content_lines.len().saturating_sub(old_lines.len() - 1) {
                let window = &content_lines[i..i + old_lines.len()];
                let window_deindented = deindent_lines(window);
                if window_deindented == old_deindented {
                    match_positions.push(i);
                }
            }

            if match_positions.is_empty() {
                return None;
            }

            if !replace_all && match_positions.len() > 1 {
                return Some(Err(EditError::MultipleMatches {
                    count: match_positions.len(),
                }));
            }

            Some(Ok(apply_line_replacements(
                content,
                &content_lines,
                &old_lines,
                new,
                &match_positions,
            )))
        }

        /// Remove the minimum common leading whitespace from a slice of lines.
        fn deindent_lines(lines: &[&str]) -> Vec<String> {
            let min_indent = lines
                .iter()
                .filter(|l| !l.trim().is_empty())
                .map(|l| l.len() - l.trim_start().len())
                .min()
                .unwrap_or(0);

            lines
                .iter()
                .map(|l| {
                    if l.len() >= min_indent {
                        l[min_indent..].to_string()
                    } else {
                        l.to_string()
                    }
                })
                .collect()
        }

        /// Replace matched line ranges in the original content, preserving byte-level
        /// accuracy for unmatched regions.
        fn apply_line_replacements(
            content: &str,
            content_lines: &[&str],
            old_lines: &[&str],
            new: &str,
            positions: &[usize],
        ) -> String {
            let old_line_count = old_lines.len();

            // Build byte offset map: line_index -> byte start in `content`
            let mut line_offsets: Vec<usize> = Vec::with_capacity(content_lines.len() + 1);
            let mut offset = 0;
            for line in content_lines {
                line_offsets.push(offset);
                offset += line.len();
                // Account for the newline character if present
                if offset < content.len() {
                    offset += if content.as_bytes().get(offset) == Some(&b'\r') {
                        if content.as_bytes().get(offset + 1) == Some(&b'\n') {
                            2
                        } else {
                            1
                        }
                    } else {
                        1
                    };
                }
            }
            line_offsets.push(content.len());

            let mut result = String::with_capacity(content.len());
            let mut last_end = 0;

            for &pos in positions {
                let match_start = line_offsets[pos];
                let match_end = line_offsets[pos + old_line_count];

                result.push_str(&content[last_end..match_start]);
                result.push_str(new);

                // If the matched range ended before EOF and replacement doesn't end
                // with newline but original did, preserve it
                if match_end > match_start
                    && !new.ends_with('\n')
                    && match_end <= content.len()
                    && content[match_start..match_end].ends_with('\n')
                {
                    result.push('\n');
                }

                last_end = match_end;
            }

            result.push_str(&content[last_end..]);
            result
        }

        // ── Tool implementation ─────────────────────────────────────────────

        #[derive(Clone)]
        pub struct EditTool {
            base_dir: Option<PathBuf>,
            require_base_dir: bool,
            tracker: Arc<FileTracker>,
            cache_manager: Option<Arc<dyn AgentCache>>,
        }

        impl EditTool {
            pub fn with_tracker(tracker: Arc<FileTracker>) -> Self {
                Self {
                    base_dir: None,
                    require_base_dir: false,
                    tracker,
                    cache_manager: None,
                }
            }

            pub fn with_base_dir(mut self, base_dir: impl Into<PathBuf>) -> Self {
                self.base_dir = Some(base_dir.into());
                self
            }

            pub fn require_base_dir(mut self) -> Self {
                self.require_base_dir = true;
                self
            }

            pub fn with_cache_manager(mut self, cache: Arc<dyn AgentCache>) -> Self {
                self.cache_manager = Some(cache);
                self
            }

            fn resolve_path(&self, path: &str) -> std::result::Result<PathBuf, String> {
                super::path_utils::resolve_path_with_policy(
                    path,
                    self.base_dir.as_deref(),
                    self.require_base_dir,
                )
            }

            /// Invalidate caches for the given path and its parent directories.
            async fn invalidate_caches(&self, path: &Path) {
                if let Some(cache) = &self.cache_manager {
                    cache.invalidate_file(path).await;
                    let mut current = path.parent();
                    while let Some(directory) = current {
                        cache
                            .invalidate_search_dir(&directory.to_string_lossy())
                            .await;
                        current = directory.parent();
                    }
                }
            }
        }

        #[async_trait]
        impl Tool for EditTool {
            fn name(&self) -> &str {
                "edit"
            }

            fn description(&self) -> &str {
                "Make precise text replacements in a file using old_string/new_string matching. \
                 Supports exact match, whitespace-normalized match, and indentation-flexible match. \
                 Use replace_all to replace all occurrences."
            }

            fn parameters_schema(&self) -> Value {
                json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "Path to the file to edit"
                        },
                        "old_string": {
                            "type": "string",
                            "description": "The exact text to find and replace"
                        },
                        "new_string": {
                            "type": "string",
                            "description": "The replacement text"
                        },
                        "replace_all": {
                            "type": "boolean",
                            "description": "Replace all occurrences (default: false)",
                            "default": false
                        }
                    },
                    "required": ["file_path", "old_string", "new_string"]
                })
            }

            async fn execute(&self, args: Value) -> Result<ToolOutput> {
                let file_path = args
                    .get("file_path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| crate::ToolError::Tool("Missing 'file_path' argument".into()))?;
                let old_string =
                    args.get("old_string")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            crate::ToolError::Tool("Missing 'old_string' argument".into())
                        })?;
                let new_string =
                    args.get("new_string")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            crate::ToolError::Tool("Missing 'new_string' argument".into())
                        })?;
                let replace_all = args
                    .get("replace_all")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                // Resolve path
                let path = match self.resolve_path(file_path) {
                    Ok(p) => p,
                    Err(e) => return Ok(ToolOutput::error(e)),
                };

                // Guard: file must exist
                if !path.exists() {
                    return Ok(ToolOutput::error(format!(
                        "File not found: {}",
                        path.display()
                    )));
                }

                // Guard: must have been read first
                if !self.tracker.has_been_read(&path) {
                    return Ok(ToolOutput::error(format!(
                        "You must read {} before editing it. Read the file first to understand its current content.",
                        path.display()
                    )));
                }

                // Guard: external modification
                match self.tracker.check_external_modification(&path).await {
                    Ok(true) => {
                        return Ok(ToolOutput::error(format!(
                            "File {} has been modified externally since it was read. Read it again before editing.",
                            path.display()
                        )));
                    }
                    Ok(false) => {}
                    Err(e) => {
                        return Ok(ToolOutput::error(format!(
                            "Cannot check file modification time: {e}"
                        )));
                    }
                }

                // Read current content
                let content = match fs::read_to_string(&path).await {
                    Ok(c) => c,
                    Err(e) => {
                        return Ok(ToolOutput::error(format!("Cannot read file: {e}")));
                    }
                };

                // Apply replacement
                let new_content = match replace(&content, old_string, new_string, replace_all) {
                    Ok(c) => c,
                    Err(e) => return Ok(ToolOutput::error(e.to_string())),
                };

                // Count changed lines for summary
                let lines_changed = count_changed_lines(&content, &new_content);

                // Write back
                if let Err(e) = fs::write(&path, &new_content).await {
                    return Ok(ToolOutput::error(format!("Cannot write file: {e}")));
                }

                self.tracker.record_write(&path);
                self.invalidate_caches(&path).await;

                // Build output message
                let msg = format!(
                    "Edit applied to {} ({} lines changed)",
                    path.display(),
                    lines_changed
                );

                Ok(ToolOutput::success(json!({
                    "message": msg,
                    "path": path.display().to_string(),
                    "lines_changed": lines_changed,
                })))
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            // ── Replacer pure-function tests ────────────────────────────────

            #[test]
            fn test_exact_match_unique() {
                let content = "fn main() {\n    println!(\"hello\");\n}\n";
                let result = replace(content, "println!(\"hello\")", "println!(\"world\")", false);
                assert_eq!(
                    result.unwrap(),
                    "fn main() {\n    println!(\"world\");\n}\n"
                );
            }

            #[test]
            fn test_exact_match_not_found() {
                let content = "fn main() {}";
                let result = replace(content, "nonexistent", "replacement", false);
                assert!(matches!(result.unwrap_err(), EditError::NotFound));
            }

            #[test]
            fn test_exact_match_multiple_rejected() {
                let content = "aaa bbb aaa";
                let result = replace(content, "aaa", "ccc", false);
                assert!(matches!(
                    result.unwrap_err(),
                    EditError::MultipleMatches { count: 2 }
                ));
            }

            #[test]
            fn test_replace_all() {
                let content = "aaa bbb aaa";
                let result = replace(content, "aaa", "ccc", true).unwrap();
                assert_eq!(result, "ccc bbb ccc");
            }

            #[test]
            fn test_identical_strings_error() {
                let result = replace("content", "same", "same", false);
                assert!(matches!(result.unwrap_err(), EditError::IdenticalStrings));
            }

            #[test]
            fn test_whitespace_normalized_matches() {
                let content = "fn foo() {  \n    bar();  \n}\n";
                // old_string has no trailing spaces
                let old = "fn foo() {\n    bar();\n}";
                let new = "fn foo() {\n    baz();\n}";
                let result = replace(content, old, new, false).unwrap();
                assert!(result.contains("baz()"));
            }

            #[test]
            fn test_indentation_flexible_matches() {
                let content = "        if true {\n            do_stuff();\n        }\n";
                // old_string uses less indentation
                let old = "    if true {\n        do_stuff();\n    }";
                let new = "    if true {\n        do_other();\n    }";
                let result = replace(content, old, new, false).unwrap();
                assert!(result.contains("do_other()"));
            }

            #[test]
            fn test_fallback_chain_prefers_exact() {
                // Content that matches at all levels: exact should win
                let content = "hello world\n";
                let result = replace(content, "hello world", "goodbye world", false).unwrap();
                assert_eq!(result, "goodbye world\n");
            }

            // ── Tool integration tests ──────────────────────────────────────

            #[tokio::test]
            async fn test_edit_tool_basic() {
                let dir = tempfile::tempdir().unwrap();
                let base = dir.path().canonicalize().unwrap();
                let file_path = base.join("test.txt");
                tokio::fs::write(&file_path, "line1\nline2\nline3\n")
                    .await
                    .unwrap();

                let tracker = Arc::new(FileTracker::new());
                tracker.record_read(&file_path);

                let tool = EditTool::with_tracker(tracker).with_base_dir(&base);

                let output = tool
                    .execute(json!({
                        "file_path": file_path.to_str().unwrap(),
                        "old_string": "line2",
                        "new_string": "modified"
                    }))
                    .await
                    .unwrap();

                assert!(output.success);
                assert_eq!(output.result["lines_changed"], 1);
                let content = tokio::fs::read_to_string(&file_path).await.unwrap();
                assert_eq!(content, "line1\nmodified\nline3\n");
            }

            #[test]
            fn test_count_changed_lines_counts_same_length_replacements() {
                assert_eq!(count_changed_lines("a\nb\nc\n", "a\nx\nc\n"), 1);
                assert_eq!(count_changed_lines("a\nb\nc\n", "x\nb\ny\n"), 2);
            }

            #[test]
            fn test_count_changed_lines_handles_insertions_and_deletions() {
                assert_eq!(count_changed_lines("a\nb\nc\n", "x\na\nb\nc\n"), 1);
                assert_eq!(count_changed_lines("a\nb\nc\n", "a\nc\n"), 1);
                assert_eq!(count_changed_lines("a\nb\nc\n", "a\nx\ny\nc\n"), 2);
            }

            #[tokio::test]
            async fn test_edit_tool_read_guard() {
                let dir = tempfile::tempdir().unwrap();
                let base = dir.path().canonicalize().unwrap();
                let file_path = base.join("test.txt");
                tokio::fs::write(&file_path, "content").await.unwrap();

                let tracker = Arc::new(FileTracker::new());
                // Intentionally do NOT record a read

                let tool = EditTool::with_tracker(tracker).with_base_dir(&base);

                let output = tool
                    .execute(json!({
                        "file_path": file_path.to_str().unwrap(),
                        "old_string": "content",
                        "new_string": "replaced"
                    }))
                    .await
                    .unwrap();

                assert!(!output.success);
                assert!(output.error.as_deref().unwrap_or("").contains("must read"));
            }
        }
    }

    pub mod multiedit {
        // Multi-edit tool for applying multiple replacements to a single file atomically.
        //
        // All edits succeed or the file is not modified.

        use std::path::{Path, PathBuf};
        use std::sync::Arc;

        use async_trait::async_trait;
        use serde_json::{Value, json};
        use tokio::fs;

        use super::edit::{EditError, count_changed_lines, replace};
        use super::file_tracker::FileTracker;
        use crate::{Result, Tool, ToolOutput};
        use types::cache::AgentCache;

        #[derive(Clone)]
        pub struct MultiEditTool {
            base_dir: Option<PathBuf>,
            require_base_dir: bool,
            tracker: Arc<FileTracker>,
            cache_manager: Option<Arc<dyn AgentCache>>,
        }

        impl MultiEditTool {
            pub fn with_tracker(tracker: Arc<FileTracker>) -> Self {
                Self {
                    base_dir: None,
                    require_base_dir: false,
                    tracker,
                    cache_manager: None,
                }
            }

            pub fn with_base_dir(mut self, base_dir: impl Into<PathBuf>) -> Self {
                self.base_dir = Some(base_dir.into());
                self
            }

            pub fn require_base_dir(mut self) -> Self {
                self.require_base_dir = true;
                self
            }

            pub fn with_cache_manager(mut self, cache: Arc<dyn AgentCache>) -> Self {
                self.cache_manager = Some(cache);
                self
            }

            fn resolve_path(&self, path: &str) -> std::result::Result<PathBuf, String> {
                super::path_utils::resolve_path_with_policy(
                    path,
                    self.base_dir.as_deref(),
                    self.require_base_dir,
                )
            }

            async fn invalidate_caches(&self, path: &Path) {
                if let Some(cache) = &self.cache_manager {
                    cache.invalidate_file(path).await;
                    let mut current = path.parent();
                    while let Some(directory) = current {
                        cache
                            .invalidate_search_dir(&directory.to_string_lossy())
                            .await;
                        current = directory.parent();
                    }
                }
            }
        }

        #[async_trait]
        impl Tool for MultiEditTool {
            fn name(&self) -> &str {
                "multiedit"
            }

            fn description(&self) -> &str {
                "Apply multiple text replacements to a single file atomically. \
                 All edits must succeed or the file is not modified."
            }

            fn parameters_schema(&self) -> Value {
                json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "Path to the file to edit"
                        },
                        "edits": {
                            "type": "array",
                            "description": "List of edit operations to apply sequentially",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "old_string": {
                                        "type": "string",
                                        "description": "The exact text to find"
                                    },
                                    "new_string": {
                                        "type": "string",
                                        "description": "The replacement text"
                                    },
                                    "replace_all": {
                                        "type": "boolean",
                                        "description": "Replace all occurrences (default: false)",
                                        "default": false
                                    }
                                },
                                "required": ["old_string", "new_string"]
                            }
                        }
                    },
                    "required": ["file_path", "edits"]
                })
            }

            async fn execute(&self, args: Value) -> Result<ToolOutput> {
                let file_path = args
                    .get("file_path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| crate::ToolError::Tool("Missing 'file_path' argument".into()))?;

                let edits = args
                    .get("edits")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| {
                        crate::ToolError::Tool("Missing 'edits' array argument".into())
                    })?;

                if edits.is_empty() {
                    return Ok(ToolOutput::error("'edits' array must not be empty"));
                }

                // Resolve path
                let path = match self.resolve_path(file_path) {
                    Ok(p) => p,
                    Err(e) => return Ok(ToolOutput::error(e)),
                };

                // Guard: file must exist
                if !path.exists() {
                    return Ok(ToolOutput::error(format!(
                        "File not found: {}",
                        path.display()
                    )));
                }

                // Guard: must have been read first
                if !self.tracker.has_been_read(&path) {
                    return Ok(ToolOutput::error(format!(
                        "You must read {} before editing it. Read the file first to understand its current content.",
                        path.display()
                    )));
                }

                // Guard: external modification
                match self.tracker.check_external_modification(&path).await {
                    Ok(true) => {
                        return Ok(ToolOutput::error(format!(
                            "File {} has been modified externally since it was read. Read it again before editing.",
                            path.display()
                        )));
                    }
                    Ok(false) => {}
                    Err(e) => {
                        return Ok(ToolOutput::error(format!(
                            "Cannot check file modification time: {e}"
                        )));
                    }
                }

                // Read current content
                let content = match fs::read_to_string(&path).await {
                    Ok(c) => c,
                    Err(e) => {
                        return Ok(ToolOutput::error(format!("Cannot read file: {e}")));
                    }
                };

                // Apply all edits sequentially in memory
                let mut current = content.clone();
                for (i, edit) in edits.iter().enumerate() {
                    let old_string =
                        edit.get("old_string")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| {
                                crate::ToolError::Tool(format!("Edit {i}: missing 'old_string'"))
                            })?;
                    let new_string =
                        edit.get("new_string")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| {
                                crate::ToolError::Tool(format!("Edit {i}: missing 'new_string'"))
                            })?;
                    let replace_all_edit = edit
                        .get("replace_all")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    match replace(&current, old_string, new_string, replace_all_edit) {
                        Ok(result) => current = result,
                        Err(EditError::IdenticalStrings) => {
                            return Ok(ToolOutput::error(format!(
                                "Edit {i}: old_string and new_string are identical"
                            )));
                        }
                        Err(EditError::NotFound) => {
                            return Ok(ToolOutput::error(format!(
                                "Edit {i}: old_string not found in file (after applying previous edits). \
                                 No changes were written to disk."
                            )));
                        }
                        Err(EditError::MultipleMatches { count }) => {
                            return Ok(ToolOutput::error(format!(
                                "Edit {i}: old_string matched {count} locations. \
                                 No changes were written to disk."
                            )));
                        }
                    }
                }

                // All edits succeeded; write once
                let lines_changed = count_changed_lines(&content, &current);

                if let Err(e) = fs::write(&path, &current).await {
                    return Ok(ToolOutput::error(format!("Cannot write file: {e}")));
                }

                self.tracker.record_write(&path);
                self.invalidate_caches(&path).await;

                let msg = format!(
                    "{} edits applied to {} ({} lines changed)",
                    edits.len(),
                    path.display(),
                    lines_changed
                );

                Ok(ToolOutput::success(json!({
                    "message": msg,
                    "path": path.display().to_string(),
                    "edits_applied": edits.len(),
                    "lines_changed": lines_changed,
                })))
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            #[tokio::test]
            async fn test_multiedit_sequential() {
                let dir = tempfile::tempdir().unwrap();
                let base = dir.path().canonicalize().unwrap();
                let file_path = base.join("test.txt");
                tokio::fs::write(&file_path, "aaa\nbbb\nccc\n")
                    .await
                    .unwrap();

                let tracker = Arc::new(FileTracker::new());
                tracker.record_read(&file_path);

                let tool = MultiEditTool::with_tracker(tracker).with_base_dir(&base);

                let output = tool
                    .execute(json!({
                        "file_path": file_path.to_str().unwrap(),
                        "edits": [
                            { "old_string": "aaa", "new_string": "xxx" },
                            { "old_string": "ccc", "new_string": "zzz" }
                        ]
                    }))
                    .await
                    .unwrap();

                assert!(output.success);
                assert_eq!(output.result["lines_changed"], 2);
                let content = tokio::fs::read_to_string(&file_path).await.unwrap();
                assert_eq!(content, "xxx\nbbb\nzzz\n");
            }

            #[tokio::test]
            async fn test_multiedit_atomic_failure() {
                let dir = tempfile::tempdir().unwrap();
                let base = dir.path().canonicalize().unwrap();
                let file_path = base.join("test.txt");
                tokio::fs::write(&file_path, "aaa\nbbb\nccc\n")
                    .await
                    .unwrap();

                let tracker = Arc::new(FileTracker::new());
                tracker.record_read(&file_path);

                let tool = MultiEditTool::with_tracker(tracker).with_base_dir(&base);

                let output = tool
                    .execute(json!({
                        "file_path": file_path.to_str().unwrap(),
                        "edits": [
                            { "old_string": "aaa", "new_string": "xxx" },
                            { "old_string": "nonexistent", "new_string": "yyy" }
                        ]
                    }))
                    .await
                    .unwrap();

                assert!(!output.success);
                // File should NOT have been modified
                let content = tokio::fs::read_to_string(&file_path).await.unwrap();
                assert_eq!(content, "aaa\nbbb\nccc\n");
            }
        }
    }

    pub mod agent_crud {
        // Agent CRUD tool for managing stored agents.

        use async_trait::async_trait;
        use serde::Deserialize;
        use serde_json::{Value, json};
        use std::sync::Arc;
        use types::request::AgentNode as ContractAgentNode;

        use crate::Result;
        use crate::{Tool, ToolError, ToolOutput};
        use types::store::{AgentCreateRequest, AgentStore, AgentUpdateRequest};

        #[derive(Clone)]
        pub struct AgentCrudTool {
            store: Arc<dyn AgentStore>,
            allow_write: bool,
        }

        impl AgentCrudTool {
            pub fn new(store: Arc<dyn AgentStore>) -> Self {
                Self {
                    store,
                    allow_write: false,
                }
            }

            pub fn with_write(mut self, allow_write: bool) -> Self {
                self.allow_write = allow_write;
                self
            }

            fn write_guard(&self) -> Result<()> {
                if self.allow_write {
                    Ok(())
                } else {
                    Err(crate::ToolError::Tool(
                        "Write access to agents is disabled. Available read-only operations: list, get. To modify agents, the user must grant write permissions.".to_string(),
                    ))
                }
            }

            fn parse_contract_agent(value: Value) -> Result<ContractAgentNode> {
                serde_json::from_value(value)
                    .map_err(|e| ToolError::Tool(format!("Invalid agent payload: {e}")))
            }
        }

        #[derive(Debug, Deserialize)]
        #[serde(tag = "operation", rename_all = "snake_case")]
        enum AgentAction {
            List,
            Show {
                id: String,
            },
            Create {
                name: String,
                agent: Value,
            },
            Update {
                id: String,
                #[serde(default)]
                name: Option<String>,
                #[serde(default)]
                agent: Option<Value>,
            },
            Delete {
                id: String,
            },
        }

        #[async_trait]
        impl Tool for AgentCrudTool {
            fn name(&self) -> &str {
                "manage_agents"
            }

            fn description(&self) -> &str {
                "Create, read, update, list, and delete agent definitions and configuration."
            }

            fn parameters_schema(&self) -> Value {
                json!({
                    "type": "object",
                    "properties": {
                        "operation": {
                            "type": "string",
                            "enum": ["list", "show", "create", "update", "delete"],
                            "description": "Agent operation to perform"
                        },
                        "id": {
                            "type": "string",
                            "description": "Agent ID (for show/update/delete)"
                        },
                        "name": {
                            "type": "string",
                            "description": "Agent name (for create/update)"
                        },
                        "agent": {
                            "type": "object",
                            "description": "Agent configuration (for create/update)"
                        }
                    },
                    "required": ["operation"]
                })
            }

            async fn execute(&self, input: Value) -> Result<ToolOutput> {
                let action: AgentAction = serde_json::from_value(input)?;

                let output =
                    match action {
                        AgentAction::List => {
                            ToolOutput::success(self.store.list_agents().map_err(|e| {
                                ToolError::Tool(format!("Failed to list agent: {e}"))
                            })?)
                        }
                        AgentAction::Show { id } => {
                            ToolOutput::success(self.store.get_agent(&id).map_err(|e| {
                                ToolError::Tool(format!("Failed to get agent: {e}"))
                            })?)
                        }
                        AgentAction::Create { name, agent } => {
                            self.write_guard()?;
                            let request = AgentCreateRequest {
                                name,
                                agent: Self::parse_contract_agent(agent)?,
                            };
                            ToolOutput::success(self.store.create_agent(request).map_err(|e| {
                                let message = e.to_string();
                                if message.contains("\"type\":\"validation_error\"") {
                                    ToolError::Tool(message)
                                } else {
                                    ToolError::Tool(format!("Failed to create agent: {e}"))
                                }
                            })?)
                        }
                        AgentAction::Update { id, name, agent } => {
                            self.write_guard()?;
                            let request = AgentUpdateRequest {
                                id,
                                name,
                                agent: agent.map(Self::parse_contract_agent).transpose()?,
                            };
                            ToolOutput::success(self.store.update_agent(request).map_err(|e| {
                                let message = e.to_string();
                                if message.contains("\"type\":\"validation_error\"") {
                                    ToolError::Tool(message)
                                } else {
                                    ToolError::Tool(format!("Failed to update agent: {e}"))
                                }
                            })?)
                        }
                        AgentAction::Delete { id } => {
                            self.write_guard()?;
                            ToolOutput::success(self.store.delete_agent(&id).map_err(|e| {
                                ToolError::Tool(format!("Failed to delete agent: {e}"))
                            })?)
                        }
                    };

                Ok(output)
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use std::sync::Mutex;

            struct MockStore;

            impl AgentStore for MockStore {
                fn list_agents(&self) -> Result<Value> {
                    Ok(json!([{"id": "agent-1"}]))
                }

                fn get_agent(&self, _id: &str) -> Result<Value> {
                    Ok(json!({"id": "agent-1"}))
                }

                fn create_agent(&self, _request: AgentCreateRequest) -> Result<Value> {
                    Ok(json!({"id": "agent-1"}))
                }

                fn update_agent(&self, _request: AgentUpdateRequest) -> Result<Value> {
                    Ok(json!({"id": "agent-1"}))
                }

                fn delete_agent(&self, _id: &str) -> Result<Value> {
                    Ok(json!({"deleted": true}))
                }
            }

            #[tokio::test]
            async fn test_list_agents() {
                let tool = AgentCrudTool::new(Arc::new(MockStore));
                let output = tool.execute(json!({"operation": "list"})).await.unwrap();
                assert!(output.success);
            }

            #[tokio::test]
            async fn test_create_requires_write() {
                let tool = AgentCrudTool::new(Arc::new(MockStore));
                let result = tool
                    .execute(json!({"operation": "create", "name": "Agent", "agent": {}}))
                    .await;
                let err = result.expect_err("expected write-guard error");
                assert!(
                    err.to_string()
                        .contains("Available read-only operations: list, get")
                );
            }

            struct ValidationStore;

            impl AgentStore for ValidationStore {
                fn list_agents(&self) -> Result<Value> {
                    Ok(json!([]))
                }

                fn get_agent(&self, _id: &str) -> Result<Value> {
                    Ok(json!({}))
                }

                fn create_agent(&self, _request: AgentCreateRequest) -> Result<Value> {
                    Err(crate::ToolError::Tool(
                        "{\"type\":\"validation_error\",\"errors\":[{\"field\":\"temperature\",\"message\":\"invalid\"}]}".to_string(),
                    ))
                }

                fn update_agent(&self, _request: AgentUpdateRequest) -> Result<Value> {
                    Err(crate::ToolError::Tool(
                        "{\"type\":\"validation_error\",\"errors\":[{\"field\":\"tools\",\"message\":\"unknown\"}]}".to_string(),
                    ))
                }

                fn delete_agent(&self, _id: &str) -> Result<Value> {
                    Ok(json!({}))
                }
            }

            #[tokio::test]
            async fn test_create_propagates_validation_payload_without_wrapping() {
                let tool = AgentCrudTool::new(Arc::new(ValidationStore)).with_write(true);
                let err = tool
                    .execute(json!({"operation": "create", "name": "Agent", "agent": {}}))
                    .await
                    .expect_err("expected validation error");
                assert!(err.to_string().contains("\"type\":\"validation_error\""));
            }

            struct CapturingStore {
                captured_model_ref: Arc<Mutex<Option<Option<types::request::WireModelRef>>>>,
            }

            impl AgentStore for CapturingStore {
                fn list_agents(&self) -> Result<Value> {
                    Ok(json!([]))
                }

                fn get_agent(&self, _id: &str) -> Result<Value> {
                    Ok(json!({}))
                }

                fn create_agent(&self, request: AgentCreateRequest) -> Result<Value> {
                    *self
                        .captured_model_ref
                        .lock()
                        .expect("captured model_ref lock") = Some(request.agent.model_ref);
                    Ok(json!({"id": "agent-1"}))
                }

                fn update_agent(&self, _request: AgentUpdateRequest) -> Result<Value> {
                    Ok(json!({"id": "agent-1"}))
                }

                fn delete_agent(&self, _id: &str) -> Result<Value> {
                    Ok(json!({}))
                }
            }

            #[tokio::test]
            async fn test_create_parses_agent_payload_into_contract_request() {
                let captured_model_ref = Arc::new(Mutex::new(None));
                let tool = AgentCrudTool::new(Arc::new(CapturingStore {
                    captured_model_ref: captured_model_ref.clone(),
                }))
                .with_write(true);

                let output = tool
                    .execute(json!({
                        "operation": "create",
                        "name": "Agent",
                        "agent": {
                            "model_ref": {
                                "provider": "openai",
                                "model": "gpt-5-mini"
                            }
                        }
                    }))
                    .await
                    .expect("create succeeds");

                assert!(output.success);
                assert_eq!(
                    *captured_model_ref.lock().expect("captured model_ref lock"),
                    Some(Some(types::request::WireModelRef {
                        provider: "openai".to_string(),
                        model: "gpt-5-mini".to_string(),
                    }))
                );
            }

            #[tokio::test]
            async fn test_create_rejects_invalid_agent_payload_before_store_call() {
                let tool = AgentCrudTool::new(Arc::new(MockStore)).with_write(true);
                let err = tool
                    .execute(json!({
                        "operation": "create",
                        "name": "Agent",
                        "agent": {
                            "temperature": "hot"
                        }
                    }))
                    .await
                    .expect_err("invalid payload should be rejected");

                assert!(err.to_string().contains("Invalid agent payload"));
            }
        }
    }

    pub mod config {
        // System configuration tool for AI agents.

        use async_trait::async_trait;
        use serde::Deserialize;
        use serde_json::{Value, json};
        use std::sync::Arc;
        use types::config_types::{CliConfig, ConfigDocument};
        use types::store::ConfigStore;

        use crate::Result;
        use crate::{Tool, ToolError, ToolOutput};

        #[derive(Clone)]
        pub struct ConfigTool {
            store: Arc<dyn ConfigStore>,
            allow_write: bool,
        }

        impl ConfigTool {
            pub fn new(store: Arc<dyn ConfigStore>) -> Self {
                Self {
                    store,
                    allow_write: false,
                }
            }

            pub fn with_write(mut self, allow_write: bool) -> Self {
                self.allow_write = allow_write;
                self
            }

            fn storage_error(error: impl std::fmt::Display) -> ToolError {
                ToolError::Tool(format!(
                    "Config storage error: {error}. The config file may be missing, invalid, or inaccessible. Retry the operation."
                ))
            }

            fn write_guard(&self) -> Result<()> {
                if self.allow_write {
                    Ok(())
                } else {
                    Err(ToolError::Tool(
                        "Write access to config is disabled. Available read-only operations: get, show, list. To modify config, the user must grant write permissions.".to_string(),
                    ))
                }
            }

            fn get_effective_config(&self) -> Result<ConfigDocument> {
                self.store
                    .get_effective_config()
                    .map_err(Self::storage_error)
            }

            fn get_writable_config(&self) -> Result<ConfigDocument> {
                self.store
                    .get_writable_config()
                    .map_err(Self::storage_error)
            }

            fn persist_config(&self, config: &ConfigDocument) -> Result<()> {
                self.store
                    .persist_config(config)
                    .map_err(Self::storage_error)
            }

            fn daemon_view(config: &ConfigDocument) -> Result<Value> {
                let mut encoded = serde_json::to_value(config)?;
                if let Some(object) = encoded.as_object_mut() {
                    object.remove("cli");
                }
                Ok(encoded)
            }

            fn reject_cli_local_config(config: &ConfigDocument) -> Result<()> {
                let default_cli = CliConfig::default();
                let cli = &config.cli;
                let has_cli_overrides = cli.version != default_cli.version
                    || cli.agent.is_some()
                    || cli.model.is_some();
                if has_cli_overrides {
                    return Err(ToolError::Tool(
                        "CLI-local config fields are not available through manage_config. Use the CLI-local config command path for cli.* settings.".to_string(),
                    ));
                }
                Ok(())
            }

            fn reject_cli_section_in_payload(input: &Value) -> Result<()> {
                let has_cli_section = input
                    .get("operation")
                    .and_then(Value::as_str)
                    .is_some_and(|operation| operation == "set")
                    && input
                        .get("config")
                        .and_then(Value::as_object)
                        .is_some_and(|config| config.contains_key("cli"));
                if has_cli_section {
                    return Err(ToolError::Tool(
                        "CLI-local config fields are not available through manage_config. Use the CLI-local config command path for cli.* settings.".to_string(),
                    ));
                }
                Ok(())
            }

            fn apply_update(&self, key: &str, value: &Value) -> Result<ConfigDocument> {
                let mut config = self.get_writable_config()?;
                apply_update(key, value, &mut config)?;
                Ok(config)
            }
        }

        #[async_trait]
        impl Tool for ConfigTool {
            fn name(&self) -> &str {
                "manage_config"
            }

            fn description(&self) -> &str {
                "Read and update runtime configuration values such as workers, retries, and timeouts."
            }

            fn parameters_schema(&self) -> Value {
                parameters_schema()
            }

            async fn execute(&self, input: Value) -> Result<ToolOutput> {
                Self::reject_cli_section_in_payload(&input)?;
                let action: ConfigAction = serde_json::from_value(input)?;

                let output = match action {
                    ConfigAction::Get | ConfigAction::Show => {
                        let config = self.get_effective_config()?;
                        ToolOutput::success(Self::daemon_view(&config)?)
                    }
                    ConfigAction::List => ToolOutput::success(json!({
                        "fields": SUPPORTED_FIELDS,
                    })),
                    ConfigAction::Reset => {
                        self.write_guard()?;
                        let config = self.store.reset_config().map_err(Self::storage_error)?;
                        ToolOutput::success(Self::daemon_view(&config)?)
                    }
                    ConfigAction::Set { config, key, value } => {
                        self.write_guard()?;
                        let updated = if let Some(config) = config {
                            Self::reject_cli_local_config(&config)?;
                            *config
                        } else if let Some(key) = key {
                            let resolved_value = value.unwrap_or(Value::Null);
                            self.apply_update(&key, &resolved_value)?
                        } else {
                            return Ok(ToolOutput::error(
                                "set requires either config or key/value".to_string(),
                            ));
                        };

                        self.persist_config(&updated)?;
                        ToolOutput::success(Self::daemon_view(&updated)?)
                    }
                };

                Ok(output)
            }
        }

        #[derive(Debug, Deserialize)]
        #[serde(tag = "operation", rename_all = "snake_case")]
        enum ConfigAction {
            Get,
            Show,
            List,
            Reset,
            Set {
                #[serde(default)]
                config: Option<Box<ConfigDocument>>,
                #[serde(default)]
                key: Option<String>,
                #[serde(default)]
                value: Option<Value>,
            },
        }

        const SUPPORTED_FIELDS: &[&str] = &[
            "system.worker_count",
            "system.stall_timeout_seconds",
            "system.chat_response_timeout_seconds",
            "system.max_retries",
            "system.chat_session_retention_days",
            "system.log_file_retention_days",
            "system.experimental_features",
            "agent.tool_timeout_secs",
            "agent.llm_timeout_secs",
            "agent.bash_timeout_secs",
            "agent.approval_timeout_secs",
            "agent.max_iterations",
            "agent.max_depth",
            "agent.subagent_timeout_secs",
            "agent.max_parallel_subagents",
            "agent.max_tool_calls",
            "agent.max_tool_concurrency",
            "agent.max_tool_result_length",
            "agent.prune_tool_max_chars",
            "agent.compact_preserve_tokens",
            "agent.max_wall_clock_secs",
            "agent.fallback_models",
            "api.session_list_limit",
            "api.web_search_num_results",
            "runtime.chat_max_session_history",
            "registry.github_cache_ttl_secs",
            "registry.marketplace_cache_ttl_secs",
        ];

        const VALID_TOP_LEVEL_FIELDS: &str = "system.*, agent.*, api.*, runtime.*, registry.*";
        const VALID_AGENT_FIELDS: &str = "agent.tool_timeout_secs, agent.llm_timeout_secs, agent.bash_timeout_secs, agent.approval_timeout_secs, agent.max_iterations, agent.max_depth, agent.subagent_timeout_secs, agent.max_parallel_subagents, agent.max_tool_calls, agent.max_tool_concurrency, agent.max_tool_result_length, agent.prune_tool_max_chars, agent.compact_preserve_tokens, agent.max_wall_clock_secs, agent.fallback_models";
        const VALID_API_FIELDS: &str = "api.session_list_limit, api.web_search_num_results";
        const VALID_RUNTIME_FIELDS: &str = "runtime.chat_max_session_history";
        const VALID_REGISTRY_FIELDS: &str =
            "registry.github_cache_ttl_secs, registry.marketplace_cache_ttl_secs";

        fn parameters_schema() -> Value {
            json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["get", "show", "list", "set", "reset"],
                        "description": "Config operation to perform"
                    },
                    "config": {
                        "type": "object",
                        "description": "Full config object (for set)"
                    },
                    "key": {
                        "type": "string",
                        "description": "Config field to update (for set)"
                    },
                    "value": {
                        "description": "Value for the config field (for set)"
                    }
                },
                "required": ["operation"]
            })
        }

        fn apply_update(key: &str, value: &Value, config: &mut ConfigDocument) -> Result<()> {
            match key {
                "system.worker_count" => {
                    config.system.worker_count = parse_u64(value, key)? as usize;
                }
                "system.stall_timeout_seconds" => {
                    config.system.stall_timeout_seconds = parse_u64(value, key)?;
                }
                "system.chat_response_timeout_seconds" => {
                    config.system.chat_response_timeout_seconds =
                        parse_optional_timeout(value, key)?;
                }
                "system.max_retries" => {
                    config.system.max_retries = parse_u32(value, key)?;
                }
                "system.chat_session_retention_days" => {
                    config.system.chat_session_retention_days = parse_u32(value, key)?;
                }
                "system.log_file_retention_days" => {
                    config.system.log_file_retention_days = parse_u32(value, key)?;
                }
                "system.experimental_features" => {
                    config.system.experimental_features = parse_string_list(value, key)?;
                }
                _ if key.starts_with("system.") => {
                    return Err(unknown_domain_field(
                        "system",
                        key.trim_start_matches("system."),
                        "system.worker_count, system.stall_timeout_seconds, system.chat_response_timeout_seconds, system.max_retries, system.chat_session_retention_days, system.log_file_retention_days, system.experimental_features",
                    ));
                }

                "agent.tool_timeout_secs" => {
                    config.agent.tool_timeout_secs = parse_u64(value, key)?;
                }
                "agent.llm_timeout_secs" => {
                    config.agent.llm_timeout_secs = parse_optional_timeout(value, key)?;
                }
                "agent.bash_timeout_secs" => {
                    config.agent.bash_timeout_secs = parse_u64(value, key)?;
                }
                "agent.approval_timeout_secs" => {
                    config.agent.approval_timeout_secs = parse_u64(value, key)?;
                }
                "agent.max_iterations" => {
                    config.agent.max_iterations = parse_usize(value, key)?;
                }
                "agent.max_depth" => {
                    config.agent.max_depth = parse_usize(value, key)?;
                }
                "agent.subagent_timeout_secs" => {
                    config.agent.subagent_timeout_secs = parse_u64(value, key)?;
                }
                "agent.max_parallel_subagents" => {
                    config.agent.max_parallel_subagents = parse_usize(value, key)?;
                }
                "agent.max_tool_calls" => {
                    config.agent.max_tool_calls = parse_usize(value, key)?;
                }
                "agent.max_tool_concurrency" => {
                    config.agent.max_tool_concurrency = parse_usize(value, key)?;
                }
                "agent.max_tool_result_length" => {
                    config.agent.max_tool_result_length = parse_usize(value, key)?;
                }
                "agent.prune_tool_max_chars" => {
                    config.agent.prune_tool_max_chars = parse_usize(value, key)?;
                }
                "agent.compact_preserve_tokens" => {
                    config.agent.compact_preserve_tokens = parse_usize(value, key)?;
                }
                "agent.max_wall_clock_secs" => {
                    config.agent.max_wall_clock_secs = parse_optional_timeout(value, key)?;
                }
                "agent.fallback_models" => {
                    config.agent.fallback_models = parse_optional_string_list(value, key)?;
                }
                _ if key.starts_with("agent.") => {
                    return Err(unknown_domain_field(
                        "agent",
                        key.trim_start_matches("agent."),
                        VALID_AGENT_FIELDS,
                    ));
                }

                "api.session_list_limit" => {
                    config.api.session_list_limit = parse_u32(value, key)?;
                }
                "api.web_search_num_results" => {
                    config.api.web_search_num_results = parse_usize(value, key)?;
                }
                _ if key.starts_with("api.") => {
                    return Err(unknown_domain_field(
                        "api",
                        key.trim_start_matches("api."),
                        VALID_API_FIELDS,
                    ));
                }

                "runtime.chat_max_session_history" => {
                    config.runtime.chat_max_session_history = parse_usize(value, key)?;
                }
                _ if key.starts_with("runtime.") => {
                    return Err(unknown_domain_field(
                        "runtime",
                        key.trim_start_matches("runtime."),
                        VALID_RUNTIME_FIELDS,
                    ));
                }

                "registry.github_cache_ttl_secs" => {
                    config.registry.github_cache_ttl_secs = parse_u64(value, key)?;
                }
                "registry.marketplace_cache_ttl_secs" => {
                    config.registry.marketplace_cache_ttl_secs = parse_u64(value, key)?;
                }
                _ if key.starts_with("registry.") => {
                    return Err(unknown_domain_field(
                        "registry",
                        key.trim_start_matches("registry."),
                        VALID_REGISTRY_FIELDS,
                    ));
                }

                _ => return Err(unknown_top_level_field(key)),
            }
            Ok(())
        }

        fn parse_u64(value: &Value, key: &str) -> Result<u64> {
            value
                .as_u64()
                .ok_or_else(|| ToolError::Tool(format!("{key} must be a number")))
        }

        fn parse_u32(value: &Value, key: &str) -> Result<u32> {
            Ok(parse_u64(value, key)? as u32)
        }

        fn parse_usize(value: &Value, key: &str) -> Result<usize> {
            Ok(parse_u64(value, key)? as usize)
        }

        fn parse_optional_timeout(value: &Value, key: &str) -> Result<Option<u64>> {
            if value.is_null() {
                return Ok(None);
            }
            value
                .as_u64()
                .map(Some)
                .ok_or_else(|| ToolError::Tool(format!("{key} must be a number or null")))
        }

        fn parse_optional_string_list(value: &Value, key: &str) -> Result<Option<Vec<String>>> {
            if value.is_null() {
                return Ok(None);
            }

            let entries = value.as_array().ok_or_else(|| {
                ToolError::Tool(format!("{key} must be an array of strings or null"))
            })?;

            let mut result = Vec::with_capacity(entries.len());
            for entry in entries {
                let text = entry.as_str().ok_or_else(|| {
                    ToolError::Tool(format!("{key} must be an array of strings or null"))
                })?;
                result.push(text.to_string());
            }

            Ok(Some(result))
        }

        fn parse_string_list(value: &Value, key: &str) -> Result<Vec<String>> {
            let values = value
                .as_array()
                .ok_or_else(|| ToolError::Tool(format!("{key} must be an array of strings")))?;
            let mut result = Vec::with_capacity(values.len());
            for entry in values {
                let text = entry
                    .as_str()
                    .ok_or_else(|| ToolError::Tool(format!("{key} must be an array of strings")))?;
                result.push(text.to_string());
            }
            Ok(result)
        }

        fn unknown_top_level_field(key: &str) -> ToolError {
            ToolError::Tool(format!(
                "Unknown config field: '{key}'. Valid fields: {VALID_TOP_LEVEL_FIELDS}."
            ))
        }

        fn unknown_domain_field(domain: &str, field: &str, valid_fields: &str) -> ToolError {
            ToolError::Tool(format!(
                "Unknown {domain} config field: '{domain}.{field}'. Valid {domain} fields: {valid_fields}."
            ))
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use std::sync::{Arc, RwLock};
            use types::config_types::{CliConfig, ConfigDocument, SystemConfig};
            use types::store::ConfigStore;

            struct TestContext {
                store: Arc<dyn ConfigStore>,
            }

            struct TestConfigStore {
                config: Arc<RwLock<ConfigDocument>>,
            }

            impl TestConfigStore {
                fn new(config: ConfigDocument) -> Self {
                    Self {
                        config: Arc::new(RwLock::new(config)),
                    }
                }
            }

            fn config_error(e: impl std::fmt::Display) -> types::ToolError {
                types::ToolError::Tool(format!("Config storage error: {e}"))
            }

            impl ConfigStore for TestConfigStore {
                fn get_effective_config(&self) -> types::error::Result<ConfigDocument> {
                    self.config
                        .read()
                        .map_err(config_error)
                        .map(|config| config.clone())
                }

                fn get_writable_config(&self) -> types::error::Result<ConfigDocument> {
                    self.get_effective_config()
                }

                fn persist_config(&self, config: &ConfigDocument) -> types::error::Result<()> {
                    *self.config.write().map_err(config_error)? = config.clone();
                    Ok(())
                }

                fn reset_config(&self) -> types::error::Result<ConfigDocument> {
                    let doc = ConfigDocument::from_system_config(
                        SystemConfig::default(),
                        CliConfig::default(),
                    );
                    self.persist_config(&doc)?;
                    Ok(doc)
                }
            }

            fn default_config_document() -> ConfigDocument {
                ConfigDocument::from_system_config(SystemConfig::default(), CliConfig::default())
            }

            fn setup_storage() -> TestContext {
                let store: Arc<dyn ConfigStore> =
                    Arc::new(TestConfigStore::new(default_config_document()));
                TestContext { store }
            }

            #[tokio::test]
            async fn test_get_config() {
                let ctx = setup_storage();
                let tool = ConfigTool::new(ctx.store);

                let output = tool.execute(json!({ "operation": "get" })).await.unwrap();
                assert!(output.success);
                assert!(
                    output
                        .result
                        .pointer("/system/worker_count")
                        .and_then(|value| value.as_u64())
                        .is_some()
                );
                assert!(
                    output.result.get("cli").is_none(),
                    "daemon-facing config view must omit cli-local settings"
                );
            }

            #[tokio::test]
            async fn test_set_requires_write() {
                let ctx = setup_storage();
                let tool = ConfigTool::new(ctx.store);

                let result = tool
                    .execute(json!({
                        "operation": "set",
                        "key": "system.worker_count",
                        "value": 8
                    }))
                    .await;
                let err = result.expect_err("expected write-guard error");
                assert!(
                    err.to_string()
                        .contains("Available read-only operations: get, show, list")
                );
            }

            #[tokio::test]
            async fn test_set_rejects_unknown_field_with_valid_fields_hint() {
                let ctx = setup_storage();
                let tool = ConfigTool::new(ctx.store).with_write(true);

                let result = tool
                    .execute(json!({
                        "operation": "set",
                        "key": "invalid_field",
                        "value": 8
                    }))
                    .await;
                let err = result.expect_err("expected unknown field error");
                let message = err.to_string();

                assert!(message.contains("Unknown config field: 'invalid_field'"));
                assert!(
                    message
                        .contains("Valid fields: system.*, agent.*, api.*, runtime.*, registry.*.")
                );
            }

            #[tokio::test]
            async fn test_set_experimental_features() {
                let ctx = setup_storage();
                let tool = ConfigTool::new(ctx.store).with_write(true);

                let output = tool
                    .execute(json!({
                        "operation": "set",
                        "key": "system.experimental_features",
                        "value": ["plan_mode", "websocket_transport"]
                    }))
                    .await
                    .unwrap();
                assert!(output.success);
                let values = output
                    .result
                    .pointer("/system/experimental_features")
                    .and_then(|value| value.as_array())
                    .expect("experimental_features should be an array");
                assert_eq!(values.len(), 2);
            }

            #[tokio::test]
            async fn test_set_optional_timeout_with_null() {
                let ctx = setup_storage();
                let tool = ConfigTool::new(ctx.store).with_write(true);

                let output = tool
                    .execute(json!({
                        "operation": "set",
                        "key": "system.chat_response_timeout_seconds",
                        "value": null
                    }))
                    .await
                    .unwrap();
                assert!(output.success);
                assert!(
                    output
                        .result
                        .pointer("/system/chat_response_timeout_seconds")
                        .is_some_and(|v| v.is_null())
                );
            }

            #[tokio::test]
            async fn test_set_agent_max_wall_clock_with_null() {
                let ctx = setup_storage();
                let tool = ConfigTool::new(ctx.store).with_write(true);

                let output = tool
                    .execute(json!({
                        "operation": "set",
                        "key": "agent.max_wall_clock_secs",
                        "value": null
                    }))
                    .await
                    .unwrap();
                assert!(output.success);
                let agent = output
                    .result
                    .get("agent")
                    .expect("agent block should exist");
                assert!(
                    agent
                        .get("max_wall_clock_secs")
                        .is_some_and(|v| v.is_null())
                );
            }

            #[tokio::test]
            async fn test_list_supported_fields() {
                let ctx = setup_storage();
                let tool = ConfigTool::new(ctx.store);

                let output = tool.execute(json!({ "operation": "list" })).await.unwrap();
                assert!(output.success);
                let fields = output
                    .result
                    .get("fields")
                    .and_then(|v| v.as_array())
                    .expect("fields should be an array");
                assert!(
                    fields
                        .iter()
                        .any(|field| field.as_str() == Some("system.log_file_retention_days")),
                    "list should expose system.log_file_retention_days"
                );
            }

            #[tokio::test]
            async fn test_listed_retention_fields_are_settable() {
                let ctx = setup_storage();
                let tool = ConfigTool::new(ctx.store).with_write(true);

                let updates = [
                    ("system.chat_session_retention_days", json!(0)),
                    ("system.log_file_retention_days", json!(30)),
                ];

                for (key, value) in updates {
                    let output = tool
                        .execute(json!({
                            "operation": "set",
                            "key": key,
                            "value": value
                        }))
                        .await
                        .unwrap_or_else(|err| {
                            panic!("set should support listed field '{key}': {err}")
                        });
                    assert!(
                        output.success,
                        "set should succeed for listed field '{key}'"
                    );
                }
            }

            #[tokio::test]
            async fn test_set_agent_defaults() {
                let ctx = setup_storage();
                let tool = ConfigTool::new(ctx.store).with_write(true);

                let updates = [
                    ("agent.tool_timeout_secs", json!(180)),
                    ("agent.llm_timeout_secs", json!(900)),
                    ("agent.bash_timeout_secs", json!(600)),
                    ("agent.approval_timeout_secs", json!(420)),
                    ("agent.max_iterations", json!(50)),
                    ("agent.max_depth", json!(4)),
                    ("agent.subagent_timeout_secs", json!(900)),
                    ("agent.max_parallel_subagents", json!(12)),
                    ("agent.max_tool_calls", json!(300)),
                    ("agent.max_tool_concurrency", json!(24)),
                    ("agent.max_tool_result_length", json!(8192)),
                    ("agent.prune_tool_max_chars", json!(4096)),
                    ("agent.compact_preserve_tokens", json!(16000)),
                    ("agent.max_wall_clock_secs", json!(3600)),
                    ("agent.fallback_models", json!(["glm-5", "gpt-5"])),
                ];

                for (key, value) in updates {
                    let output = tool
                        .execute(json!({
                            "operation": "set",
                            "key": key,
                            "value": value
                        }))
                        .await
                        .unwrap_or_else(|err| {
                            panic!("set should support agent field '{key}': {err}")
                        });
                    assert!(output.success, "set should succeed for agent field '{key}'");
                }

                // Verify the values persisted
                let output = tool.execute(json!({ "operation": "get" })).await.unwrap();
                let agent = output
                    .result
                    .get("agent")
                    .expect("agent block should exist");
                assert_eq!(
                    agent.get("tool_timeout_secs").and_then(|v| v.as_u64()),
                    Some(180)
                );
                assert_eq!(
                    agent.get("llm_timeout_secs").and_then(|v| v.as_u64()),
                    Some(900)
                );
                assert_eq!(
                    agent.get("bash_timeout_secs").and_then(|v| v.as_u64()),
                    Some(600)
                );
                assert_eq!(
                    agent.get("max_iterations").and_then(|v| v.as_u64()),
                    Some(50)
                );
                assert_eq!(agent.get("max_depth").and_then(|v| v.as_u64()), Some(4));
                assert_eq!(
                    agent.get("approval_timeout_secs").and_then(|v| v.as_u64()),
                    Some(420)
                );
                assert_eq!(
                    agent.get("max_parallel_subagents").and_then(|v| v.as_u64()),
                    Some(12)
                );
                assert_eq!(
                    agent.get("max_tool_concurrency").and_then(|v| v.as_u64()),
                    Some(24)
                );
                assert_eq!(
                    agent.get("max_tool_result_length").and_then(|v| v.as_u64()),
                    Some(8192)
                );
                assert_eq!(
                    agent.get("prune_tool_max_chars").and_then(|v| v.as_u64()),
                    Some(4096)
                );
                assert_eq!(
                    agent
                        .get("compact_preserve_tokens")
                        .and_then(|v| v.as_u64()),
                    Some(16000)
                );
                assert_eq!(
                    agent.get("fallback_models"),
                    Some(&json!(["glm-5", "gpt-5"]))
                );
            }

            #[tokio::test]
            async fn test_set_agent_unknown_field() {
                let ctx = setup_storage();
                let tool = ConfigTool::new(ctx.store).with_write(true);

                let result = tool
                    .execute(json!({
                        "operation": "set",
                        "key": "agent.nonexistent",
                        "value": 42
                    }))
                    .await;
                let err = result.expect_err("expected unknown agent field error");
                assert!(err.to_string().contains("Unknown agent config field"));
            }

            #[tokio::test]
            async fn test_get_includes_agent_defaults() {
                let ctx = setup_storage();
                let tool = ConfigTool::new(ctx.store);

                let output = tool.execute(json!({ "operation": "get" })).await.unwrap();
                assert!(output.success);
                let agent = output
                    .result
                    .get("agent")
                    .expect("agent block should exist");
                assert!(agent.get("tool_timeout_secs").is_some());
                assert!(agent.get("llm_timeout_secs").is_some());
                assert!(agent.get("bash_timeout_secs").is_some());
                assert!(agent.get("approval_timeout_secs").is_some());
                assert!(agent.get("max_iterations").is_some());
                assert!(agent.get("max_tool_concurrency").is_some());
                assert!(agent.get("max_tool_result_length").is_some());
                assert!(agent.get("prune_tool_max_chars").is_some());
                assert!(agent.get("compact_preserve_tokens").is_some());
                assert!(agent.get("fallback_models").is_some());
            }

            #[tokio::test]
            async fn test_set_runtime_and_registry_defaults() {
                let ctx = setup_storage();
                let tool = ConfigTool::new(ctx.store).with_write(true);

                let updates = [
                    ("runtime.chat_max_session_history", json!(40)),
                    ("registry.github_cache_ttl_secs", json!(900)),
                    ("registry.marketplace_cache_ttl_secs", json!(450)),
                ];

                for (key, value) in updates {
                    let output = tool
                        .execute(json!({
                            "operation": "set",
                            "key": key,
                            "value": value
                        }))
                        .await
                        .unwrap_or_else(|err| {
                            panic!("set should support config field '{key}': {err}")
                        });
                    assert!(
                        output.success,
                        "set should succeed for config field '{key}'"
                    );
                }

                let output = tool.execute(json!({ "operation": "get" })).await.unwrap();
                assert_eq!(
                    output
                        .result
                        .pointer("/runtime/chat_max_session_history")
                        .and_then(|value| value.as_u64()),
                    Some(40)
                );
                assert_eq!(
                    output
                        .result
                        .pointer("/registry/github_cache_ttl_secs")
                        .and_then(|value| value.as_u64()),
                    Some(900)
                );
                assert_eq!(
                    output
                        .result
                        .pointer("/registry/marketplace_cache_ttl_secs")
                        .and_then(|value| value.as_u64()),
                    Some(450)
                );
            }

            #[tokio::test]
            async fn test_set_agent_fallback_models_allows_null_clear() {
                let ctx = setup_storage();
                let tool = ConfigTool::new(ctx.store).with_write(true);

                tool.execute(json!({
                    "operation": "set",
                    "key": "agent.fallback_models",
                    "value": ["glm-5", "gpt-5"]
                }))
                .await
                .expect("initial fallback_models set should succeed");

                let output = tool
                    .execute(json!({
                        "operation": "set",
                        "key": "agent.fallback_models",
                        "value": null
                    }))
                    .await
                    .expect("clearing fallback_models should succeed");

                assert!(output.success);
                let agent = output
                    .result
                    .get("agent")
                    .expect("agent block should exist");
                assert!(
                    agent
                        .get("fallback_models")
                        .is_some_and(|value| value.is_null())
                );
            }

            #[tokio::test]
            async fn test_set_rejects_cli_fields() {
                let ctx = setup_storage();
                let tool = ConfigTool::new(ctx.store).with_write(true);

                let err = tool
                    .execute(json!({
                        "operation": "set",
                        "key": "cli.model",
                        "value": "gpt-5"
                    }))
                    .await
                    .expect_err("cli fields should be rejected by daemon-facing config");

                assert!(
                    err.to_string()
                        .contains("Unknown config field: 'cli.model'")
                );
            }

            #[tokio::test]
            async fn test_set_rejects_cli_block_in_full_config_payload() {
                let ctx = setup_storage();
                let tool = ConfigTool::new(ctx.store).with_write(true);

                let err = tool
                    .execute(json!({
                        "operation": "set",
                        "config": {
                            "system": {
                                "worker_count": 6
                            },
                            "cli": {
                                "model": "gpt-5"
                            }
                        }
                    }))
                    .await
                    .expect_err("cli block should be rejected for daemon-facing config writes");

                assert!(
                    err.to_string().contains(
                        "CLI-local config fields are not available through manage_config"
                    )
                );
            }

            #[tokio::test]
            async fn test_set_rejects_default_cli_block_in_full_config_payload() {
                let ctx = setup_storage();
                let tool = ConfigTool::new(ctx.store).with_write(true);

                let err = tool
                    .execute(json!({
                        "operation": "set",
                        "config": {
                            "system": {
                                "worker_count": 6
                            },
                            "cli": {}
                        }
                    }))
                    .await
                    .expect_err("default cli block should still be rejected");

                assert!(
                    err.to_string().contains(
                        "CLI-local config fields are not available through manage_config"
                    )
                );
            }

            #[tokio::test]
            async fn test_set_api_defaults() {
                let ctx = setup_storage();
                let tool = ConfigTool::new(ctx.store).with_write(true);

                let updates = [
                    ("api.session_list_limit", json!(30)),
                    ("api.web_search_num_results", json!(6)),
                ];

                for (key, value) in updates {
                    let output = tool
                        .execute(json!({
                            "operation": "set",
                            "key": key,
                            "value": value
                        }))
                        .await
                        .unwrap_or_else(|err| {
                            panic!("set should support api field '{key}': {err}")
                        });
                    assert!(output.success, "set should succeed for api field '{key}'");
                }

                let output = tool.execute(json!({ "operation": "get" })).await.unwrap();
                let api = output.result.get("api").expect("api block should exist");
                assert_eq!(
                    api.get("web_search_num_results").and_then(|v| v.as_u64()),
                    Some(6)
                );
            }
        }
    }

    pub mod file_tracker {
        // File read/write tracking for external modification detection.

        use parking_lot::RwLock;
        use std::collections::HashMap;
        use std::io;
        use std::path::{Path, PathBuf};
        use std::time::SystemTime;

        use tokio::fs;

        #[derive(Debug, Default)]
        pub struct FileTracker {
            records: RwLock<HashMap<PathBuf, FileRecord>>,
        }

        #[derive(Debug, Clone)]
        struct FileRecord {
            last_read: SystemTime,
            last_write: Option<SystemTime>,
        }

        impl FileTracker {
            pub fn new() -> Self {
                Self {
                    records: RwLock::new(HashMap::new()),
                }
            }

            /// Record that we read a file.
            pub fn record_read(&self, path: &Path) {
                let mut records = self.records.write();
                let entry = records.entry(path.to_path_buf()).or_insert(FileRecord {
                    last_read: SystemTime::UNIX_EPOCH,
                    last_write: None,
                });
                entry.last_read = SystemTime::now();
            }

            /// Record that we wrote a file.
            pub fn record_write(&self, path: &Path) {
                let mut records = self.records.write();
                let entry = records.entry(path.to_path_buf()).or_insert(FileRecord {
                    last_read: SystemTime::UNIX_EPOCH,
                    last_write: None,
                });
                entry.last_write = Some(SystemTime::now());
            }

            /// Check if a file has been read at least once.
            pub fn has_been_read(&self, path: &Path) -> bool {
                let records = self.records.read();
                records
                    .get(path)
                    .is_some_and(|record| record.last_read > SystemTime::UNIX_EPOCH)
            }

            /// Check if file was modified externally since last read.
            pub async fn check_external_modification(&self, path: &Path) -> io::Result<bool> {
                let (last_read, last_write) = {
                    let records = self.records.read();
                    let Some(record) = records.get(path) else {
                        return Ok(false);
                    };
                    (record.last_read, record.last_write)
                };

                let metadata = match fs::metadata(path).await {
                    Ok(metadata) => metadata,
                    Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
                    Err(err) => return Err(err),
                };

                let modified = metadata.modified()?;
                if modified <= last_read {
                    return Ok(false);
                }

                if let Some(last_write) = last_write {
                    Ok(modified > last_write)
                } else {
                    Ok(true)
                }
            }

            /// Get last read time for a file.
            pub fn last_read(&self, path: &Path) -> Option<SystemTime> {
                let records = self.records.read();
                records.get(path).map(|record| record.last_read)
            }
        }

        #[cfg(test)]
        mod tests {
            use super::FileTracker;
            use std::path::Path;

            #[test]
            fn has_been_read_returns_false_for_untracked_path() {
                let tracker = FileTracker::new();
                assert!(!tracker.has_been_read(Path::new("/tmp/untracked.txt")));
            }

            #[test]
            fn has_been_read_returns_true_after_record_read() {
                let tracker = FileTracker::new();
                let path = Path::new("/tmp/tracked.txt");
                tracker.record_read(path);
                assert!(tracker.has_been_read(path));
            }

            #[test]
            fn has_been_read_returns_false_after_write_only() {
                let tracker = FileTracker::new();
                let path = Path::new("/tmp/write-only.txt");
                tracker.record_write(path);
                assert!(!tracker.has_been_read(path));
            }
        }
    }

    pub mod patch {
        use std::iter::Peekable;
        use std::path::{Path, PathBuf};
        use std::str::Lines;
        use std::sync::Arc;

        use anyhow::{Result as AnyResult, anyhow};
        use async_trait::async_trait;
        use serde_json::Value;
        use tokio::fs;

        use super::file_tracker::FileTracker;
        use crate::Result;
        use crate::{Tool, ToolOutput};

        #[derive(Debug, Clone)]
        pub struct PatchTool {
            base_dir: Option<PathBuf>,
            require_base_dir: bool,
            tracker: Arc<FileTracker>,
        }

        impl PatchTool {
            pub fn new(tracker: Arc<FileTracker>) -> Self {
                Self {
                    base_dir: None,
                    require_base_dir: false,
                    tracker,
                }
            }

            pub fn with_base_dir(mut self, base_dir: impl Into<PathBuf>) -> Self {
                self.base_dir = Some(base_dir.into());
                self
            }

            pub fn require_base_dir(mut self) -> Self {
                self.require_base_dir = true;
                self
            }

            fn resolve_path(&self, path: &str) -> std::result::Result<PathBuf, String> {
                crate::impls::path_utils::resolve_path_with_policy(
                    path,
                    self.base_dir.as_deref(),
                    self.require_base_dir,
                )
            }
        }

        #[derive(Debug, Clone)]
        enum PatchOperation {
            Update { path: String, hunks: Vec<Hunk> },
            Add { path: String, content: String },
            Delete { path: String },
        }

        #[derive(Debug, Clone, PartialEq, Eq)]
        struct Hunk {
            context_before: Vec<String>,
            removals: Vec<String>,
            additions: Vec<String>,
            context_after: Vec<String>,
        }

        fn parse_patch(text: &str) -> AnyResult<Vec<PatchOperation>> {
            let mut operations = Vec::new();
            let mut lines = text.lines().peekable();

            while let Some(line) = lines.next() {
                if let Some(path) = line.strip_prefix("*** Update File: ") {
                    let block = collect_block(&mut lines);
                    let hunks = parse_hunks(&block)?;
                    operations.push(PatchOperation::Update {
                        path: path.trim().to_string(),
                        hunks,
                    });
                    continue;
                }

                if let Some(path) = line.strip_prefix("*** Add File: ") {
                    let block = collect_block(&mut lines);
                    let content = parse_added_content(&block);
                    operations.push(PatchOperation::Add {
                        path: path.trim().to_string(),
                        content,
                    });
                    continue;
                }

                if let Some(path) = line.strip_prefix("*** Delete File: ") {
                    operations.push(PatchOperation::Delete {
                        path: path.trim().to_string(),
                    });
                    continue;
                }
            }

            if operations.is_empty() {
                return Err(anyhow!("No valid patch operations found"));
            }

            Ok(operations)
        }

        fn collect_block(lines: &mut Peekable<Lines<'_>>) -> Vec<String> {
            let mut block = Vec::new();
            while let Some(line) = lines.peek() {
                if line.starts_with("*** ") {
                    break;
                }
                block.push(lines.next().unwrap_or_default().to_string());
            }
            block
        }

        fn parse_hunks(block: &[String]) -> AnyResult<Vec<Hunk>> {
            if block.is_empty() {
                return Err(anyhow!("Update block is empty"));
            }

            if block.iter().any(|line| line.starts_with("@@")) {
                return parse_unified_hunks(block);
            }

            let mut hunks = Vec::new();
            let mut start = 0;

            for (idx, line) in block.iter().enumerate() {
                if line.trim() == "---" {
                    if start < idx {
                        hunks.push(parse_hunk_lines(&block[start..idx])?);
                    }
                    start = idx + 1;
                }
            }

            if start < block.len() {
                hunks.push(parse_hunk_lines(&block[start..])?);
            }

            if hunks.is_empty() {
                return Err(anyhow!("No hunks found in update block"));
            }

            Ok(hunks)
        }

        fn parse_hunk_lines(lines: &[String]) -> AnyResult<Hunk> {
            let mut context_before = Vec::new();
            let mut removals = Vec::new();
            let mut additions = Vec::new();
            let mut context_after = Vec::new();

            let mut in_changes = false;
            let mut finished_changes = false;

            for line in lines {
                if let Some(stripped) = line.strip_prefix('-') {
                    if finished_changes {
                        return Err(anyhow!("Change lines must be contiguous"));
                    }
                    in_changes = true;
                    removals.push(stripped.to_string());
                    continue;
                }

                if let Some(stripped) = line.strip_prefix('+') {
                    if finished_changes {
                        return Err(anyhow!("Change lines must be contiguous"));
                    }
                    in_changes = true;
                    additions.push(stripped.to_string());
                    continue;
                }

                if !in_changes {
                    context_before.push(line.to_string());
                } else {
                    finished_changes = true;
                    context_after.push(line.to_string());
                }
            }

            if removals.is_empty() && additions.is_empty() {
                return Err(anyhow!("Hunk has no changes"));
            }

            Ok(Hunk {
                context_before,
                removals,
                additions,
                context_after,
            })
        }

        fn parse_unified_hunks(block: &[String]) -> AnyResult<Vec<Hunk>> {
            let mut hunks = Vec::new();
            let mut current = Vec::new();
            let mut saw_header = false;

            for line in block {
                if line.starts_with("--- ") || line.starts_with("+++ ") {
                    continue;
                }
                if line.starts_with("@@") {
                    if !current.is_empty() {
                        hunks.push(parse_unified_hunk_lines(&current)?);
                        current.clear();
                    }
                    saw_header = true;
                    continue;
                }
                if saw_header {
                    current.push(line.clone());
                }
            }

            if !current.is_empty() {
                hunks.push(parse_unified_hunk_lines(&current)?);
            }

            if hunks.is_empty() {
                return Err(anyhow!("No hunks found in update block"));
            }

            Ok(hunks)
        }

        fn parse_unified_hunk_lines(lines: &[String]) -> AnyResult<Hunk> {
            let mut lines = lines;
            while let Some((last, rest)) = lines.split_last()
                && last.is_empty()
            {
                lines = rest;
            }
            let normalized = lines
                .iter()
                .map(|line| {
                    line.strip_prefix(' ')
                        .map(ToOwned::to_owned)
                        .unwrap_or_else(|| line.to_string())
                })
                .collect::<Vec<_>>();
            parse_hunk_lines(&normalized)
        }

        fn parse_added_content(lines: &[String]) -> String {
            if lines.iter().any(|line| line.starts_with("@@")) {
                return parse_unified_added_content(lines);
            }
            let mut content_lines = Vec::new();
            for line in lines {
                if let Some(stripped) = line.strip_prefix('+') {
                    content_lines.push(stripped.to_string());
                } else {
                    content_lines.push(line.to_string());
                }
            }
            content_lines.join("\n")
        }

        fn parse_unified_added_content(lines: &[String]) -> String {
            let mut content_lines = Vec::new();
            let mut saw_header = false;

            for line in lines {
                if line.starts_with("--- ") || line.starts_with("+++ ") {
                    continue;
                }
                if line.starts_with("@@") {
                    saw_header = true;
                    continue;
                }
                if !saw_header {
                    continue;
                }
                if let Some(stripped) = line.strip_prefix('+') {
                    content_lines.push(stripped.to_string());
                } else if let Some(stripped) = line.strip_prefix(' ') {
                    content_lines.push(stripped.to_string());
                }
            }

            content_lines.join("\n")
        }

        fn apply_hunks(original: &str, hunks: &[Hunk]) -> AnyResult<String> {
            let mut lines: Vec<String> = original.lines().map(|line| line.to_string()).collect();

            for hunk in hunks {
                let position = find_hunk_position(&lines, hunk)?;
                let remove_count =
                    hunk.context_before.len() + hunk.removals.len() + hunk.context_after.len();
                let mut new_lines = Vec::new();
                new_lines.extend(hunk.context_before.iter().cloned());
                new_lines.extend(hunk.additions.iter().cloned());
                new_lines.extend(hunk.context_after.iter().cloned());

                lines.splice(position..position + remove_count, new_lines);
            }

            Ok(lines.join("\n"))
        }

        fn find_hunk_position(lines: &[String], hunk: &Hunk) -> AnyResult<usize> {
            let mut search_lines: Vec<&str> = Vec::new();
            search_lines.extend(hunk.context_before.iter().map(String::as_str));
            search_lines.extend(hunk.removals.iter().map(String::as_str));
            search_lines.extend(hunk.context_after.iter().map(String::as_str));

            if search_lines.is_empty() {
                return Err(anyhow!("Hunk has no searchable context"));
            }
            if search_lines.len() > lines.len() {
                return Err(anyhow!("Could not find matching context for hunk"));
            }

            let mut first_match: Option<usize> = None;

            for i in 0..=lines.len() - search_lines.len() {
                let mut matched = true;
                for (offset, expected) in search_lines.iter().enumerate() {
                    if lines[i + offset] != *expected {
                        matched = false;
                        break;
                    }
                }
                if matched {
                    if first_match.is_some() {
                        return Err(anyhow!(
                            "Ambiguous patch: hunk matches at multiple locations. Add more context lines to disambiguate."
                        ));
                    }
                    first_match = Some(i);
                }
            }

            first_match.ok_or_else(|| anyhow!("Could not find matching context for hunk"))
        }

        #[async_trait]
        impl Tool for PatchTool {
            fn name(&self) -> &str {
                "patch"
            }

            fn description(&self) -> &str {
                "Apply structured multi-file patches (add, update, delete) in one operation."
            }

            fn parameters_schema(&self) -> Value {
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "patch": {
                            "type": "string",
                            "description": "Patch text using *** Update/Add/Delete File headers. Update blocks accept either simple context/-old/+new lines or unified diff hunks with @@ headers."
                        }
                    },
                    "required": ["patch"]
                })
            }

            async fn execute(&self, input: Value) -> Result<ToolOutput> {
                let patch_text = match input.get("patch").and_then(|value| value.as_str()) {
                    Some(value) => value,
                    None => return Ok(ToolOutput::error("patch is required")),
                };

                let operations = match parse_patch(patch_text) {
                    Ok(ops) => ops,
                    Err(err) => return Ok(ToolOutput::error(err.to_string())),
                };

                match self.apply_operations(&operations).await {
                    Ok(results) => Ok(ToolOutput::success(serde_json::json!({
                        "results": results
                    }))),
                    Err(err) => Ok(ToolOutput::error(err.to_string())),
                }
            }
        }

        impl PatchTool {
            async fn apply_operations(
                &self,
                operations: &[PatchOperation],
            ) -> AnyResult<Vec<String>> {
                let mut staged: Vec<StagedOperation> = Vec::new();

                for operation in operations {
                    match operation {
                        PatchOperation::Update { path, hunks } => {
                            let resolved = self.resolve_path(path).map_err(|err| anyhow!(err))?;
                            self.ensure_file_exists(&resolved)?;
                            if !self.tracker.has_been_read(&resolved) {
                                return Err(anyhow!(
                                    "File {} has not been read. Read it before patching.",
                                    resolved.display()
                                ));
                            }
                            if self.tracker.check_external_modification(&resolved).await? {
                                return Err(anyhow!(
                                    "File {} modified externally. Read it first.",
                                    resolved.display()
                                ));
                            }
                            let original = fs::read_to_string(&resolved).await?;
                            let patched = apply_hunks(&original, hunks)?;
                            staged.push(StagedOperation::Update {
                                path: resolved,
                                original,
                                patched,
                            });
                        }
                        PatchOperation::Add { path, content } => {
                            let resolved = self.resolve_path(path).map_err(|err| anyhow!(err))?;
                            if resolved.exists() {
                                return Err(anyhow!("File already exists: {}", resolved.display()));
                            }
                            staged.push(StagedOperation::Add {
                                path: resolved,
                                content: content.to_string(),
                            });
                        }
                        PatchOperation::Delete { path } => {
                            let resolved = self.resolve_path(path).map_err(|err| anyhow!(err))?;
                            if !self.tracker.has_been_read(&resolved) {
                                return Err(anyhow!(
                                    "File {} has not been read. Read it before deleting.",
                                    resolved.display()
                                ));
                            }
                            self.ensure_file_exists(&resolved)?;
                            if self.tracker.check_external_modification(&resolved).await? {
                                return Err(anyhow!(
                                    "File {} modified externally. Read it first.",
                                    resolved.display()
                                ));
                            }
                            let original = fs::read_to_string(&resolved).await?;
                            staged.push(StagedOperation::Delete {
                                path: resolved,
                                original,
                            });
                        }
                    }
                }

                let mut backups = Vec::new();
                let mut results = Vec::new();

                for op in &staged {
                    let apply_result: AnyResult<()> = match op {
                        StagedOperation::Update {
                            path,
                            original,
                            patched,
                        } => {
                            backups.push(Backup {
                                path: path.clone(),
                                original: Some(original.clone()),
                            });
                            match fs::write(path, patched).await {
                                Ok(()) => {
                                    self.tracker.record_write(path);
                                    results.push(format!("Updated: {}", path.display()));
                                    Ok(())
                                }
                                Err(err) => Err(err.into()),
                            }
                        }
                        StagedOperation::Add { path, content } => {
                            let create_result: AnyResult<()> = if let Some(parent) = path.parent() {
                                fs::create_dir_all(parent).await.map_err(|err| err.into())
                            } else {
                                Ok(())
                            };

                            if let Err(err) = create_result {
                                Err(err)
                            } else {
                                backups.push(Backup {
                                    path: path.clone(),
                                    original: None,
                                });
                                match fs::write(path, content).await {
                                    Ok(()) => {
                                        self.tracker.record_write(path);
                                        results.push(format!("Created: {}", path.display()));
                                        Ok(())
                                    }
                                    Err(err) => Err(err.into()),
                                }
                            }
                        }
                        StagedOperation::Delete { path, original } => {
                            backups.push(Backup {
                                path: path.clone(),
                                original: Some(original.clone()),
                            });
                            match fs::remove_file(path).await {
                                Ok(()) => {
                                    self.tracker.record_write(path);
                                    results.push(format!("Deleted: {}", path.display()));
                                    Ok(())
                                }
                                Err(err) => Err(err.into()),
                            }
                        }
                    };

                    if let Err(err) = apply_result {
                        rollback(&backups).await;
                        return Err(err);
                    }
                }

                Ok(results)
            }

            fn ensure_file_exists(&self, path: &Path) -> AnyResult<()> {
                if !path.exists() {
                    return Err(anyhow!("File not found: {}", path.display()));
                }
                if !path.is_file() {
                    return Err(anyhow!("Not a file: {}", path.display()));
                }
                Ok(())
            }
        }

        #[derive(Debug, Clone)]
        enum StagedOperation {
            Update {
                path: PathBuf,
                original: String,
                patched: String,
            },
            Add {
                path: PathBuf,
                content: String,
            },
            Delete {
                path: PathBuf,
                original: String,
            },
        }

        #[derive(Debug, Clone)]
        struct Backup {
            path: PathBuf,
            original: Option<String>,
        }

        async fn rollback(backups: &[Backup]) {
            for backup in backups.iter().rev() {
                match &backup.original {
                    Some(content) => {
                        let _ = fs::write(&backup.path, content).await;
                    }
                    None => {
                        let _ = fs::remove_file(&backup.path).await;
                    }
                }
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use crate::Tool;

            #[tokio::test]
            async fn apply_operations_add_update_delete() {
                let temp_dir = tempfile::TempDir::new().unwrap();
                let tracker = Arc::new(FileTracker::new());
                let tool = PatchTool::new(tracker).with_base_dir(temp_dir.path());

                let file_path = temp_dir.path().join("example.txt");
                fs::write(&file_path, "line1\nline2\nline3").await.unwrap();
                let resolved = tool.resolve_path("example.txt").unwrap();
                tool.tracker.record_read(&resolved);

                let patch = "*** Update File: example.txt\nline1\n-line2\n+line2b\nline3\n*** Add File: new.txt\n+hello\n+world\n*** Delete File: example.txt";
                let operations = parse_patch(patch).unwrap();
                let result = tool.apply_operations(&operations).await;
                assert!(result.is_ok());
            }

            #[tokio::test]
            async fn apply_operations_update_requires_read_first() {
                let temp_dir = tempfile::TempDir::new().unwrap();
                let tracker = Arc::new(FileTracker::new());
                let tool = PatchTool::new(tracker).with_base_dir(temp_dir.path());

                let file_path = temp_dir.path().join("example.txt");
                fs::write(&file_path, "line1\nline2\nline3").await.unwrap();

                let patch = "*** Update File: example.txt\nline1\n-line2\n+line2b\nline3";
                let operations = parse_patch(patch).unwrap();
                let result = tool.apply_operations(&operations).await;

                assert!(result.is_err());
                assert!(
                    result
                        .err()
                        .unwrap()
                        .to_string()
                        .contains("has not been read")
                );
            }

            #[tokio::test]
            async fn patch_escape_error_includes_path_and_base_dir() {
                let temp_dir = tempfile::TempDir::new().unwrap();
                let tracker = Arc::new(FileTracker::new());
                let tool = PatchTool::new(tracker).with_base_dir(temp_dir.path());

                let output = tool
                    .execute(serde_json::json!({
                        "patch": "*** Add File: ../outside.txt\n+blocked"
                    }))
                    .await
                    .unwrap();

                assert!(!output.success);
                let error = output.error.unwrap();
                assert!(error.contains("escapes allowed base directory"));
                assert!(error.contains(temp_dir.path().display().to_string().as_str()));
            }
        }

        #[tokio::test]
        async fn apply_operations_delete_requires_read_first() {
            let temp_dir = tempfile::TempDir::new().unwrap();
            let tracker = Arc::new(FileTracker::new());
            let tool = PatchTool::new(tracker).with_base_dir(temp_dir.path());

            let file_path = temp_dir.path().join("to_delete.txt");
            fs::write(&file_path, "content").await.unwrap();

            // Delete without read first should fail
            let patch = "*** Delete File: to_delete.txt";
            let operations = parse_patch(patch).unwrap();
            let result = tool.apply_operations(&operations).await;

            assert!(result.is_err());
            assert!(
                result
                    .err()
                    .unwrap()
                    .to_string()
                    .contains("has not been read")
            );
        }

        #[tokio::test]
        async fn apply_operations_delete_succeeds_after_read() {
            let temp_dir = tempfile::TempDir::new().unwrap();
            let tracker = Arc::new(FileTracker::new());
            let tool = PatchTool::new(tracker).with_base_dir(temp_dir.path());

            let file_path = temp_dir.path().join("to_delete.txt");
            fs::write(&file_path, "content").await.unwrap();

            // Read first
            let resolved = tool.resolve_path("to_delete.txt").unwrap();
            tool.tracker.record_read(&resolved);

            // Now delete should succeed
            let patch = "*** Delete File: to_delete.txt";
            let operations = parse_patch(patch).unwrap();
            let result = tool.apply_operations(&operations).await;

            assert!(result.is_ok());
        }
    }

    pub mod reply {
        // Reply tool — allows the agent to send intermediate messages to the user
        // during execution (before the final response).

        use async_trait::async_trait;
        use serde::Deserialize;
        use serde_json::{Value, json};
        use std::sync::Arc;

        use crate::Result;
        use crate::{Tool, ToolOutput};
        use types::store::ReplySender;

        #[derive(Debug, Deserialize)]
        struct ReplyInput {
            /// Message to send to the user.
            message: String,
        }

        /// Tool that lets the agent send a message to the user mid-execution.
        pub struct ReplyTool {
            sender: Arc<dyn ReplySender>,
        }

        impl ReplyTool {
            pub fn new(sender: Arc<dyn ReplySender>) -> Self {
                Self { sender }
            }
        }

        #[async_trait]
        impl Tool for ReplyTool {
            fn name(&self) -> &str {
                "reply"
            }

            fn description(&self) -> &str {
                "Send an intermediate message to the user during execution. Use this to acknowledge requests, provide progress updates, or share partial results before the final response. The message is delivered immediately to the active chat stream."
            }

            fn parameters_schema(&self) -> Value {
                json!({
                    "type": "object",
                    "properties": {
                        "message": {
                            "type": "string",
                            "description": "The message to send to the user"
                        }
                    },
                    "required": ["message"]
                })
            }

            async fn execute(&self, input: Value) -> Result<ToolOutput> {
                let parsed: ReplyInput = serde_json::from_value(input)
                    .map_err(|e| crate::ToolError::Tool(format!("Invalid reply input: {e}")))?;

                if parsed.message.trim().is_empty() {
                    return Ok(ToolOutput::error("Message cannot be empty"));
                }

                match self.sender.send(parsed.message.clone()).await {
                    Ok(()) => Ok(ToolOutput::success(
                        json!({"status": "sent", "message": parsed.message}),
                    )),
                    Err(e) => Ok(ToolOutput::error(format!(
                        "Failed to send reply: {e}. The reply channel may have closed. Check if the conversation is still active."
                    ))),
                }
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use std::future::Future;
            use std::pin::Pin;
            use std::sync::Mutex;

            struct MockSender {
                messages: Arc<Mutex<Vec<String>>>,
            }

            struct FailingSender;

            impl ReplySender for MockSender {
                fn send(
                    &self,
                    message: String,
                ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
                    let messages = self.messages.clone();
                    Box::pin(async move {
                        messages.lock().unwrap().push(message);
                        Ok(())
                    })
                }
            }

            impl ReplySender for FailingSender {
                fn send(
                    &self,
                    _message: String,
                ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
                    Box::pin(async move { anyhow::bail!("channel closed") })
                }
            }

            #[tokio::test]
            async fn test_reply_tool_sends_message() {
                let messages = Arc::new(Mutex::new(Vec::new()));
                let sender = Arc::new(MockSender {
                    messages: messages.clone(),
                });
                let tool = ReplyTool::new(sender);

                let result = tool
                    .execute(json!({"message": "Working on it..."}))
                    .await
                    .unwrap();

                assert!(result.success);
                assert_eq!(messages.lock().unwrap().len(), 1);
                assert_eq!(messages.lock().unwrap()[0], "Working on it...");
            }

            #[tokio::test]
            async fn test_reply_tool_rejects_empty_message() {
                let sender = Arc::new(MockSender {
                    messages: Arc::new(Mutex::new(Vec::new())),
                });
                let tool = ReplyTool::new(sender);

                let result = tool.execute(json!({"message": "  "})).await.unwrap();
                assert!(!result.success);
            }

            #[tokio::test]
            async fn test_reply_tool_error_guidance() {
                let tool = ReplyTool::new(Arc::new(FailingSender));
                let result = tool.execute(json!({"message": "ping"})).await.unwrap();

                assert!(!result.success);
                assert!(
                    result
                        .error
                        .expect("expected error")
                        .contains("Check if the conversation is still active")
                );
            }
        }
    }

    pub mod secrets {
        // Secrets management tool for AI agents.

        use async_trait::async_trait;
        use serde::Deserialize;
        use serde_json::{Value, json};
        use std::sync::Arc;

        use types::store::SecretStore;

        use crate::Result;
        use crate::{Tool, ToolError, ToolOutput};

        #[derive(Clone)]
        pub struct SecretsTool {
            store: Arc<dyn SecretStore>,
            allow_write: bool,
            get_policy: SecretGetPolicy,
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub enum SecretGetPolicy {
            #[default]
            Open,
            MetadataOnly,
            Deny,
        }

        impl SecretsTool {
            pub fn new(store: Arc<dyn SecretStore>) -> Self {
                Self {
                    store,
                    allow_write: false,
                    get_policy: SecretGetPolicy::Open,
                }
            }

            pub fn with_write(mut self, allow_write: bool) -> Self {
                self.allow_write = allow_write;
                self
            }

            pub fn with_get_policy(mut self, get_policy: SecretGetPolicy) -> Self {
                self.get_policy = get_policy;
                self
            }

            fn write_guard(&self) -> Result<()> {
                if self.allow_write {
                    Ok(())
                } else {
                    Err(crate::ToolError::Tool(
                        "Write access to secrets is disabled. Available read-only operations: list, has. To modify secrets, the user must grant write permissions.".to_string(),
                    ))
                }
            }
        }

        #[derive(Debug, Deserialize)]
        #[serde(tag = "operation", rename_all = "snake_case")]
        enum SecretsAction {
            List,
            Get {
                key: String,
            },
            Set {
                key: String,
                value: String,
                #[serde(default)]
                description: Option<String>,
            },
            Delete {
                key: String,
            },
            Has {
                key: String,
            },
        }

        #[async_trait]
        impl Tool for SecretsTool {
            fn name(&self) -> &str {
                "manage_secrets"
            }

            fn description(&self) -> &str {
                "List, read, set, delete, and existence-check named secrets in secure storage."
            }

            fn parameters_schema(&self) -> Value {
                json!({
                    "type": "object",
                    "properties": {
                        "operation": {
                            "type": "string",
                            "enum": ["list", "get", "set", "delete", "has"],
                            "description": "Secret operation to perform"
                        },
                        "key": {
                            "type": "string",
                            "description": "Secret key (for get/set/delete/has)"
                        },
                        "value": {
                            "type": "string",
                            "description": "Secret value (for set)"
                        },
                        "description": {
                            "type": "string",
                            "description": "Optional description (for set)"
                        }
                    },
                    "required": ["operation"]
                })
            }

            async fn execute(&self, input: Value) -> Result<ToolOutput> {
                let action: SecretsAction = serde_json::from_value(input)?;

                let output = match action {
                    SecretsAction::List => {
                        let result = self.store.list_secrets()?;
                        ToolOutput::success(result)
                    }
                    SecretsAction::Get { key } => {
                        let value = self.store.get_secret(&key)?;
                        match self.get_policy {
                            SecretGetPolicy::Open => ToolOutput::success(json!({
                                "key": key,
                                "found": value.is_some(),
                                "value": value
                            })),
                            SecretGetPolicy::MetadataOnly => ToolOutput::success(json!({
                                "key": key,
                                "found": value.is_some(),
                                "value": Value::Null
                            })),
                            SecretGetPolicy::Deny => {
                                return Err(ToolError::Tool(
                                    "Reading secret values is disabled by policy. Use list/has for non-sensitive checks."
                                        .to_string(),
                                ));
                            }
                        }
                    }
                    SecretsAction::Set {
                        key,
                        value,
                        description,
                    } => {
                        self.write_guard()?;
                        let existed = self.store.has_secret(&key)?;
                        self.store.set_secret(&key, &value, description)?;
                        ToolOutput::success(json!({
                            "key": key,
                            "updated": existed,
                            "created": !existed
                        }))
                    }
                    SecretsAction::Delete { key } => {
                        self.write_guard()?;
                        let existed = self.store.has_secret(&key)?;
                        if existed {
                            self.store.delete_secret(&key)?;
                        }
                        ToolOutput::success(json!({ "key": key, "deleted": existed }))
                    }
                    SecretsAction::Has { key } => {
                        let exists = self.store.has_secret(&key)?;
                        ToolOutput::success(json!({ "key": key, "exists": exists }))
                    }
                };

                Ok(output)
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use types::error::Result as TraitResult;

            #[derive(Clone)]
            struct MockSecretStore {
                secrets: Arc<parking_lot::RwLock<std::collections::HashMap<String, String>>>,
            }

            impl MockSecretStore {
                fn new() -> Self {
                    Self {
                        secrets: Arc::new(parking_lot::RwLock::new(
                            std::collections::HashMap::new(),
                        )),
                    }
                }
            }

            impl SecretStore for MockSecretStore {
                fn list_secrets(&self) -> TraitResult<Value> {
                    let map = self.secrets.read();
                    let secrets: Vec<Value> = map
                        .keys()
                        .map(|k| json!({ "key": k, "value": "", "description": null }))
                        .collect();
                    Ok(json!({ "count": secrets.len(), "secrets": secrets }))
                }

                fn get_secret(&self, key: &str) -> TraitResult<Option<String>> {
                    let map = self.secrets.read();
                    Ok(map.get(key).cloned())
                }

                fn set_secret(
                    &self,
                    key: &str,
                    value: &str,
                    _description: Option<String>,
                ) -> TraitResult<()> {
                    self.secrets
                        .write()
                        .insert(key.to_string(), value.to_string());
                    Ok(())
                }

                fn delete_secret(&self, key: &str) -> TraitResult<()> {
                    self.secrets.write().remove(key);
                    Ok(())
                }

                fn has_secret(&self, key: &str) -> TraitResult<bool> {
                    Ok(self.secrets.read().contains_key(key))
                }
            }

            fn setup_store() -> Arc<dyn SecretStore> {
                Arc::new(MockSecretStore::new())
            }

            #[tokio::test(flavor = "current_thread")]
            async fn test_list_and_get_secret() {
                let store = setup_store();
                store
                    .set_secret("TEST_KEY", "value", Some("desc".to_string()))
                    .unwrap();

                let tool = SecretsTool::new(store).with_write(true);
                let output = tool
                    .execute(json!({ "operation": "get", "key": "TEST_KEY" }))
                    .await
                    .unwrap();

                assert!(output.success);
                assert_eq!(output.result["value"], "value");

                let list_output = tool.execute(json!({ "operation": "list" })).await.unwrap();
                assert!(list_output.success);
                assert_eq!(list_output.result["count"], 1);
            }

            #[tokio::test(flavor = "current_thread")]
            async fn test_write_guard() {
                let store = setup_store();
                let tool = SecretsTool::new(store);
                let result = tool
                    .execute(json!({ "operation": "set", "key": "A", "value": "B" }))
                    .await;
                let err = result.expect_err("expected write-guard error");
                assert!(
                    err.to_string()
                        .contains("Available read-only operations: list, has")
                );
            }

            #[tokio::test(flavor = "current_thread")]
            async fn test_get_metadata_only_policy_redacts_value() {
                let store = setup_store();
                store
                    .set_secret("TEST_KEY", "value", Some("desc".to_string()))
                    .unwrap();

                let tool = SecretsTool::new(store).with_get_policy(SecretGetPolicy::MetadataOnly);
                let output = tool
                    .execute(json!({ "operation": "get", "key": "TEST_KEY" }))
                    .await
                    .unwrap();

                assert!(output.success);
                assert_eq!(output.result["found"], true);
                assert_eq!(output.result["value"], Value::Null);
            }

            #[tokio::test(flavor = "current_thread")]
            async fn test_get_deny_policy_blocks_read() {
                let store = setup_store();
                store
                    .set_secret("TEST_KEY", "value", Some("desc".to_string()))
                    .unwrap();

                let tool = SecretsTool::new(store).with_get_policy(SecretGetPolicy::Deny);
                let result = tool
                    .execute(json!({ "operation": "get", "key": "TEST_KEY" }))
                    .await;
                let err = result.expect_err("expected get policy deny error");
                assert!(
                    err.to_string()
                        .contains("Reading secret values is disabled by policy")
                );
            }
        }
    }

    pub mod session {
        // Chat session management tool.

        use async_trait::async_trait;
        use serde::Deserialize;
        use serde_json::{Value, json};
        use std::sync::Arc;

        use crate::Result;
        use crate::{Tool, ToolError, ToolOutput};
        use types::store::{
            SessionCreateRequest, SessionListFilter, SessionSearchQuery, SessionStore,
        };

        #[derive(Clone)]
        pub struct SessionTool {
            store: Arc<dyn SessionStore>,
            allow_write: bool,
        }

        impl SessionTool {
            pub fn new(store: Arc<dyn SessionStore>) -> Self {
                Self {
                    store,
                    allow_write: false,
                }
            }

            pub fn with_write(mut self, allow_write: bool) -> Self {
                self.allow_write = allow_write;
                self
            }

            fn write_guard(&self) -> Result<()> {
                if self.allow_write {
                    Ok(())
                } else {
                    Err(crate::ToolError::Tool(
                        "Write access to sessions is disabled. Available read-only operations: list, get, search. To modify sessions, the user must grant write permissions.".to_string(),
                    ))
                }
            }
        }

        #[derive(Debug, Deserialize)]
        #[serde(tag = "operation", rename_all = "snake_case")]
        enum SessionAction {
            List {
                #[serde(default)]
                agent_id: Option<String>,
                #[serde(default)]
                skill_id: Option<String>,
                #[serde(default)]
                include_messages: Option<bool>,
                #[serde(default)]
                include_archived: Option<bool>,
            },
            Get {
                id: String,
            },
            Create {
                agent_id: String,
                model: String,
                #[serde(default)]
                name: Option<String>,
                #[serde(default)]
                skill_id: Option<String>,
                #[serde(default)]
                retention: Option<String>,
            },
            Delete {
                id: String,
            },
            Archive {
                id: String,
            },
            Unarchive {
                id: String,
            },
            Purge {
                id: String,
            },
            Search {
                query: String,
                #[serde(default)]
                agent_id: Option<String>,
                #[serde(default)]
                skill_id: Option<String>,
                #[serde(default)]
                include_archived: Option<bool>,
                #[serde(default)]
                limit: Option<u32>,
            },
            Cleanup,
        }

        #[async_trait]
        impl Tool for SessionTool {
            fn name(&self) -> &str {
                "manage_sessions"
            }

            fn description(&self) -> &str {
                "Create, list, fetch, search, archive, unarchive, and purge chat sessions."
            }

            fn parameters_schema(&self) -> Value {
                json!({
                    "type": "object",
                    "properties": {
                        "operation": {
                            "type": "string",
                            "enum": ["list", "get", "create", "delete", "archive", "unarchive", "purge", "search", "cleanup"],
                            "description": "Session operation to perform"
                        },
                        "id": {
                            "type": "string",
                            "description": "Session ID (for get/delete/archive/unarchive/purge)"
                        },
                        "agent_id": {
                            "type": "string",
                            "description": "Agent ID filter (for list/search) or agent ID for create"
                        },
                        "skill_id": {
                            "type": "string",
                            "description": "Optional skill ID filter (for list/search/create)"
                        },
                        "include_messages": {
                            "type": "boolean",
                            "description": "Include full messages in list results",
                            "default": false
                        },
                        "include_archived": {
                            "type": "boolean",
                            "description": "Include archived sessions in list/search results",
                            "default": false
                        },
                        "model": {
                            "type": "string",
                            "description": "Model name (for create)"
                        },
                        "name": {
                            "type": "string",
                            "description": "Optional session name (for create)"
                        },
                        "query": {
                            "type": "string",
                            "description": "Search query (for search)"
                        },
                        "retention": {
                            "type": "string",
                            "description": "Optional per-session retention policy for create (1h, 1d, 7d, 30d)"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Max results (for search)",
                            "minimum": 1
                        }
                    },
                    "required": ["operation"]
                })
            }

            async fn execute(&self, input: Value) -> Result<ToolOutput> {
                let action: SessionAction = serde_json::from_value(input)?;

                let output =
                    match action {
                        SessionAction::List {
                            agent_id,
                            skill_id,
                            include_messages,
                            include_archived,
                        } => {
                            let filter = SessionListFilter {
                                agent_id,
                                skill_id,
                                include_messages,
                                include_archived,
                            };
                            ToolOutput::success(self.store.list_sessions(filter).map_err(|e| {
                                ToolError::Tool(format!("Failed to list session: {e}"))
                            })?)
                        }
                        SessionAction::Get { id } => {
                            ToolOutput::success(self.store.get_session(&id).map_err(|e| {
                                ToolError::Tool(format!("Failed to get session: {e}"))
                            })?)
                        }
                        SessionAction::Create {
                            agent_id,
                            model,
                            name,
                            skill_id,
                            retention,
                        } => {
                            self.write_guard()?;
                            let request = SessionCreateRequest {
                                agent_id,
                                model,
                                name,
                                skill_id,
                                retention,
                            };
                            ToolOutput::success(self.store.create_session(request).map_err(
                                |e| ToolError::Tool(format!("Failed to create session: {e}")),
                            )?)
                        }
                        SessionAction::Delete { id } => {
                            self.write_guard()?;
                            ToolOutput::success(self.store.delete_session(&id).map_err(|e| {
                                ToolError::Tool(format!("Failed to delete session: {e}"))
                            })?)
                        }
                        SessionAction::Archive { id } => {
                            self.write_guard()?;
                            ToolOutput::success(self.store.archive_session(&id).map_err(|e| {
                                ToolError::Tool(format!("Failed to archive session: {e}"))
                            })?)
                        }
                        SessionAction::Unarchive { id } => {
                            self.write_guard()?;
                            ToolOutput::success(self.store.unarchive_session(&id).map_err(|e| {
                                ToolError::Tool(format!("Failed to unarchive session: {e}"))
                            })?)
                        }
                        SessionAction::Purge { id } => {
                            self.write_guard()?;
                            ToolOutput::success(self.store.purge_session(&id).map_err(|e| {
                                ToolError::Tool(format!("Failed to purge session: {e}"))
                            })?)
                        }
                        SessionAction::Search {
                            query,
                            agent_id,
                            skill_id,
                            include_archived,
                            limit,
                        } => {
                            let request = SessionSearchQuery {
                                query,
                                agent_id,
                                skill_id,
                                include_archived,
                                limit,
                            };
                            ToolOutput::success(self.store.search_sessions(request).map_err(
                                |e| ToolError::Tool(format!("Failed to search session: {e}")),
                            )?)
                        }
                        SessionAction::Cleanup => {
                            self.write_guard()?;
                            ToolOutput::success(self.store.cleanup_sessions().map_err(|e| {
                                ToolError::Tool(format!("Failed to cleanup sessions: {e}"))
                            })?)
                        }
                    };

                Ok(output)
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            struct MockStore;

            impl SessionStore for MockStore {
                fn list_sessions(&self, _filter: SessionListFilter) -> Result<Value> {
                    Ok(json!([{"id": "session-1"}]))
                }

                fn get_session(&self, _id: &str) -> Result<Value> {
                    Ok(json!({"id": "session-1"}))
                }

                fn create_session(&self, _request: SessionCreateRequest) -> Result<Value> {
                    Ok(json!({"id": "session-1"}))
                }

                fn archive_session(&self, _id: &str) -> Result<Value> {
                    Ok(json!({"archived": true}))
                }

                fn unarchive_session(&self, _id: &str) -> Result<Value> {
                    Ok(json!({"unarchived": true}))
                }

                fn purge_session(&self, _id: &str) -> Result<Value> {
                    Ok(json!({"purged": true}))
                }

                fn delete_session(&self, _id: &str) -> Result<Value> {
                    Ok(json!({"deleted": true}))
                }

                fn search_sessions(&self, _query: SessionSearchQuery) -> Result<Value> {
                    Ok(json!([{"id": "session-1"}]))
                }

                fn cleanup_sessions(&self) -> Result<Value> {
                    Ok(
                        json!({"scanned": 1, "deleted": 1, "skipped": 0, "failed": 0, "bytes_freed": 100}),
                    )
                }
            }

            #[tokio::test]
            async fn test_list_sessions() {
                let tool = SessionTool::new(Arc::new(MockStore));
                let output = tool.execute(json!({"operation": "list"})).await.unwrap();
                assert!(output.success);
            }

            #[tokio::test]
            async fn test_create_requires_write() {
                let tool = SessionTool::new(Arc::new(MockStore));
                let result = tool
                    .execute(json!({"operation": "create", "agent_id": "agent", "model": "gpt"}))
                    .await;
                let err = result.expect_err("expected write-guard error");
                assert!(
                    err.to_string()
                        .contains("Available read-only operations: list, get, search")
                );
            }

            #[tokio::test]
            async fn test_cleanup_requires_write() {
                let tool = SessionTool::new(Arc::new(MockStore));
                let result = tool.execute(json!({"operation": "cleanup"})).await;
                assert!(result.is_err());
            }

            #[tokio::test]
            async fn test_cleanup_with_write_enabled() {
                let tool = SessionTool::new(Arc::new(MockStore)).with_write(true);
                let output = tool.execute(json!({"operation": "cleanup"})).await.unwrap();
                assert!(output.success);
                assert_eq!(output.result["deleted"], 1);
            }

            #[tokio::test]
            async fn test_archive_with_write_enabled() {
                let tool = SessionTool::new(Arc::new(MockStore)).with_write(true);
                let output = tool
                    .execute(json!({"operation": "archive", "id": "session-1"}))
                    .await
                    .unwrap();
                assert!(output.success);
                assert_eq!(output.result["archived"], true);
            }
        }
    }

    pub mod skill {
        // Skill tool for listing and reading skills

        use async_trait::async_trait;
        use serde::Deserialize;
        use serde_json::{Value, json};
        use std::sync::Arc;

        use crate::Result;
        use crate::{SecurityGate, ToolAction};
        use crate::{Tool, ToolOutput, check_security};
        use types::skill::SkillProvider;

        #[derive(Debug, Deserialize)]
        #[serde(tag = "action", rename_all = "snake_case")]
        enum SkillInput {
            List,
            Read { id: String },
            Export { id: String },
        }

        /// Skill tool for managing skills
        pub struct SkillTool {
            provider: Arc<dyn SkillProvider>,
            security_gate: Option<Arc<dyn SecurityGate>>,
            agent_id: Option<String>,
            task_id: Option<String>,
        }

        impl SkillTool {
            /// Create a new skill tool with the given provider
            pub fn new(provider: Arc<dyn SkillProvider>) -> Self {
                Self {
                    provider,
                    security_gate: None,
                    agent_id: None,
                    task_id: None,
                }
            }

            pub fn with_security(
                mut self,
                security_gate: Arc<dyn SecurityGate>,
                agent_id: impl Into<String>,
                task_id: impl Into<String>,
            ) -> Self {
                self.security_gate = Some(security_gate);
                self.agent_id = Some(agent_id.into());
                self.task_id = Some(task_id.into());
                self
            }
        }

        #[async_trait]
        impl Tool for SkillTool {
            fn name(&self) -> &str {
                "skill"
            }

            fn description(&self) -> &str {
                "List, read, and export reusable skill definitions from the skrun-managed catalog."
            }

            fn parameters_schema(&self) -> Value {
                json!({
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["list", "read", "export"],
                            "description": "Action to perform"
                        },
                        "id": {
                            "type": "string",
                            "description": "Skill ID (required for read/export)"
                        }
                    },
                    "required": ["action"]
                })
            }

            async fn execute(&self, input: Value) -> Result<ToolOutput> {
                let params: SkillInput = serde_json::from_value(input)?;

                match params {
                    SkillInput::List => {
                        if let Some(message) = check_security(
                            self.security_gate.as_deref(),
                            ToolAction {
                                tool_name: "skill".to_string(),
                                operation: "list".to_string(),
                                target: "*".to_string(),
                                summary: "List skills".to_string(),
                            },
                            self.agent_id.as_deref(),
                            self.task_id.as_deref(),
                        )
                        .await?
                        {
                            return Ok(ToolOutput::error(message));
                        }
                        let skills = self.provider.list_skills();
                        Ok(ToolOutput::success(json!({
                            "skills": skills
                        })))
                    }
                    SkillInput::Read { id } => {
                        if let Some(message) = check_security(
                            self.security_gate.as_deref(),
                            ToolAction {
                                tool_name: "skill".to_string(),
                                operation: "read".to_string(),
                                target: id.clone(),
                                summary: format!("Read skill '{}'", id),
                            },
                            self.agent_id.as_deref(),
                            self.task_id.as_deref(),
                        )
                        .await?
                        {
                            return Ok(ToolOutput::error(message));
                        }
                        match self.provider.get_skill(&id) {
                            Some(skill) => Ok(ToolOutput::success(json!(skill))),
                            None => Ok(ToolOutput::error(format!("Skill '{}' not found", id))),
                        }
                    }
                    SkillInput::Export { id } => match self.provider.export_skill(&id) {
                        Ok(markdown) => {
                            if let Some(message) = check_security(
                                self.security_gate.as_deref(),
                                ToolAction {
                                    tool_name: "skill".to_string(),
                                    operation: "export".to_string(),
                                    target: id.clone(),
                                    summary: format!("Export skill '{}'", id),
                                },
                                self.agent_id.as_deref(),
                                self.task_id.as_deref(),
                            )
                            .await?
                            {
                                return Ok(ToolOutput::error(message));
                            }
                            Ok(ToolOutput::success(json!({
                                "id": id,
                                "markdown": markdown
                            })))
                        }
                        Err(err) => Ok(ToolOutput::error(format!("Skill operation failed: {err}"))),
                    },
                }
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use types::skill::{SkillContent, SkillInfo, SkillSource};

            #[derive(Clone)]
            struct TestSkill {
                id: String,
                name: String,
                description: Option<String>,
                tags: Option<Vec<String>>,
                content: String,
                source: SkillSource,
                read_only: bool,
                source_ref: Option<String>,
            }

            struct MockSkillProvider {
                skills: Vec<TestSkill>,
            }

            impl SkillProvider for MockSkillProvider {
                fn list_skills(&self) -> Vec<SkillInfo> {
                    self.skills
                        .iter()
                        .map(|skill| SkillInfo {
                            id: skill.id.clone(),
                            name: skill.name.clone(),
                            description: skill.description.clone(),
                            tags: skill.tags.clone(),
                            kind: None,
                            executable: false,
                            suggested_tools: Vec::new(),
                            source: skill.source,
                            read_only: skill.read_only,
                            source_ref: skill.source_ref.clone(),
                        })
                        .collect()
                }

                fn get_skill(&self, id: &str) -> Option<SkillContent> {
                    self.skills
                        .iter()
                        .find(|skill| skill.id == id)
                        .map(|skill| SkillContent {
                            id: skill.id.clone(),
                            name: skill.name.clone(),
                            content: skill.content.clone(),
                            kind: None,
                            executable: false,
                            suggested_tools: Vec::new(),
                            source: skill.source,
                            read_only: skill.read_only,
                            source_ref: skill.source_ref.clone(),
                        })
                }

                fn export_skill(&self, id: &str) -> std::result::Result<String, String> {
                    let skill = self
                        .skills
                        .iter()
                        .find(|s| s.id == id)
                        .ok_or_else(|| format!("Skill {} not found", id))?;
                    Ok(format!(
                        "---\nname: {}\n---\n\n{}",
                        skill.name, skill.content
                    ))
                }
            }

            fn create_mock_provider() -> Arc<dyn SkillProvider> {
                Arc::new(MockSkillProvider {
                    skills: vec![TestSkill {
                        id: "test-skill".to_string(),
                        name: "Test Skill".to_string(),
                        description: Some("A test skill".to_string()),
                        tags: Some(vec!["test".to_string()]),
                        content: "# Test Skill Content\n\nThis is a test.".to_string(),
                        source: SkillSource::User,
                        read_only: false,
                        source_ref: None,
                    }],
                })
            }

            #[test]
            fn test_skill_tool_schema() {
                let tool = SkillTool::new(create_mock_provider());
                assert_eq!(tool.name(), "skill");
                assert!(!tool.description().is_empty());

                let schema = tool.parameters_schema();
                assert!(schema.get("properties").is_some());
            }

            #[test]
            fn test_schema_never_exposes_write_actions() {
                let tool = SkillTool::new(create_mock_provider());
                let schema = tool.parameters_schema();
                let actions = schema["properties"]["action"]["enum"]
                    .as_array()
                    .expect("enum array");
                for write_action in ["create", "update", "delete", "import"] {
                    assert!(
                        !actions
                            .iter()
                            .any(|value| value.as_str() == Some(write_action)),
                        "skill tool must not expose {write_action} action"
                    );
                }
            }

            #[tokio::test]
            async fn test_list_skills() {
                let tool = SkillTool::new(create_mock_provider());
                let result = tool.execute(json!({ "action": "list" })).await.unwrap();

                assert!(result.success);
                let skills = result.result.get("skills").unwrap().as_array().unwrap();
                assert_eq!(skills.len(), 1);
                assert_eq!(skills[0]["id"], "test-skill");
            }

            #[tokio::test]
            async fn test_read_skill() {
                let tool = SkillTool::new(create_mock_provider());
                let result = tool
                    .execute(json!({ "action": "read", "id": "test-skill" }))
                    .await
                    .unwrap();

                assert!(result.success);
                assert_eq!(result.result["id"], "test-skill");
                assert!(
                    result.result["content"]
                        .as_str()
                        .unwrap()
                        .contains("Test Skill Content")
                );
            }

            #[tokio::test]
            async fn test_read_skill_not_found() {
                let tool = SkillTool::new(create_mock_provider());
                let result = tool
                    .execute(json!({ "action": "read", "id": "nonexistent" }))
                    .await
                    .unwrap();

                assert!(!result.success);
                assert!(result.error.unwrap().contains("not found"));
            }

            #[tokio::test]
            async fn test_create_action_is_not_supported() {
                let tool = SkillTool::new(create_mock_provider());
                let result = tool
                    .execute(json!({
                        "action": "create",
                        "id": "new",
                        "name": "New",
                        "content": "# New"
                    }))
                    .await;

                let err = result.expect_err("expected unsupported action error");
                assert!(err.to_string().contains("unknown variant"));
            }

            #[tokio::test]
            async fn builder_registered_skill_tool_is_read_only() {
                let registry = crate::impls::ToolRegistryBuilder::new()
                    .with_skill_tool(create_mock_provider())
                    .build();

                let schema = registry
                    .get("skill")
                    .expect("skill tool should be registered")
                    .parameters_schema();
                let actions = schema["properties"]["action"]["enum"]
                    .as_array()
                    .expect("action enum should be present");
                assert!(
                    !actions
                        .iter()
                        .any(|action| action.as_str() == Some("create")),
                    "builder-registered skill tool must not expose write actions"
                );

                let result = registry
                    .execute_safe(
                        "skill",
                        json!({
                            "action": "create",
                            "id": "new",
                            "name": "New",
                            "content": "# New"
                        }),
                    )
                    .await;

                let err = result.expect_err("expected builder-registered skill create to fail");
                assert!(err.to_string().contains("unknown variant"));
            }
        }
    }

    pub mod switch_model {
        // Tool for switching the active LLM model at runtime

        use async_trait::async_trait;
        use serde_json::{Value, json};
        use std::sync::Arc;

        use crate::{Result, ToolError};
        use crate::{Tool, ToolOutput};
        use types::{LlmProvider, LlmSwitcher};
        use types::{
            ProviderSelector, parse_model_reference, parse_provider_selector,
            resolve_available_model_name, split_provider_qualified_model,
        };

        #[derive(Clone)]
        pub struct SwitchModelTool {
            switcher: Arc<dyn LlmSwitcher>,
        }

        impl SwitchModelTool {
            pub fn new(switcher: Arc<dyn LlmSwitcher>) -> Self {
                Self { switcher }
            }

            fn model_matches_provider(&self, model: &str, provider: ProviderSelector) -> bool {
                parse_model_reference(model)
                    .map(|model_id| provider.matches_model(model_id))
                    .unwrap_or(false)
            }

            fn resolve_target_model(
                &self,
                requested_provider: Option<&str>,
                requested_model: Option<&str>,
            ) -> Result<String> {
                let available = self.switcher.available_models();

                if requested_provider.is_none() || requested_model.is_none() {
                    return Err(ToolError::Tool(
                        "Missing parameters: both 'provider' and 'model' are required".to_string(),
                    ));
                }

                let provider_raw = requested_provider.expect("requested_provider checked above");
                let provider = parse_provider_selector(provider_raw).ok_or_else(|| {
                    ToolError::Tool(format!(
                        "Unknown provider: {provider_raw}. Use provider names like openai, anthropic, minimax, minimax-coding-plan, zai, zai-coding-plan, openai-codex, gemini-cli"
                    ))
                })?;

                let model_raw = requested_model.expect("requested_model checked above");
                if let Some(model) = resolve_available_model_name(model_raw, &available) {
                    if !self.model_matches_provider(&model, provider) {
                        return Err(ToolError::Tool(format!(
                            "Model '{model_raw}' does not belong to provider '{}'",
                            provider.label()
                        )));
                    }
                    return Ok(model);
                }

                let model_candidate = if let Some((inline_provider, inline_model)) =
                    split_provider_qualified_model(model_raw)
                {
                    if inline_provider != provider {
                        return Err(ToolError::Tool(format!(
                            "Model '{model_raw}' does not belong to provider '{}'",
                            provider.label()
                        )));
                    }
                    inline_model.to_string()
                } else {
                    model_raw.to_string()
                };

                let model = resolve_available_model_name(&model_candidate, &available).ok_or_else(|| {
                    ToolError::Tool(format!(
                        "Unknown model: '{model_candidate}'. Use manage_agents tool to list available models, or check the provider's documentation."
                    ))
                })?;
                if !self.model_matches_provider(&model, provider) {
                    return Err(ToolError::Tool(format!(
                        "Model '{model_raw}' does not belong to provider '{}'",
                        provider.label()
                    )));
                }
                Ok(model)
            }
        }

        #[async_trait]
        impl Tool for SwitchModelTool {
            fn name(&self) -> &str {
                "switch_model"
            }

            fn description(&self) -> &str {
                "Switch the active LLM provider and model for the current agent execution."
            }

            fn parameters_schema(&self) -> Value {
                json!({
                    "type": "object",
                    "description": "Both 'provider' and 'model' are required.",
                    "properties": {
                        "provider": {
                            "type": "string",
                            "description": "Provider selector (e.g. openai, anthropic, openai-codex, gemini-cli)"
                        },
                        "model": {
                            "type": "string",
                            "description": "Model name to switch to. Supports `provider:model` format for compatibility."
                        },
                        "reason": {
                            "type": "string",
                            "description": "Optional reason for switching models"
                        }
                    },
                    "required": ["provider", "model"]
                })
            }

            async fn execute(&self, input: Value) -> Result<ToolOutput> {
                let requested_model = input
                    .get("model")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                let requested_provider = input
                    .get("provider")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                let reason = input
                    .get("reason")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string());

                let model_name = self.resolve_target_model(requested_provider, requested_model)?;
                let swap_result = self.switcher.switch_model(&model_name)?;

                let payload = json!({
                    "switched": true,
                    "request": {
                        "provider": requested_provider,
                        "model": requested_model
                    },
                    "from": {
                        "provider": swap_result.previous_provider,
                        "runtime_provider": swap_result.previous_runtime_provider.map(LlmProvider::as_str),
                        "model": swap_result.previous_model
                    },
                    "to": {
                        "provider": swap_result.new_provider,
                        "runtime_provider": swap_result.new_runtime_provider.as_str(),
                        "model": swap_result.new_model
                    },
                    "reason": reason
                });

                Ok(ToolOutput::success(payload))
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use ::agent::llm::{
                ClientKind, CompletionRequest, CompletionResponse, FinishReason, LlmClient,
                LlmClientFactory, LlmProvider, StreamResult, SwappableLlm, TokenUsage,
            };
            use std::collections::HashMap;
            use std::sync::Mutex;

            struct MockClient {
                provider: String,
                model: String,
            }

            impl MockClient {
                fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
                    Self {
                        provider: provider.into(),
                        model: model.into(),
                    }
                }
            }

            #[async_trait]
            impl LlmClient for MockClient {
                fn provider(&self) -> &str {
                    &self.provider
                }

                fn model(&self) -> &str {
                    &self.model
                }

                async fn complete(
                    &self,
                    _request: CompletionRequest,
                ) -> ::agent::llm::Result<CompletionResponse> {
                    Ok(CompletionResponse {
                        content: Some(String::new()),
                        tool_calls: vec![],
                        finish_reason: FinishReason::Stop,
                        usage: Some(TokenUsage::default()),
                        reasoning_content: None,
                    })
                }

                fn complete_stream(&self, _request: CompletionRequest) -> StreamResult {
                    unimplemented!("streaming is not used in switch_model tests")
                }
            }

            struct MockFactory {
                available: Vec<String>,
                providers: HashMap<String, LlmProvider>,
                api_keys: HashMap<LlmProvider, String>,
                client_kinds: HashMap<String, ClientKind>,
                create_calls: Mutex<Vec<(String, Option<String>)>>,
            }

            impl MockFactory {
                fn new(
                    available: Vec<&str>,
                    providers: Vec<(&str, LlmProvider)>,
                    api_keys: Vec<(LlmProvider, &str)>,
                    codex_models: Vec<&str>,
                ) -> Self {
                    let normalize = |value: &str| value.trim().to_lowercase();
                    Self {
                        available: available.into_iter().map(str::to_string).collect(),
                        providers: providers
                            .into_iter()
                            .map(|(model, provider)| (normalize(model), provider))
                            .collect(),
                        api_keys: api_keys
                            .into_iter()
                            .map(|(provider, key)| (provider, key.to_string()))
                            .collect(),
                        client_kinds: codex_models
                            .into_iter()
                            .map(|model| (normalize(model), ClientKind::CodexCli))
                            .collect(),
                        create_calls: Mutex::new(Vec::new()),
                    }
                }

                fn calls(&self) -> Vec<(String, Option<String>)> {
                    self.create_calls.lock().expect("lock poisoned").clone()
                }
            }

            impl LlmClientFactory for MockFactory {
                fn create_client(
                    &self,
                    model: &str,
                    api_key: Option<&str>,
                ) -> ::agent::llm::Result<Arc<dyn LlmClient>> {
                    self.create_calls
                        .lock()
                        .expect("lock poisoned")
                        .push((model.to_string(), api_key.map(ToString::to_string)));
                    let provider = self.provider_for_model(model).ok_or_else(|| {
                        ::agent::llm::AiError::Llm(format!("no provider found for model {model}"))
                    })?;
                    Ok(Arc::new(MockClient::new(provider.as_str(), model)))
                }

                fn available_models(&self) -> Vec<String> {
                    self.available.clone()
                }

                fn resolve_api_key(&self, provider: LlmProvider) -> Option<String> {
                    self.api_keys.get(&provider).cloned()
                }

                fn provider_for_model(&self, model: &str) -> Option<LlmProvider> {
                    self.providers.get(&model.trim().to_lowercase()).copied()
                }

                fn client_kind_for_model(&self, model: &str) -> Option<ClientKind> {
                    let normalized = model.trim().to_lowercase();
                    self.providers.contains_key(&normalized).then(|| {
                        self.client_kinds
                            .get(&normalized)
                            .copied()
                            .unwrap_or(ClientKind::Http)
                    })
                }
            }

            fn build_tool(factory: Arc<MockFactory>) -> (SwitchModelTool, Arc<SwappableLlm>) {
                use ::agent::llm::LlmSwitcherImpl;
                let llm = Arc::new(SwappableLlm::new(Arc::new(MockClient::new(
                    "anthropic",
                    "claude-haiku-4-5",
                ))));
                let switcher = Arc::new(LlmSwitcherImpl::new(llm.clone(), factory));
                (SwitchModelTool::new(switcher), llm)
            }

            #[tokio::test]
            async fn execute_requires_provider_and_model() {
                let factory = Arc::new(MockFactory::new(
                    vec!["claude-sonnet-4-5", "gpt-5.3-codex"],
                    vec![
                        ("claude-sonnet-4-5", LlmProvider::Anthropic),
                        ("gpt-5.3-codex", LlmProvider::OpenAI),
                    ],
                    vec![(LlmProvider::Anthropic, "anthropic-key")],
                    vec!["gpt-5.3-codex"],
                ));
                let (tool, _) = build_tool(factory);

                let error = tool
                    .execute(json!({ "model": "CLAUDE-SONNET-4-5" }))
                    .await
                    .expect_err("switch should fail without provider");

                assert!(
                    error
                        .to_string()
                        .contains("both 'provider' and 'model' are required"),
                    "unexpected error: {error}"
                );
            }

            #[tokio::test]
            async fn execute_supports_provider_and_model_for_codex_cli() {
                let factory = Arc::new(MockFactory::new(
                    vec!["claude-sonnet-4-5", "gpt-5.3-codex"],
                    vec![
                        ("claude-sonnet-4-5", LlmProvider::Anthropic),
                        ("gpt-5.3-codex", LlmProvider::OpenAI),
                    ],
                    vec![],
                    vec!["gpt-5.3-codex"],
                ));
                let (tool, llm) = build_tool(factory.clone());

                let output = tool
                    .execute(json!({
                        "provider": "openai-codex",
                        "model": "gpt-5.3-codex"
                    }))
                    .await
                    .expect("switch should succeed");

                assert!(output.success);
                assert_eq!(llm.current_model(), "gpt-5.3-codex");
                assert_eq!(factory.calls(), vec![("gpt-5.3-codex".to_string(), None)]);
            }

            #[tokio::test]
            async fn execute_rejects_provider_model_mismatch() {
                let factory = Arc::new(MockFactory::new(
                    vec!["claude-sonnet-4-5", "gpt-5.3-codex"],
                    vec![
                        ("claude-sonnet-4-5", LlmProvider::Anthropic),
                        ("gpt-5.3-codex", LlmProvider::OpenAI),
                    ],
                    vec![(LlmProvider::Anthropic, "anthropic-key")],
                    vec!["gpt-5.3-codex"],
                ));
                let (tool, _) = build_tool(factory);

                let error = tool
                    .execute(json!({
                        "provider": "anthropic",
                        "model": "gpt-5.3-codex"
                    }))
                    .await
                    .expect_err("switch should fail");

                assert!(
                    error
                        .to_string()
                        .contains("does not belong to provider 'anthropic'"),
                    "unexpected error: {error}"
                );
            }

            #[tokio::test]
            async fn execute_rejects_openai_provider_for_openai_codex_model() {
                let factory = Arc::new(MockFactory::new(
                    vec!["gpt-5", "gpt-5.3-codex"],
                    vec![
                        ("gpt-5", LlmProvider::OpenAI),
                        ("gpt-5.3-codex", LlmProvider::OpenAI),
                    ],
                    vec![(LlmProvider::OpenAI, "openai-key")],
                    vec!["gpt-5.3-codex"],
                ));
                let (tool, _) = build_tool(factory);

                let error = tool
                    .execute(json!({
                        "provider": "openai",
                        "model": "gpt-5.3-codex"
                    }))
                    .await
                    .expect_err("switch should fail");

                assert!(
                    error
                        .to_string()
                        .contains("does not belong to provider 'openai'"),
                    "unexpected error: {error}"
                );
            }

            #[tokio::test]
            async fn execute_rejects_missing_model() {
                let factory = Arc::new(MockFactory::new(
                    vec!["gpt-5.3-codex", "claude-sonnet-4-5"],
                    vec![
                        ("claude-sonnet-4-5", LlmProvider::Anthropic),
                        ("gpt-5.3-codex", LlmProvider::OpenAI),
                    ],
                    vec![],
                    vec!["gpt-5.3-codex"],
                ));
                let (tool, _) = build_tool(factory);

                let error = tool
                    .execute(json!({ "provider": "openai-codex" }))
                    .await
                    .expect_err("switch should fail without model");

                assert!(
                    error
                        .to_string()
                        .contains("both 'provider' and 'model' are required"),
                    "unexpected error: {error}"
                );
            }

            #[tokio::test]
            async fn execute_reports_unknown_model_with_actionable_guidance() {
                let factory = Arc::new(MockFactory::new(
                    vec!["gpt-5.3-codex", "claude-sonnet-4-5"],
                    vec![
                        ("claude-sonnet-4-5", LlmProvider::Anthropic),
                        ("gpt-5.3-codex", LlmProvider::OpenAI),
                    ],
                    vec![],
                    vec!["gpt-5.3-codex"],
                ));
                let (tool, _) = build_tool(factory);

                let error = tool
                    .execute(json!({
                        "provider": "openai-codex",
                        "model": "missing-model"
                    }))
                    .await
                    .expect_err("switch should fail for unknown model");
                let message = error.to_string();

                assert!(message.contains("Unknown model: 'missing-model'"));
                assert!(message.contains("Use manage_agents tool to list available models"));
            }

            #[tokio::test]
            async fn execute_reports_missing_api_key_with_manage_secrets_guidance() {
                let factory = Arc::new(MockFactory::new(
                    vec!["claude-sonnet-4-5"],
                    vec![("claude-sonnet-4-5", LlmProvider::Anthropic)],
                    vec![],
                    vec![],
                ));
                let (tool, _) = build_tool(factory);

                let error = tool
                    .execute(json!({
                        "provider": "anthropic",
                        "model": "claude-sonnet-4-5"
                    }))
                    .await
                    .expect_err("switch should fail without provider key");
                let message = error.to_string();

                assert!(message.contains("No API key for provider 'anthropic'"));
                assert!(message.contains("Set the key via manage_secrets tool"));
            }

            #[tokio::test]
            async fn execute_supports_provider_qualified_model_when_provider_matches() {
                let factory = Arc::new(MockFactory::new(
                    vec!["gpt-5.3-codex", "claude-sonnet-4-5"],
                    vec![
                        ("claude-sonnet-4-5", LlmProvider::Anthropic),
                        ("gpt-5.3-codex", LlmProvider::OpenAI),
                    ],
                    vec![],
                    vec!["gpt-5.3-codex"],
                ));
                let (tool, llm) = build_tool(factory.clone());

                let output = tool
                    .execute(json!({
                        "provider": "openai-codex",
                        "model": "openai-codex:gpt-5.3-codex"
                    }))
                    .await
                    .expect("switch should succeed");

                assert!(output.success);
                assert_eq!(llm.current_model(), "gpt-5.3-codex");
                assert_eq!(factory.calls(), vec![("gpt-5.3-codex".to_string(), None)]);
            }

            #[tokio::test]
            async fn execute_supports_shared_catalog_aliases_for_coding_plan_models() {
                let factory = Arc::new(MockFactory::new(
                    vec!["minimax-coding-plan-m2-1", "minimax-coding-plan-m2-5"],
                    vec![
                        ("minimax-coding-plan-m2-1", LlmProvider::MiniMaxCodingPlan),
                        ("minimax-coding-plan-m2-5", LlmProvider::MiniMaxCodingPlan),
                    ],
                    vec![(LlmProvider::MiniMaxCodingPlan, "minimax-key")],
                    vec![],
                ));
                let (tool, llm) = build_tool(factory.clone());

                let output = tool
                    .execute(json!({
                        "provider": "minimax-coding-plan",
                        "model": "minimax/coding-plan"
                    }))
                    .await
                    .expect("switch should resolve shared alias");

                assert!(output.success);
                assert_eq!(llm.current_model(), "minimax-coding-plan-m2-5");
                assert_eq!(
                    factory.calls(),
                    vec![(
                        "minimax-coding-plan-m2-5".to_string(),
                        Some("minimax-key".to_string())
                    )]
                );
            }

            #[test]
            fn schema_is_claude_compatible() {
                let factory = Arc::new(MockFactory::new(
                    vec!["claude-sonnet-4-5"],
                    vec![("claude-sonnet-4-5", LlmProvider::Anthropic)],
                    vec![(LlmProvider::Anthropic, "anthropic-key")],
                    vec![],
                ));
                let (tool, _) = build_tool(factory);
                let schema = tool.parameters_schema();

                assert!(schema.get("anyOf").is_none());
                assert!(schema.get("oneOf").is_none());
                assert!(schema.get("allOf").is_none());
                assert_eq!(
                    schema["required"],
                    json!(["provider", "model"]),
                    "provider and model should both be required"
                );
            }
        }
    }

    pub mod manage_ops {
        // Unified operational diagnostics tool for daemon health and log tail.

        use async_trait::async_trait;
        use serde_json::{Value, json};
        use std::sync::Arc;

        use crate::{Result, ToolError};
        use crate::{Tool, ToolOutput};
        use types::store::OpsProvider;

        pub struct ManageOpsTool {
            provider: Arc<dyn OpsProvider>,
        }

        impl ManageOpsTool {
            pub fn new(provider: Arc<dyn OpsProvider>) -> Self {
                Self { provider }
            }
        }

        fn parse_limit(input: &Value, key: &str, default: usize, max: usize) -> usize {
            input
                .get(key)
                .and_then(Value::as_u64)
                .map(|v| v as usize)
                .unwrap_or(default)
                .clamp(1, max)
        }

        #[async_trait]
        impl Tool for ManageOpsTool {
            fn name(&self) -> &str {
                "manage_ops"
            }

            fn description(&self) -> &str {
                "Unified operational diagnostics for daemon health and log tail."
            }

            fn parameters_schema(&self) -> Value {
                json!({
                    "type": "object",
                    "properties": {
                        "operation": {
                            "type": "string",
                            "enum": ["daemon_health", "log_tail"],
                            "description": "Operation to execute."
                        },
                        "lines": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "Number of lines for log_tail."
                        },
                        "path": {
                            "type": "string",
                            "description": "Optional log file path for log_tail. Must stay under ~/.restflow/logs."
                        }
                    },
                    "required": ["operation"]
                })
            }

            async fn execute(&self, input: Value) -> Result<ToolOutput> {
                let operation = input
                    .get("operation")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ToolError::Tool("Missing operation parameter".to_string()))?;

                let result = match operation {
                    "daemon_health" => self.provider.daemon_health().await?,
                    "log_tail" => {
                        let lines = parse_limit(&input, "lines", 100, 1000);
                        let path = input.get("path").and_then(Value::as_str);
                        self.provider.log_tail(lines, path)?
                    }
                    other => {
                        return Err(ToolError::Tool(format!(
                            "Unknown operation: {}. Supported: daemon_health, log_tail",
                            other
                        )));
                    }
                };

                Ok(ToolOutput::success(result))
            }
        }
    }

    pub mod glob_tool {
        // Glob pattern file matching tool for AI agents.
        //
        // Provides fast file name pattern matching with:
        // - Full glob syntax (`**`, `{a,b}`, `[abc]`, `?`, `*`)
        // - Results sorted by modification time (newest first)
        // - Automatic skipping of hidden/generated directories
        // - Configurable base directory

        use async_trait::async_trait;
        use serde::Deserialize;
        use serde_json::{Value, json};
        use std::path::{Path, PathBuf};
        use std::time::SystemTime;
        use tokio::fs;

        use super::shared::should_skip_glob_dir;
        use crate::Result;
        use crate::{Tool, ToolOutput};

        /// Maximum entries to return
        const MAX_RESULTS: usize = 1000;

        #[derive(Debug, Deserialize)]
        struct GlobInput {
            pattern: String,
            path: Option<String>,
        }

        /// Glob pattern matching tool that finds files by name patterns.
        #[derive(Clone)]
        pub struct GlobTool {
            base_dir: Option<PathBuf>,
            require_base_dir: bool,
        }

        impl Default for GlobTool {
            fn default() -> Self {
                Self::new()
            }
        }

        impl GlobTool {
            pub fn new() -> Self {
                Self {
                    base_dir: None,
                    require_base_dir: false,
                }
            }

            pub fn with_base_dir(mut self, base: impl Into<PathBuf>) -> Self {
                self.base_dir = Some(base.into());
                self
            }

            pub fn require_base_dir(mut self) -> Self {
                self.require_base_dir = true;
                self
            }

            fn resolve_base(&self, path: Option<&str>) -> std::result::Result<PathBuf, String> {
                if let Some(p) = path {
                    let candidate = PathBuf::from(p);
                    if candidate.is_absolute() {
                        Ok(candidate)
                    } else if let Some(base) = &self.base_dir {
                        Ok(base.join(candidate))
                    } else {
                        Err(
                            "Relative search paths require an explicit workspace root or base directory."
                                .to_string(),
                        )
                    }
                } else if let Some(base) = &self.base_dir {
                    Ok(base.clone())
                } else if self.require_base_dir {
                    Err(
                        "This tool requires an explicit workspace root or base directory."
                            .to_string(),
                    )
                } else {
                    Err("A base directory is required for glob search.".to_string())
                }
            }
        }

        #[async_trait]
        impl Tool for GlobTool {
            fn name(&self) -> &str {
                "glob"
            }

            fn description(&self) -> &str {
                "Fast file pattern matching tool. Supports glob patterns like \"**/*.rs\" or \"src/**/*.ts\". Returns matching file paths sorted by modification time (newest first)."
            }

            fn parameters_schema(&self) -> Value {
                json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "description": "Glob pattern to match files (e.g. \"**/*.rs\", \"src/{main,lib}.rs\")",
                            "type": "string"
                        },
                        "path": {
                            "description": "Base directory to search in. Requires an explicit path or configured workspace root.",
                            "type": "string"
                        }
                    },
                    "required": ["pattern"]
                })
            }

            async fn execute(&self, input: Value) -> Result<ToolOutput> {
                let params: GlobInput = match serde_json::from_value(input) {
                    Ok(v) => v,
                    Err(err) => return Ok(ToolOutput::error(format!("Invalid input: {}", err))),
                };

                let base = match self.resolve_base(params.path.as_deref()) {
                    Ok(base) => base,
                    Err(error) => return Ok(ToolOutput::error(error)),
                };
                if !base.is_dir() {
                    return Ok(ToolOutput::error(format!(
                        "Directory not found: {}",
                        base.display()
                    )));
                }

                // Split pattern into prefix path + glob part.
                // E.g. "src/**/*.rs" -> walk from base/src with pattern "**/*.rs"
                let (walk_root, glob_pattern) = split_pattern(&base, &params.pattern);

                if !walk_root.is_dir() {
                    return Ok(ToolOutput::success(json!({
                        "files": [],
                        "total": 0
                    })));
                }

                let mut matches: Vec<(String, SystemTime)> = Vec::new();
                walk_and_match(&walk_root, &walk_root, &glob_pattern, &mut matches).await;

                // Sort by mtime descending (newest first)
                matches.sort_by_key(|entry| std::cmp::Reverse(entry.1));

                let total = matches.len();
                let truncated = total > MAX_RESULTS;
                let files: Vec<String> = matches
                    .into_iter()
                    .take(MAX_RESULTS)
                    .map(|(p, _)| p)
                    .collect();

                Ok(ToolOutput::success(json!({
                    "files": files,
                    "total": total,
                    "truncated": truncated
                })))
            }
        }

        /// Split a glob pattern into a concrete prefix (for walking) and the glob remainder.
        ///
        /// E.g. `src/components/**/*.tsx` -> (base/src/components, `**/*.tsx`)
        /// E.g. `**/*.rs` -> (base, `**/*.rs`)
        fn split_pattern(base: &Path, pattern: &str) -> (PathBuf, String) {
            let parts: Vec<&str> = pattern.split('/').collect();
            let mut prefix = base.to_path_buf();
            let mut glob_start = 0;

            for (i, part) in parts.iter().enumerate() {
                // If the part contains glob metacharacters, stop here
                if part.contains('*')
                    || part.contains('?')
                    || part.contains('[')
                    || part.contains('{')
                {
                    glob_start = i;
                    break;
                }
                prefix = prefix.join(part);
                glob_start = i + 1;
            }

            let glob_part = if glob_start < parts.len() {
                parts[glob_start..].join("/")
            } else {
                // Pattern was entirely concrete — match the exact file
                String::new()
            };

            (prefix, glob_part)
        }

        /// Recursively walk directories and collect glob-matching files.
        #[async_recursion::async_recursion]
        async fn walk_and_match(
            root: &Path,
            dir: &Path,
            pattern: &str,
            results: &mut Vec<(String, SystemTime)>,
        ) {
            if results.len() >= MAX_RESULTS * 2 {
                return; // Stop early if we have more than enough
            }

            let mut entries = match fs::read_dir(dir).await {
                Ok(entries) => entries,
                Err(_) => return,
            };

            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                let file_name = match entry.file_name().into_string() {
                    Ok(s) => s,
                    Err(_) => continue,
                };

                // Skip hidden directories and known generated dirs
                if should_skip_glob_dir(&file_name) && path.is_dir() {
                    continue;
                }

                if path.is_dir() {
                    walk_and_match(root, &path, pattern, results).await;
                } else {
                    let relative = match path.strip_prefix(root) {
                        Ok(r) => r.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/"),
                        Err(_) => continue,
                    };

                    let matched = if pattern.is_empty() {
                        // Exact file match (pattern was entirely concrete)
                        relative.is_empty() || path == root
                    } else {
                        glob_match::glob_match(pattern, &relative)
                    };

                    if matched {
                        let mtime = entry
                            .metadata()
                            .await
                            .ok()
                            .and_then(|m| m.modified().ok())
                            .unwrap_or(SystemTime::UNIX_EPOCH);
                        results.push((path.to_string_lossy().to_string(), mtime));
                    }
                }
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use tempfile::tempdir;
            use tokio::fs;

            async fn setup_test_dir() -> tempfile::TempDir {
                let dir = tempdir().unwrap();
                let base = dir.path();

                // Create structure:
                // src/main.rs
                // src/lib.rs
                // src/utils/helpers.rs
                // src/utils/config.rs
                // tests/test_main.rs
                // .git/config
                // node_modules/pkg/index.js
                fs::create_dir_all(base.join("src/utils")).await.unwrap();
                fs::create_dir_all(base.join("tests")).await.unwrap();
                fs::create_dir_all(base.join(".git")).await.unwrap();
                fs::create_dir_all(base.join("node_modules/pkg"))
                    .await
                    .unwrap();

                fs::write(base.join("src/main.rs"), "fn main() {}")
                    .await
                    .unwrap();
                fs::write(base.join("src/lib.rs"), "pub mod utils;")
                    .await
                    .unwrap();
                fs::write(base.join("src/utils/helpers.rs"), "pub fn help() {}")
                    .await
                    .unwrap();
                fs::write(base.join("src/utils/config.rs"), "pub fn cfg() {}")
                    .await
                    .unwrap();
                fs::write(base.join("tests/test_main.rs"), "#[test] fn t() {}")
                    .await
                    .unwrap();
                fs::write(base.join(".git/config"), "[core]").await.unwrap();
                fs::write(
                    base.join("node_modules/pkg/index.js"),
                    "module.exports = {}",
                )
                .await
                .unwrap();

                dir
            }

            #[tokio::test]
            async fn test_glob_star_pattern() {
                let dir = setup_test_dir().await;
                let tool = GlobTool::new().with_base_dir(dir.path());

                let out = tool.execute(json!({ "pattern": "*.rs" })).await.unwrap();
                assert!(out.success);
                // No .rs files at root level
                assert_eq!(out.result["total"], 0);
            }

            #[tokio::test]
            async fn test_glob_double_star() {
                let dir = setup_test_dir().await;
                let tool = GlobTool::new().with_base_dir(dir.path());

                let out = tool.execute(json!({ "pattern": "**/*.rs" })).await.unwrap();
                assert!(out.success);
                let files = out.result["files"].as_array().unwrap();
                // Should find: src/main.rs, src/lib.rs, src/utils/helpers.rs, src/utils/config.rs, tests/test_main.rs
                assert_eq!(files.len(), 5);
            }

            #[tokio::test]
            async fn test_glob_brace_expansion() {
                let dir = setup_test_dir().await;
                let tool = GlobTool::new().with_base_dir(dir.path());

                let out = tool
                    .execute(json!({ "pattern": "src/{main,lib}.rs" }))
                    .await
                    .unwrap();
                assert!(out.success);
                let files = out.result["files"].as_array().unwrap();
                assert_eq!(files.len(), 2);
            }

            #[tokio::test]
            async fn test_glob_sorted_by_mtime() {
                let dir = setup_test_dir().await;
                let tool = GlobTool::new().with_base_dir(dir.path());

                // Write files with slight delay to ensure different mtimes
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                fs::write(
                    dir.path().join("src/main.rs"),
                    "fn main() { /* updated */ }",
                )
                .await
                .unwrap();

                let out = tool
                    .execute(json!({ "pattern": "src/*.rs" }))
                    .await
                    .unwrap();
                assert!(out.success);
                let files = out.result["files"].as_array().unwrap();
                assert_eq!(files.len(), 2);
                // Newest file should be first (main.rs was just updated)
                let first = files[0].as_str().unwrap();
                assert!(first.contains("main.rs"));
            }

            #[tokio::test]
            async fn test_glob_skip_hidden_dirs() {
                let dir = setup_test_dir().await;
                let tool = GlobTool::new().with_base_dir(dir.path());

                let out = tool.execute(json!({ "pattern": "**/*" })).await.unwrap();
                assert!(out.success);
                let files = out.result["files"].as_array().unwrap();
                // Should not include .git/config or node_modules/pkg/index.js
                for f in files {
                    let path = f.as_str().unwrap();
                    assert!(!path.contains(".git/"), "Should skip .git: {}", path);
                    assert!(
                        !path.contains("node_modules/"),
                        "Should skip node_modules: {}",
                        path
                    );
                }
            }

            #[tokio::test]
            async fn test_glob_empty_results() {
                let dir = setup_test_dir().await;
                let tool = GlobTool::new().with_base_dir(dir.path());

                let out = tool.execute(json!({ "pattern": "**/*.py" })).await.unwrap();
                assert!(out.success);
                assert_eq!(out.result["total"], 0);
                assert!(out.result["files"].as_array().unwrap().is_empty());
            }

            #[tokio::test]
            async fn test_glob_with_path_prefix() {
                let dir = setup_test_dir().await;
                let tool = GlobTool::new().with_base_dir(dir.path());

                let out = tool
                    .execute(json!({ "pattern": "src/utils/**/*.rs" }))
                    .await
                    .unwrap();
                assert!(out.success);
                let files = out.result["files"].as_array().unwrap();
                assert_eq!(files.len(), 2); // helpers.rs and config.rs
            }

            #[tokio::test]
            async fn test_glob_nonexistent_dir() {
                let tool = GlobTool::new().with_base_dir("/nonexistent_path_xyz");

                let out = tool.execute(json!({ "pattern": "**/*.rs" })).await.unwrap();
                assert!(!out.success);
            }

            #[tokio::test]
            async fn test_glob_requires_explicit_base_dir_when_scoped() {
                let tool = GlobTool::new().require_base_dir();

                let out = tool.execute(json!({ "pattern": "**/*.rs" })).await.unwrap();
                assert!(!out.success);
                assert!(
                    out.error
                        .as_deref()
                        .unwrap_or_default()
                        .contains("workspace root or base directory")
                );
            }

            #[tokio::test]
            async fn test_glob_rejects_relative_search_path_without_workspace_root() {
                let tool = GlobTool::new();

                let out = tool
                    .execute(json!({
                        "pattern": "**/*.rs",
                        "path": "src"
                    }))
                    .await
                    .unwrap();
                assert!(!out.success);
                assert!(
                    out.error
                        .as_deref()
                        .unwrap_or_default()
                        .contains("Relative search paths require")
                );
            }
        }
    }

    pub mod grep_tool {
        // Grep-like content search tool for AI agents.
        //
        // Provides powerful content search with:
        // - Full regex syntax via the `regex` crate
        // - Context lines around matches (-A/-B/-C)
        // - Multiple output modes (content, files_with_matches, count)
        // - File type and glob filtering
        // - Case-insensitive and multiline matching
        // - Pagination (head_limit, offset)

        use async_trait::async_trait;
        use regex::{Regex, RegexBuilder};
        use serde::Deserialize;
        use serde_json::{Value, json};
        use std::path::{Path, PathBuf};
        use tokio::fs;

        use super::shared::{is_likely_binary, should_skip_grep_dir};
        use crate::Result;
        use crate::{Tool, ToolOutput};

        /// Maximum total matches to collect before stopping
        const MAX_TOTAL_MATCHES: usize = 5000;

        /// Maximum file size to search (5 MB)
        const MAX_FILE_SIZE: u64 = 5 * 1024 * 1024;

        #[derive(Debug, Deserialize)]
        struct GrepInput {
            pattern: String,
            path: Option<String>,
            glob: Option<String>,
            #[serde(rename = "type")]
            file_type: Option<String>,
            output_mode: Option<String>,
            #[serde(rename = "-A")]
            after_context: Option<usize>,
            #[serde(rename = "-B")]
            before_context: Option<usize>,
            #[serde(rename = "-C")]
            context: Option<usize>,
            #[serde(rename = "-i")]
            case_insensitive: Option<bool>,
            #[serde(rename = "-n")]
            show_line_numbers: Option<bool>,
            multiline: Option<bool>,
            head_limit: Option<usize>,
            offset: Option<usize>,
        }

        /// A single match with context
        struct MatchResult {
            file: String,
            line_number: usize,
            line: String,
            before: Vec<(usize, String)>,
            after: Vec<(usize, String)>,
        }

        /// Grep-like content search tool.
        #[derive(Clone)]
        pub struct GrepTool {
            base_dir: Option<PathBuf>,
            require_base_dir: bool,
        }

        impl Default for GrepTool {
            fn default() -> Self {
                Self::new()
            }
        }

        impl GrepTool {
            pub fn new() -> Self {
                Self {
                    base_dir: None,
                    require_base_dir: false,
                }
            }

            pub fn with_base_dir(mut self, base: impl Into<PathBuf>) -> Self {
                self.base_dir = Some(base.into());
                self
            }

            pub fn require_base_dir(mut self) -> Self {
                self.require_base_dir = true;
                self
            }

            fn resolve_base(&self, path: Option<&str>) -> std::result::Result<PathBuf, String> {
                if let Some(p) = path {
                    let candidate = PathBuf::from(p);
                    if candidate.is_absolute() {
                        Ok(candidate)
                    } else if let Some(base) = &self.base_dir {
                        Ok(base.join(candidate))
                    } else {
                        Err(
                            "Relative search paths require an explicit workspace root or base directory."
                                .to_string(),
                        )
                    }
                } else if let Some(base) = &self.base_dir {
                    Ok(base.clone())
                } else if self.require_base_dir {
                    Err(
                        "This tool requires an explicit workspace root or base directory."
                            .to_string(),
                    )
                } else {
                    Err("A search path or base directory is required.".to_string())
                }
            }
        }

        #[async_trait]
        impl Tool for GrepTool {
            fn name(&self) -> &str {
                "grep"
            }

            fn description(&self) -> &str {
                "Search file contents using regex patterns. Supports context lines, output modes (content/files_with_matches/count), file type filtering, case-insensitive and multiline matching."
            }

            fn parameters_schema(&self) -> Value {
                json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "description": "Regex pattern to search for",
                            "type": "string"
                        },
                        "path": {
                            "description": "File or directory to search in. Requires an explicit path or configured workspace root.",
                            "type": "string"
                        },
                        "glob": {
                            "description": "Glob pattern to filter files (e.g. \"*.rs\", \"*.{ts,tsx}\")",
                            "type": "string"
                        },
                        "type": {
                            "description": "File type filter (e.g. \"rust\", \"js\", \"py\", \"go\", \"java\")",
                            "type": "string"
                        },
                        "output_mode": {
                            "description": "Output mode: \"content\" (matching lines with context), \"files_with_matches\" (file paths only), \"count\" (match counts per file)",
                            "type": "string",
                            "enum": ["content", "files_with_matches", "count"],
                            "default": "content"
                        },
                        "-A": {
                            "description": "Number of lines to show after each match",
                            "type": "integer",
                            "minimum": 0
                        },
                        "-B": {
                            "description": "Number of lines to show before each match",
                            "type": "integer",
                            "minimum": 0
                        },
                        "-C": {
                            "description": "Number of context lines before and after each match",
                            "type": "integer",
                            "minimum": 0
                        },
                        "-i": {
                            "description": "Case insensitive search",
                            "type": "boolean"
                        },
                        "-n": {
                            "description": "Show line numbers (default: true for content mode)",
                            "type": "boolean",
                            "default": true
                        },
                        "multiline": {
                            "description": "Enable multiline mode where . matches newlines",
                            "type": "boolean"
                        },
                        "head_limit": {
                            "description": "Maximum number of results to return",
                            "type": "integer",
                            "minimum": 1
                        },
                        "offset": {
                            "description": "Skip first N results before applying head_limit",
                            "type": "integer",
                            "minimum": 0
                        }
                    },
                    "required": ["pattern"]
                })
            }

            async fn execute(&self, input: Value) -> Result<ToolOutput> {
                let params: GrepInput = match serde_json::from_value(input) {
                    Ok(v) => v,
                    Err(err) => return Ok(ToolOutput::error(format!("Invalid input: {}", err))),
                };

                let case_insensitive = params.case_insensitive.unwrap_or(false);
                let multiline = params.multiline.unwrap_or(false);
                let show_line_numbers = params.show_line_numbers.unwrap_or(true);
                let output_mode = params.output_mode.as_deref().unwrap_or("content");
                let offset = params.offset.unwrap_or(0);
                let head_limit = params.head_limit.unwrap_or(0); // 0 = unlimited

                // Context lines: -C overrides -A/-B
                let before_ctx = params.context.or(params.before_context).unwrap_or(0);
                let after_ctx = params.context.or(params.after_context).unwrap_or(0);

                // Build regex
                let regex = match RegexBuilder::new(&params.pattern)
                    .case_insensitive(case_insensitive)
                    .dot_matches_new_line(multiline)
                    .multi_line(multiline)
                    .build()
                {
                    Ok(r) => r,
                    Err(err) => return Ok(ToolOutput::error(format!("Invalid regex: {}", err))),
                };

                let base = match self.resolve_base(params.path.as_deref()) {
                    Ok(base) => base,
                    Err(error) => return Ok(ToolOutput::error(error)),
                };

                // If path is a file, search just that file
                if base.is_file() {
                    let search_opts = SearchOpts {
                        output_mode,
                        before_ctx,
                        after_ctx,
                        show_line_numbers,
                        offset,
                        head_limit,
                    };
                    return search_single_file(&base, &regex, &search_opts).await;
                }

                if !base.is_dir() {
                    return Ok(ToolOutput::error(format!(
                        "Path not found: {}",
                        base.display()
                    )));
                }

                // Collect all files
                let mut files = Vec::new();
                collect_files(&base, &params.glob, &params.file_type, &mut files).await;

                match output_mode {
                    "files_with_matches" => {
                        let mut matching_files = Vec::new();
                        for file_path in &files {
                            if let Ok(content) = fs::read_to_string(file_path).await
                                && regex.is_match(&content)
                            {
                                matching_files.push(file_path.to_string_lossy().to_string());
                            }
                        }

                        let total = matching_files.len();
                        let matching_files: Vec<_> = matching_files
                            .into_iter()
                            .skip(offset)
                            .take(if head_limit > 0 {
                                head_limit
                            } else {
                                usize::MAX
                            })
                            .collect();

                        Ok(ToolOutput::success(json!({
                            "files": matching_files,
                            "total": total
                        })))
                    }
                    "count" => {
                        let mut counts: Vec<Value> = Vec::new();
                        for file_path in &files {
                            if let Ok(content) = fs::read_to_string(file_path).await {
                                let count = regex.find_iter(&content).count();
                                if count > 0 {
                                    counts.push(json!({
                                        "file": file_path.to_string_lossy(),
                                        "count": count
                                    }));
                                }
                            }
                        }

                        let total = counts.len();
                        let counts: Vec<_> = counts
                            .into_iter()
                            .skip(offset)
                            .take(if head_limit > 0 {
                                head_limit
                            } else {
                                usize::MAX
                            })
                            .collect();

                        Ok(ToolOutput::success(json!({
                            "counts": counts,
                            "total": total
                        })))
                    }
                    _ => {
                        // "content" mode — collect matches with context
                        let mut all_matches: Vec<MatchResult> = Vec::new();

                        for file_path in &files {
                            if all_matches.len() >= MAX_TOTAL_MATCHES {
                                break;
                            }

                            let content = match fs::read_to_string(file_path).await {
                                Ok(c) => c,
                                Err(_) => continue,
                            };

                            let lines: Vec<&str> = content.lines().collect();
                            let file_str = file_path.to_string_lossy().to_string();

                            for (i, line) in lines.iter().enumerate() {
                                if regex.is_match(line) {
                                    let before: Vec<(usize, String)> = if before_ctx > 0 {
                                        let start = i.saturating_sub(before_ctx);
                                        (start..i).map(|j| (j + 1, lines[j].to_string())).collect()
                                    } else {
                                        Vec::new()
                                    };

                                    let after: Vec<(usize, String)> = if after_ctx > 0 {
                                        let end = (i + 1 + after_ctx).min(lines.len());
                                        ((i + 1)..end)
                                            .map(|j| (j + 1, lines[j].to_string()))
                                            .collect()
                                    } else {
                                        Vec::new()
                                    };

                                    all_matches.push(MatchResult {
                                        file: file_str.clone(),
                                        line_number: i + 1,
                                        line: line.to_string(),
                                        before,
                                        after,
                                    });

                                    if all_matches.len() >= MAX_TOTAL_MATCHES {
                                        break;
                                    }
                                }
                            }
                        }

                        let total = all_matches.len();
                        let matches: Vec<_> = all_matches
                            .into_iter()
                            .skip(offset)
                            .take(if head_limit > 0 {
                                head_limit
                            } else {
                                usize::MAX
                            })
                            .collect();

                        // Format output as text
                        let mut output = String::new();
                        let mut last_file = String::new();

                        for m in &matches {
                            if m.file != last_file {
                                if !output.is_empty() {
                                    output.push('\n');
                                }
                                output.push_str(&m.file);
                                output.push('\n');
                                last_file.clone_from(&m.file);
                            }

                            // Before context
                            for (ln, text) in &m.before {
                                if show_line_numbers {
                                    output.push_str(&format!("{}-{}\n", ln, text));
                                } else {
                                    output.push_str(&format!("-{}\n", text));
                                }
                            }

                            // Match line
                            if show_line_numbers {
                                output.push_str(&format!("{}:{}\n", m.line_number, m.line));
                            } else {
                                output.push_str(&format!(":{}\n", m.line));
                            }

                            // After context
                            for (ln, text) in &m.after {
                                if show_line_numbers {
                                    output.push_str(&format!("{}-{}\n", ln, text));
                                } else {
                                    output.push_str(&format!("-{}\n", text));
                                }
                            }

                            if !m.before.is_empty() || !m.after.is_empty() {
                                output.push_str("--\n");
                            }
                        }

                        Ok(ToolOutput::success(json!({
                            "output": output.trim_end(),
                            "match_count": total
                        })))
                    }
                }
            }
        }

        /// Options for single-file search
        struct SearchOpts<'a> {
            output_mode: &'a str,
            before_ctx: usize,
            after_ctx: usize,
            show_line_numbers: bool,
            offset: usize,
            head_limit: usize,
        }

        /// Search a single file and return results.
        async fn search_single_file(
            path: &Path,
            regex: &Regex,
            opts: &SearchOpts<'_>,
        ) -> Result<ToolOutput> {
            let content = match fs::read_to_string(path).await {
                Ok(c) => c,
                Err(err) => return Ok(ToolOutput::error(format!("Cannot read file: {}", err))),
            };

            let file_str = path.to_string_lossy().to_string();

            match opts.output_mode {
                "files_with_matches" => {
                    if regex.is_match(&content) {
                        Ok(ToolOutput::success(json!({
                            "files": [file_str],
                            "total": 1
                        })))
                    } else {
                        Ok(ToolOutput::success(json!({
                            "files": [],
                            "total": 0
                        })))
                    }
                }
                "count" => {
                    let count = regex.find_iter(&content).count();
                    if count > 0 {
                        Ok(ToolOutput::success(json!({
                            "counts": [{ "file": file_str, "count": count }],
                            "total": 1
                        })))
                    } else {
                        Ok(ToolOutput::success(json!({
                            "counts": [],
                            "total": 0
                        })))
                    }
                }
                _ => {
                    let lines: Vec<&str> = content.lines().collect();
                    let mut matches: Vec<MatchResult> = Vec::new();

                    for (i, line) in lines.iter().enumerate() {
                        if regex.is_match(line) {
                            let before: Vec<(usize, String)> = if opts.before_ctx > 0 {
                                let start = i.saturating_sub(opts.before_ctx);
                                (start..i).map(|j| (j + 1, lines[j].to_string())).collect()
                            } else {
                                Vec::new()
                            };

                            let after: Vec<(usize, String)> = if opts.after_ctx > 0 {
                                let end = (i + 1 + opts.after_ctx).min(lines.len());
                                ((i + 1)..end)
                                    .map(|j| (j + 1, lines[j].to_string()))
                                    .collect()
                            } else {
                                Vec::new()
                            };

                            matches.push(MatchResult {
                                file: file_str.clone(),
                                line_number: i + 1,
                                line: line.to_string(),
                                before,
                                after,
                            });
                        }
                    }

                    let total = matches.len();
                    let matches: Vec<_> = matches
                        .into_iter()
                        .skip(opts.offset)
                        .take(if opts.head_limit > 0 {
                            opts.head_limit
                        } else {
                            usize::MAX
                        })
                        .collect();

                    let mut output = String::new();
                    if !matches.is_empty() {
                        output.push_str(&file_str);
                        output.push('\n');
                    }

                    for m in &matches {
                        for (ln, text) in &m.before {
                            if opts.show_line_numbers {
                                output.push_str(&format!("{}-{}\n", ln, text));
                            } else {
                                output.push_str(&format!("-{}\n", text));
                            }
                        }

                        if opts.show_line_numbers {
                            output.push_str(&format!("{}:{}\n", m.line_number, m.line));
                        } else {
                            output.push_str(&format!(":{}\n", m.line));
                        }

                        for (ln, text) in &m.after {
                            if opts.show_line_numbers {
                                output.push_str(&format!("{}-{}\n", ln, text));
                            } else {
                                output.push_str(&format!("-{}\n", text));
                            }
                        }

                        if !m.before.is_empty() || !m.after.is_empty() {
                            output.push_str("--\n");
                        }
                    }

                    Ok(ToolOutput::success(json!({
                        "output": output.trim_end(),
                        "match_count": total
                    })))
                }
            }
        }

        /// Recursively collect files matching optional glob and type filters.
        #[async_recursion::async_recursion]
        async fn collect_files(
            dir: &Path,
            glob_filter: &Option<String>,
            type_filter: &Option<String>,
            files: &mut Vec<PathBuf>,
        ) {
            let mut entries = match fs::read_dir(dir).await {
                Ok(entries) => entries,
                Err(_) => return,
            };

            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                let file_name = match entry.file_name().into_string() {
                    Ok(s) => s,
                    Err(_) => continue,
                };

                // Skip hidden directories and known generated dirs
                if path.is_dir() {
                    if should_skip_grep_dir(&file_name) {
                        continue;
                    }
                    collect_files(&path, glob_filter, type_filter, files).await;
                    continue;
                }

                // Skip binary files
                if is_likely_binary(&file_name) {
                    continue;
                }

                // Check file size
                if let Ok(meta) = entry.metadata().await
                    && meta.len() > MAX_FILE_SIZE
                {
                    continue;
                }

                // Apply type filter
                if let Some(ft) = type_filter {
                    let exts = extensions_for_type(ft);
                    if !exts.is_empty() {
                        let has_ext = exts
                            .iter()
                            .any(|ext| file_name.ends_with(&format!(".{}", ext)));
                        if !has_ext {
                            continue;
                        }
                    }
                }

                // Apply glob filter
                if let Some(glob_pat) = glob_filter
                    && !glob_match::glob_match(glob_pat, &file_name)
                {
                    continue;
                }

                files.push(path);
            }
        }

        /// Map file type names to file extensions.
        fn extensions_for_type(file_type: &str) -> &[&str] {
            match file_type {
                "rust" | "rs" => &["rs"],
                "js" | "javascript" => &["js", "jsx", "mjs"],
                "ts" | "typescript" => &["ts", "tsx", "mts"],
                "py" | "python" => &["py", "pyi"],
                "go" => &["go"],
                "java" => &["java"],
                "c" => &["c", "h"],
                "cpp" | "c++" => &["cpp", "hpp", "cc", "hh", "cxx"],
                "ruby" | "rb" => &["rb"],
                "swift" => &["swift"],
                "kotlin" | "kt" => &["kt", "kts"],
                "scala" => &["scala"],
                "php" => &["php"],
                "html" => &["html", "htm"],
                "css" => &["css"],
                "json" => &["json"],
                "yaml" | "yml" => &["yaml", "yml"],
                "toml" => &["toml"],
                "xml" => &["xml"],
                "sql" => &["sql"],
                "sh" | "shell" | "bash" => &["sh", "bash", "zsh"],
                "md" | "markdown" => &["md", "markdown"],
                "vue" => &["vue"],
                "svelte" => &["svelte"],
                _ => &[],
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use tempfile::tempdir;
            use tokio::fs;

            async fn setup_test_dir() -> tempfile::TempDir {
                let dir = tempdir().unwrap();
                let base = dir.path();

                fs::create_dir_all(base.join("src")).await.unwrap();
                fs::create_dir_all(base.join("tests")).await.unwrap();

                fs::write(
                    base.join("src/main.rs"),
                    "fn main() {\n    println!(\"Hello, world!\");\n}\n",
                )
                .await
                .unwrap();
                fs::write(
                    base.join("src/lib.rs"),
                    "pub fn greet(name: &str) -> String {\n    format!(\"Hello, {}!\", name)\n}\n",
                )
                .await
                .unwrap();
                fs::write(
                    base.join("tests/test.rs"),
                    "#[test]\nfn test_greet() {\n    assert_eq!(greet(\"world\"), \"Hello, world!\");\n}\n",
                )
                .await
                .unwrap();
                fs::write(base.join("src/config.json"), "{\"key\": \"value\"}\n")
                    .await
                    .unwrap();

                dir
            }

            #[tokio::test]
            async fn test_grep_basic_match() {
                let dir = setup_test_dir().await;
                let tool = GrepTool::new().with_base_dir(dir.path());

                let out = tool.execute(json!({ "pattern": "Hello" })).await.unwrap();
                assert!(out.success);
                assert!(out.result["match_count"].as_u64().unwrap() > 0);
                let text = out.result["output"].as_str().unwrap();
                assert!(text.contains("Hello"));
            }

            #[tokio::test]
            async fn test_grep_context_lines() {
                let dir = setup_test_dir().await;
                let tool = GrepTool::new().with_base_dir(dir.path());

                let out = tool
                    .execute(json!({
                        "pattern": "println",
                        "-B": 1,
                        "-A": 1
                    }))
                    .await
                    .unwrap();
                assert!(out.success);
                let text = out.result["output"].as_str().unwrap();
                // Should contain context lines around "println"
                assert!(text.contains("fn main()"));
            }

            #[tokio::test]
            async fn test_grep_case_insensitive() {
                let dir = setup_test_dir().await;
                let tool = GrepTool::new().with_base_dir(dir.path());

                let out = tool
                    .execute(json!({
                        "pattern": "hello",
                        "-i": true
                    }))
                    .await
                    .unwrap();
                assert!(out.success);
                assert!(out.result["match_count"].as_u64().unwrap() > 0);
            }

            #[tokio::test]
            async fn test_grep_files_with_matches_mode() {
                let dir = setup_test_dir().await;
                let tool = GrepTool::new().with_base_dir(dir.path());

                let out = tool
                    .execute(json!({
                        "pattern": "fn",
                        "output_mode": "files_with_matches"
                    }))
                    .await
                    .unwrap();
                assert!(out.success);
                let files = out.result["files"].as_array().unwrap();
                // main.rs, lib.rs, and test.rs all contain "fn"
                assert!(files.len() >= 2);
            }

            #[tokio::test]
            async fn test_grep_count_mode() {
                let dir = setup_test_dir().await;
                let tool = GrepTool::new().with_base_dir(dir.path());

                let out = tool
                    .execute(json!({
                        "pattern": "fn",
                        "output_mode": "count"
                    }))
                    .await
                    .unwrap();
                assert!(out.success);
                let counts = out.result["counts"].as_array().unwrap();
                assert!(!counts.is_empty());
                for c in counts {
                    assert!(c["count"].as_u64().unwrap() > 0);
                }
            }

            #[tokio::test]
            async fn test_grep_file_type_filter() {
                let dir = setup_test_dir().await;
                let tool = GrepTool::new().with_base_dir(dir.path());

                let out = tool
                    .execute(json!({
                        "pattern": "key",
                        "type": "json",
                        "output_mode": "files_with_matches"
                    }))
                    .await
                    .unwrap();
                assert!(out.success);
                let files = out.result["files"].as_array().unwrap();
                assert_eq!(files.len(), 1);
                assert!(files[0].as_str().unwrap().ends_with("config.json"));
            }

            #[tokio::test]
            async fn test_grep_head_limit_offset() {
                let dir = setup_test_dir().await;
                let tool = GrepTool::new().with_base_dir(dir.path());

                let out = tool
                    .execute(json!({
                        "pattern": "fn",
                        "head_limit": 1
                    }))
                    .await
                    .unwrap();
                assert!(out.success);
                // Total match count should be > 1, but output limited to 1
                let total = out.result["match_count"].as_u64().unwrap();
                assert!(total >= 1);
                let text = out.result["output"].as_str().unwrap();
                // Only one file:line match should appear
                let match_lines: Vec<_> = text.lines().filter(|l| l.contains(':')).collect();
                assert_eq!(match_lines.len(), 1);
            }

            #[tokio::test]
            async fn test_grep_multiline() {
                let dir = setup_test_dir().await;
                let tool = GrepTool::new().with_base_dir(dir.path());

                // Without multiline, this should still work line-by-line
                let out = tool
                    .execute(json!({
                        "pattern": "pub fn",
                        "output_mode": "files_with_matches"
                    }))
                    .await
                    .unwrap();
                assert!(out.success);
                let files = out.result["files"].as_array().unwrap();
                assert!(files.iter().any(|f| f.as_str().unwrap().contains("lib.rs")));
            }

            #[tokio::test]
            async fn test_grep_skip_binary() {
                let dir = setup_test_dir().await;
                // Create a binary file
                fs::write(dir.path().join("image.png"), b"\x89PNG\r\n\x1a\n")
                    .await
                    .unwrap();

                let tool = GrepTool::new().with_base_dir(dir.path());

                let out = tool
                    .execute(json!({
                        "pattern": "PNG",
                        "output_mode": "files_with_matches"
                    }))
                    .await
                    .unwrap();
                assert!(out.success);
                let files = out.result["files"].as_array().unwrap();
                // png should be skipped
                assert!(!files.iter().any(|f| f.as_str().unwrap().contains(".png")));
            }

            #[tokio::test]
            async fn test_grep_glob_filter() {
                let dir = setup_test_dir().await;
                let tool = GrepTool::new().with_base_dir(dir.path());

                let out = tool
                    .execute(json!({
                        "pattern": "fn",
                        "glob": "*.rs",
                        "output_mode": "files_with_matches"
                    }))
                    .await
                    .unwrap();
                assert!(out.success);
                let files = out.result["files"].as_array().unwrap();
                for f in files {
                    assert!(f.as_str().unwrap().ends_with(".rs"));
                }
            }

            #[tokio::test]
            async fn test_grep_no_matches() {
                let dir = setup_test_dir().await;
                let tool = GrepTool::new().with_base_dir(dir.path());

                let out = tool
                    .execute(json!({ "pattern": "NONEXISTENT_STRING_XYZ" }))
                    .await
                    .unwrap();
                assert!(out.success);
                assert_eq!(out.result["match_count"], 0);
            }

            #[tokio::test]
            async fn test_grep_requires_explicit_base_dir_when_scoped() {
                let tool = GrepTool::new().require_base_dir();

                let out = tool.execute(json!({ "pattern": "Hello" })).await.unwrap();
                assert!(!out.success);
                assert!(
                    out.error
                        .as_deref()
                        .unwrap_or_default()
                        .contains("workspace root or base directory")
                );
            }

            #[tokio::test]
            async fn test_grep_rejects_relative_search_path_without_workspace_root() {
                let tool = GrepTool::new();

                let out = tool
                    .execute(json!({
                        "pattern": "Hello",
                        "path": "src"
                    }))
                    .await
                    .unwrap();
                assert!(!out.success);
                assert!(
                    out.error
                        .as_deref()
                        .unwrap_or_default()
                        .contains("Relative search paths require")
                );
            }
        }
    }

    pub mod batch {
        // Batch tool — execute up to 25 tool calls in a single invocation.
        //
        // Allows the LLM to batch multiple independent tool calls into one round-trip,
        // avoiding the overhead of 25 separate LLM calls. Each sub-invocation runs in
        // parallel with bounded concurrency.

        use async_trait::async_trait;
        use futures::StreamExt;
        use futures::stream::FuturesUnordered;
        use serde::{Deserialize, Serialize};
        use serde_json::{Value, json};
        use std::collections::HashMap;
        use std::sync::Arc;
        use std::time::Duration;
        use tokio::sync::Semaphore;

        use crate::ToolRegistry;
        use crate::{Result, ToolError};
        use crate::{Tool, ToolOutput};

        /// Maximum number of sub-invocations per batch call.
        const MAX_BATCH_SIZE: usize = 25;

        /// A single tool invocation within a batch.
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct BatchInvocation {
            /// Name of the tool to invoke.
            pub tool: String,
            /// Input arguments for the tool.
            pub input: Value,
        }

        /// Parameters for the batch tool.
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct BatchParams {
            /// Array of tool invocations (max 25).
            pub invocations: Vec<BatchInvocation>,
            /// Continue executing remaining invocations if one fails (default: true).
            #[serde(default = "default_continue_on_error")]
            pub continue_on_error: bool,
            /// Optional per-invocation timeout in seconds.
            /// When omitted, sub-invocations are not timed out by this tool.
            pub timeout_secs: Option<u64>,
        }

        fn default_continue_on_error() -> bool {
            true
        }

        /// Batch tool that executes multiple tool calls in parallel.
        pub struct BatchTool {
            tools: Arc<ToolRegistry>,
        }

        impl BatchTool {
            /// Create a new batch tool backed by the given tool registry.
            pub fn new(tools: Arc<ToolRegistry>) -> Self {
                Self { tools }
            }
        }

        #[async_trait]
        impl Tool for BatchTool {
            fn name(&self) -> &str {
                "batch"
            }

            fn description(&self) -> &str {
                "Execute up to 25 tool calls in a single invocation. Each sub-call runs in parallel. \
                 Use this to batch multiple independent operations and avoid round-trip overhead."
            }

            fn parameters_schema(&self) -> Value {
                json!({
                    "type": "object",
                    "properties": {
                        "invocations": {
                            "type": "array",
                            "description": "Array of tool invocations to execute in parallel (max 25)",
                            "maxItems": MAX_BATCH_SIZE,
                            "items": {
                                "type": "object",
                                "properties": {
                                    "tool": {
                                        "type": "string",
                                        "description": "Name of the tool to invoke"
                                    },
                                    "input": {
                                        "type": "object",
                                        "description": "Input arguments for the tool"
                                    }
                                },
                                "required": ["tool", "input"]
                            }
                        },
                        "continue_on_error": {
                            "type": "boolean",
                            "default": true,
                            "description": "Continue executing remaining invocations if one fails (default: true)"
                        },
                        "timeout_secs": {
                            "type": "integer",
                            "description": "Optional per-invocation timeout in seconds. If omitted, no timeout is applied (the executor's wrapper timeout still applies)."
                        }
                    },
                    "required": ["invocations"]
                })
            }

            async fn execute(&self, input: Value) -> Result<ToolOutput> {
                let params: BatchParams = serde_json::from_value(input)
                    .map_err(|e| ToolError::Tool(format!("Invalid batch parameters: {}", e)))?;

                if params.invocations.is_empty() {
                    return Ok(ToolOutput::success(json!({
                        "results": [],
                        "summary": { "total": 0, "succeeded": 0, "failed": 0 }
                    })));
                }

                if params.invocations.len() > MAX_BATCH_SIZE {
                    return Err(ToolError::Tool(format!(
                        "Batch size {} exceeds maximum of {}",
                        params.invocations.len(),
                        MAX_BATCH_SIZE
                    )));
                }

                // Prevent recursion: reject if any invocation calls "batch"
                for inv in &params.invocations {
                    if inv.tool == "batch" {
                        return Err(ToolError::Tool(
                            "Recursive batch calls are not allowed".to_string(),
                        ));
                    }
                }

                let continue_on_error = params.continue_on_error;
                let timeout = params.timeout_secs.map(Duration::from_secs);
                let semaphore = Arc::new(Semaphore::new(MAX_BATCH_SIZE));
                let mut unordered = FuturesUnordered::new();
                let mut pending = HashMap::new();

                for (idx, inv) in params.invocations.into_iter().enumerate() {
                    let tools = Arc::clone(&self.tools);
                    let sem = Arc::clone(&semaphore);
                    let tool_name = inv.tool;
                    pending.insert(idx, tool_name.clone());
                    let tool_input = inv.input;
                    let tool_timeout = timeout;

                    unordered.push(async move {
                        let _permit = sem.acquire().await;
                        let result = if let Some(t) = tool_timeout {
                            tokio::time::timeout(t, tools.execute_safe(&tool_name, tool_input))
                                .await
                                .unwrap_or_else(|_| {
                                    Err(ToolError::Tool(format!("Tool '{}' timed out", tool_name)))
                                })
                        } else {
                            tools.execute_safe(&tool_name, tool_input).await
                        };
                        (idx, tool_name, result)
                    });
                }

                let mut results = Vec::new();
                let mut succeeded = 0usize;
                let mut failed = 0usize;
                let mut skipped = 0usize;

                while let Some((idx, tool_name, result)) = unordered.next().await {
                    pending.remove(&idx);
                    let entry = match result {
                        Ok(output) if output.success => {
                            succeeded += 1;
                            json!({
                                "index": idx,
                                "tool": tool_name,
                                "success": true,
                                "output": output.result
                            })
                        }
                        Ok(output) => {
                            failed += 1;
                            json!({
                                "index": idx,
                                "tool": tool_name,
                                "success": false,
                                "error": output.error.unwrap_or_else(|| "unknown error".to_string())
                            })
                        }
                        Err(e) => {
                            failed += 1;
                            json!({
                                "index": idx,
                                "tool": tool_name,
                                "success": false,
                                "error": e.to_string()
                            })
                        }
                    };
                    results.push(entry);

                    if !continue_on_error && failed > 0 {
                        // Mark remaining invocations as skipped and stop.
                        let mut skipped_entries: Vec<_> = pending
                            .into_iter()
                            .map(|(pending_idx, pending_tool)| {
                                json!({
                                    "index": pending_idx,
                                    "tool": pending_tool,
                                    "success": false,
                                    "skipped": true,
                                    "error": "Skipped due to earlier failure"
                                })
                            })
                            .collect();
                        skipped = skipped_entries.len();
                        results.append(&mut skipped_entries);
                        break;
                    }
                }

                results.sort_by_key(|entry| {
                    entry
                        .get("index")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(u64::MAX)
                });

                let total = succeeded + failed + skipped;
                Ok(ToolOutput::success(json!({
                    "results": results,
                    "summary": {
                        "total": total,
                        "succeeded": succeeded,
                        "failed": failed,
                        "skipped": skipped
                    }
                })))
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            /// A simple echo tool for testing.
            struct EchoTool;

            #[async_trait]
            impl Tool for EchoTool {
                fn name(&self) -> &str {
                    "echo"
                }
                fn description(&self) -> &str {
                    "Echo input back"
                }
                fn parameters_schema(&self) -> Value {
                    json!({"type": "object"})
                }
                async fn execute(&self, input: Value) -> Result<ToolOutput> {
                    Ok(ToolOutput::success(input))
                }
            }

            /// A tool that always fails.
            struct FailTool;

            #[async_trait]
            impl Tool for FailTool {
                fn name(&self) -> &str {
                    "fail"
                }
                fn description(&self) -> &str {
                    "Always fails"
                }
                fn parameters_schema(&self) -> Value {
                    json!({"type": "object"})
                }
                async fn execute(&self, _input: Value) -> Result<ToolOutput> {
                    Ok(ToolOutput::error("intentional failure"))
                }
            }

            /// A tool that succeeds after a delay.
            struct SlowEchoTool;

            #[async_trait]
            impl Tool for SlowEchoTool {
                fn name(&self) -> &str {
                    "slow_echo"
                }
                fn description(&self) -> &str {
                    "Echo input back after a delay"
                }
                fn parameters_schema(&self) -> Value {
                    json!({"type": "object"})
                }
                async fn execute(&self, input: Value) -> Result<ToolOutput> {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    Ok(ToolOutput::success(input))
                }
            }

            fn make_registry() -> Arc<ToolRegistry> {
                let mut registry = ToolRegistry::new();
                registry.register(EchoTool);
                registry.register(FailTool);
                registry.register(SlowEchoTool);
                Arc::new(registry)
            }

            #[tokio::test]
            async fn test_batch_two_echo_tools() {
                let registry = make_registry();
                let batch = BatchTool::new(registry);
                let result = batch
                    .execute(json!({
                        "invocations": [
                            { "tool": "echo", "input": { "msg": "hello" } },
                            { "tool": "echo", "input": { "msg": "world" } }
                        ]
                    }))
                    .await
                    .unwrap();

                assert!(result.success);
                let results = result.result["results"].as_array().unwrap();
                assert_eq!(results.len(), 2);
                assert!(results[0]["success"].as_bool().unwrap());
                assert!(results[1]["success"].as_bool().unwrap());
                assert_eq!(result.result["summary"]["succeeded"], 2);
                assert_eq!(result.result["summary"]["failed"], 0);
            }

            #[tokio::test]
            async fn test_batch_partial_failure_continue() {
                let registry = make_registry();
                let batch = BatchTool::new(registry);
                let result = batch
                    .execute(json!({
                        "invocations": [
                            { "tool": "echo", "input": { "msg": "ok" } },
                            { "tool": "fail", "input": {} },
                            { "tool": "echo", "input": { "msg": "also ok" } }
                        ],
                        "continue_on_error": true
                    }))
                    .await
                    .unwrap();

                assert!(result.success);
                let results = result.result["results"].as_array().unwrap();
                assert_eq!(results.len(), 3);
                assert!(results[0]["success"].as_bool().unwrap());
                assert!(!results[1]["success"].as_bool().unwrap());
                assert!(results[2]["success"].as_bool().unwrap());
                assert_eq!(result.result["summary"]["succeeded"], 2);
                assert_eq!(result.result["summary"]["failed"], 1);
            }

            #[tokio::test]
            async fn test_batch_exceeds_max_size() {
                let registry = make_registry();
                let batch = BatchTool::new(registry);
                let invocations: Vec<Value> = (0..26)
                    .map(|i| json!({ "tool": "echo", "input": { "i": i } }))
                    .collect();
                let result = batch.execute(json!({ "invocations": invocations })).await;

                assert!(result.is_err());
                let err = result.unwrap_err().to_string();
                assert!(err.contains("exceeds maximum"));
            }

            #[tokio::test]
            async fn test_batch_recursive_rejected() {
                let registry = make_registry();
                let batch = BatchTool::new(registry);
                let result = batch
                    .execute(json!({
                        "invocations": [
                            { "tool": "batch", "input": { "invocations": [] } }
                        ]
                    }))
                    .await;

                assert!(result.is_err());
                let err = result.unwrap_err().to_string();
                assert!(err.contains("Recursive batch"));
            }

            #[tokio::test]
            async fn test_batch_empty() {
                let registry = make_registry();
                let batch = BatchTool::new(registry);
                let result = batch.execute(json!({ "invocations": [] })).await.unwrap();

                assert!(result.success);
                assert_eq!(result.result["summary"]["total"], 0);
            }

            #[tokio::test]
            async fn test_batch_stop_on_error_marks_skipped() {
                let registry = make_registry();
                let batch = BatchTool::new(registry);
                let result = batch
                    .execute(json!({
                        "invocations": [
                            { "tool": "fail", "input": {} },
                            { "tool": "slow_echo", "input": { "msg": "one" } },
                            { "tool": "slow_echo", "input": { "msg": "two" } }
                        ],
                        "continue_on_error": false
                    }))
                    .await
                    .unwrap();

                assert!(result.success);
                let summary = &result.result["summary"];
                assert_eq!(summary["failed"].as_u64().unwrap(), 1);
                assert_eq!(summary["skipped"].as_u64().unwrap(), 2);
                assert_eq!(summary["total"].as_u64().unwrap(), 3);

                let results = result.result["results"].as_array().unwrap();
                assert_eq!(results.len(), 3);
                assert_eq!(results[0]["tool"], "fail");
                assert_eq!(results[1]["skipped"], true);
                assert_eq!(results[2]["skipped"], true);
            }
        }
    }

    pub mod list_subagents {
        // list_subagents tool - List available sub-agent definitions and running sub-agents.

        use async_trait::async_trait;
        use serde::{Deserialize, Serialize};
        use serde_json::{Value, json};
        use std::sync::Arc;

        use crate::impls::subagent_read_capability::SubagentReadCapabilityService;
        use crate::{Result, ToolError};
        use crate::{Tool, ToolOutput};
        use types::SubagentManager;

        /// Parameters for list_subagents tool.
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct ListSubagentsParams {
            /// Include currently running agents in the response.
            #[serde(default = "default_include_running")]
            pub include_running: bool,

            /// Parent run scope for running agents.
            #[serde(default)]
            pub parent_run_id: Option<String>,
        }

        fn default_include_running() -> bool {
            true
        }

        /// list_subagents tool for the shared agent execution engine.
        pub struct ListSubagentsTool {
            manager: Arc<dyn SubagentManager>,
            capability: SubagentReadCapabilityService,
        }

        impl ListSubagentsTool {
            pub fn new(manager: Arc<dyn SubagentManager>) -> Self {
                let capability = SubagentReadCapabilityService::new(manager.clone());
                Self {
                    manager,
                    capability,
                }
            }
        }

        #[async_trait]
        impl Tool for ListSubagentsTool {
            fn name(&self) -> &str {
                "list_subagents"
            }

            fn description(&self) -> &str {
                "List available agent types and currently running agents."
            }

            fn parameters_schema(&self) -> Value {
                json!({
                    "type": "object",
                    "properties": {
                        "include_running": {
                            "type": "boolean",
                            "default": true,
                            "description": "Include currently running agents"
                        },
                        "parent_run_id": {
                            "type": "string",
                            "description": "Optional parent run scope. When omitted, running_agents is hidden from the global view."
                        }
                    }
                })
            }

            async fn execute(&self, input: Value) -> Result<ToolOutput> {
                let params: ListSubagentsParams = serde_json::from_value(input)
                    .map_err(|e| ToolError::Tool(format!("Invalid parameters: {}", e)))?;

                let available: Vec<Value> = self
                    .manager
                    .list_callable()
                    .iter()
                    .map(|def| {
                        json!({
                            "id": def.id,
                            "name": def.name,
                            "description": def.description,
                            "tags": def.tags
                        })
                    })
                    .collect();

                let mut response = json!({ "available_agents": available });

                if params.include_running {
                    let running: Vec<Value> = self
                        .capability
                        .list_running_for_parent(params.parent_run_id.as_deref())
                        .iter()
                        .map(|state| {
                            json!({
                                "task_id": state.id,
                                "agent": state.agent_name,
                                "task": state.task,
                                "status": format!("{:?}", state.status),
                                "started_at": state.started_at
                            })
                        })
                        .collect();

                    response["running_agents"] = json!(running);
                    response["running_count"] = json!(
                        self.capability
                            .running_count_for_parent(params.parent_run_id.as_deref())
                    );
                }

                Ok(ToolOutput::success(response))
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use crate::Tool;
            use crate::ToolRegistry;
            use ::agent::agent::{
                SubagentConfig, SubagentDefLookup, SubagentDefSnapshot, SubagentDefSummary,
                SubagentDeps, SubagentManagerImpl, SubagentTracker,
            };
            use ::agent::llm::{MockLlmClient, MockStep};
            use std::collections::HashMap;
            use tokio::sync::mpsc;
            use types::SubagentManager;
            use types::request::RunSpawnRequest as ContractRunSpawnRequest;

            struct MockDefLookup {
                defs: HashMap<String, SubagentDefSnapshot>,
                summaries: Vec<SubagentDefSummary>,
            }

            impl MockDefLookup {
                fn with_agents(agents: Vec<(&str, &str)>) -> Self {
                    let mut defs = HashMap::new();
                    let mut summaries = Vec::new();
                    for (id, name) in agents {
                        defs.insert(
                            id.to_string(),
                            SubagentDefSnapshot {
                                name: name.to_string(),
                                system_prompt: format!("You are a {} agent.", name),
                                allowed_tools: vec![],
                                max_iterations: Some(1),
                                default_model: None,
                            },
                        );
                        summaries.push(SubagentDefSummary {
                            id: id.to_string(),
                            name: name.to_string(),
                            description: format!("{} agent", name),
                            tags: vec![],
                        });
                    }
                    Self { defs, summaries }
                }

                fn empty() -> Self {
                    Self {
                        defs: HashMap::new(),
                        summaries: Vec::new(),
                    }
                }
            }

            impl SubagentDefLookup for MockDefLookup {
                fn lookup(&self, id: &str) -> Option<SubagentDefSnapshot> {
                    self.defs.get(id).cloned()
                }
                fn list_callable(&self) -> Vec<SubagentDefSummary> {
                    self.summaries.clone()
                }
            }

            fn make_deps(lookup: MockDefLookup, mock_steps: Vec<MockStep>) -> Arc<SubagentDeps> {
                let (tx, rx) = mpsc::channel(16);
                let tracker = Arc::new(SubagentTracker::new(tx, rx));
                let definitions: Arc<dyn SubagentDefLookup> = Arc::new(lookup);
                let llm_client = Arc::new(MockLlmClient::from_steps("mock", mock_steps));
                let tool_registry = Arc::new(ToolRegistry::new());
                let config = SubagentConfig {
                    max_parallel_agents: 5,
                    subagent_timeout_secs: 10,
                    max_iterations: 5,
                    max_depth: 1,
                };
                Arc::new(SubagentDeps {
                    tracker,
                    definitions,
                    llm_client,
                    tool_registry,
                    config,
                    llm_client_factory: None,
                    orchestrator: None,
                })
            }

            fn as_manager(deps: &Arc<SubagentDeps>) -> Arc<dyn SubagentManager> {
                Arc::new(SubagentManagerImpl::from_deps(deps))
            }

            #[test]
            fn test_params_default() {
                let params: ListSubagentsParams = serde_json::from_str("{}").unwrap();
                assert!(params.include_running);
                assert!(params.parent_run_id.is_none());
            }

            #[test]
            fn test_params_no_running() {
                let params: ListSubagentsParams =
                    serde_json::from_str(r#"{"include_running": false}"#).unwrap();
                assert!(!params.include_running);
            }

            #[tokio::test]
            async fn test_list_with_definitions() {
                let deps = make_deps(
                    MockDefLookup::with_agents(vec![
                        ("researcher", "Researcher"),
                        ("coder", "Coder"),
                        ("reviewer", "Reviewer"),
                    ]),
                    vec![],
                );
                let tool = ListSubagentsTool::new(as_manager(&deps));
                let result = tool.execute(json!({})).await.unwrap();
                assert!(result.success);
                let agents = result.result["available_agents"].as_array().unwrap();
                assert_eq!(agents.len(), 3);
            }

            #[tokio::test]
            async fn test_list_no_running() {
                let deps = make_deps(MockDefLookup::with_agents(vec![("coder", "Coder")]), vec![]);
                let tool = ListSubagentsTool::new(as_manager(&deps));
                let result = tool
                    .execute(json!({"include_running": false}))
                    .await
                    .unwrap();
                assert!(result.success);
                assert!(result.result.get("running_agents").is_none());
            }

            #[tokio::test]
            async fn test_list_with_running_agent() {
                // Use a delayed response so the agent is still running when we list
                let deps = make_deps(
                    MockDefLookup::with_agents(vec![("coder", "Coder")]),
                    vec![MockStep::text("slow").with_delay(5000)],
                );
                let manager = as_manager(&deps);

                // Spawn an agent that will be slow.
                let _handle = manager
                    .spawn(ContractRunSpawnRequest {
                        agent_id: Some("coder".to_string()),
                        task: "write code".to_string(),
                        timeout_secs: Some(30),
                        parent_run_id: Some("parent-1".to_string()),
                        ..ContractRunSpawnRequest::default()
                    })
                    .expect("spawn should succeed");

                // Small delay to let the agent register
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;

                let tool = ListSubagentsTool::new(manager);
                let result = tool
                    .execute(json!({"parent_run_id": "parent-1"}))
                    .await
                    .unwrap();
                assert!(result.success);
                assert_eq!(result.result["running_count"].as_u64().unwrap(), 1);
            }

            #[tokio::test]
            async fn test_list_hides_running_agents_without_parent_scope() {
                let deps = make_deps(
                    MockDefLookup::with_agents(vec![("coder", "Coder")]),
                    vec![MockStep::text("slow").with_delay(5000)],
                );
                let manager = as_manager(&deps);

                let _handle = manager
                    .spawn(ContractRunSpawnRequest {
                        agent_id: Some("coder".to_string()),
                        task: "write code".to_string(),
                        timeout_secs: Some(30),
                        parent_run_id: Some("parent-1".to_string()),
                        ..ContractRunSpawnRequest::default()
                    })
                    .expect("spawn should succeed");

                tokio::time::sleep(std::time::Duration::from_millis(50)).await;

                let tool = ListSubagentsTool::new(manager);
                let result = tool.execute(json!({})).await.unwrap();
                assert!(result.success);
                assert_eq!(result.result["running_count"], json!(0));
                assert_eq!(result.result["running_agents"], json!([]));
            }

            #[tokio::test]
            async fn test_list_scopes_running_agents_to_parent() {
                let deps = make_deps(
                    MockDefLookup::with_agents(vec![("coder", "Coder")]),
                    vec![MockStep::text("slow").with_delay(5000)],
                );
                let manager = as_manager(&deps);

                let _handle = manager
                    .spawn(ContractRunSpawnRequest {
                        agent_id: Some("coder".to_string()),
                        task: "write code".to_string(),
                        timeout_secs: Some(30),
                        parent_run_id: Some("parent-1".to_string()),
                        ..ContractRunSpawnRequest::default()
                    })
                    .expect("spawn should succeed");

                tokio::time::sleep(std::time::Duration::from_millis(50)).await;

                let tool = ListSubagentsTool::new(manager);
                let result = tool
                    .execute(json!({"parent_run_id": "parent-1"}))
                    .await
                    .unwrap();
                assert!(result.success);
                assert_eq!(result.result["running_count"], json!(1));
            }

            #[tokio::test]
            async fn test_list_empty_definitions() {
                let deps = make_deps(MockDefLookup::empty(), vec![]);
                let tool = ListSubagentsTool::new(as_manager(&deps));
                let result = tool.execute(json!({})).await.unwrap();
                assert!(result.success);
                let agents = result.result["available_agents"].as_array().unwrap();
                assert_eq!(agents.len(), 0);
            }
        }
    }

    pub mod load_skill {
        // load_skill tool - Query and load skills dynamically.

        use async_trait::async_trait;
        use serde::{Deserialize, Serialize};
        use serde_json::{Value, json};
        use std::sync::Arc;

        use crate::{Result, ToolError};
        use crate::{SecurityGate, ToolAction};
        use crate::{Tool, ToolOutput, check_security};
        use types::skill::SkillProvider;

        /// Parameters for load_skill tool.
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct LoadSkillParams {
            /// Explicit action for load-only operations.
            pub action: Option<String>,

            /// Skill ID to load.
            pub id: Option<String>,
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum LoadSkillAction {
            List,
            Read,
        }

        /// load_skill tool — lets the LLM query available skills and load their content.
        pub struct LoadSkillTool {
            provider: Arc<dyn SkillProvider>,
            security_gate: Option<Arc<dyn SecurityGate>>,
            agent_id: Option<String>,
            task_id: Option<String>,
        }

        impl LoadSkillTool {
            pub fn new(provider: Arc<dyn SkillProvider>) -> Self {
                Self {
                    provider,
                    security_gate: None,
                    agent_id: None,
                    task_id: None,
                }
            }

            pub fn with_security(
                mut self,
                security_gate: Arc<dyn SecurityGate>,
                agent_id: impl Into<String>,
                task_id: impl Into<String>,
            ) -> Self {
                self.security_gate = Some(security_gate);
                self.agent_id = Some(agent_id.into());
                self.task_id = Some(task_id.into());
                self
            }

            async fn ensure_allowed(&self, action: ToolAction) -> Result<Option<String>> {
                check_security(
                    self.security_gate.as_deref(),
                    action,
                    self.agent_id.as_deref(),
                    self.task_id.as_deref(),
                )
                .await
            }
        }

        #[async_trait]
        impl Tool for LoadSkillTool {
            fn name(&self) -> &str {
                "load_skill"
            }

            fn description(&self) -> &str {
                "Load-only skill access tool. Supports listing skills and reading skill content. Skill execution is not supported in this tool."
            }

            fn parameters_schema(&self) -> Value {
                json!({
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["list", "read"],
                            "description": "Load-only action. Use 'list' to list skills, or 'read' to load skill content."
                        },
                        "id": {
                            "type": "string",
                            "description": "Skill ID for read action."
                        }
                    },
                    "additionalProperties": false
                })
            }

            async fn execute(&self, input: Value) -> Result<ToolOutput> {
                let params: LoadSkillParams = serde_json::from_value(input)
                    .map_err(|e| ToolError::Tool(format!("Invalid parameters: {}", e)))?;

                let action = match params.action.as_deref().map(str::trim) {
                    Some(raw) if raw.eq_ignore_ascii_case("list") => Ok(LoadSkillAction::List),
                    Some(raw) if raw.eq_ignore_ascii_case("read") => Ok(LoadSkillAction::Read),
                    Some(raw)
                        if raw.eq_ignore_ascii_case("execute")
                            || raw.eq_ignore_ascii_case("run") =>
                    {
                        Err(ToolOutput::error(
                            "skill execution not supported in this tool. load_skill is load-only; use action=list/read.",
                        ))
                    }
                    Some(raw) => Err(ToolOutput::error(format!(
                        "Unsupported action '{}'. load_skill supports only load-only actions: list, read.",
                        raw
                    ))),
                    None => Err(ToolOutput::error(
                        "Missing action. load_skill is load-only and requires action=list/read.",
                    )),
                };

                let action = match action {
                    Ok(action) => action,
                    Err(output) => return Ok(output),
                };

                if action == LoadSkillAction::List {
                    if let Some(message) = self
                        .ensure_allowed(ToolAction {
                            tool_name: "load_skill".to_string(),
                            operation: "list".to_string(),
                            target: "*".to_string(),
                            summary: "List available skills".to_string(),
                        })
                        .await?
                    {
                        return Ok(ToolOutput::error(message));
                    }

                    let skills: Vec<Value> = self
                        .provider
                        .list_skills()
                        .into_iter()
                        .map(|info| {
                            json!({
                                "id": info.id,
                                "name": info.name,
                                "description": info.description,
                                "tags": info.tags,
                                "kind": info.kind,
                                "executable": info.executable,
                                "suggested_tools": info.suggested_tools,
                                "source": info.source,
                                "read_only": info.read_only,
                                "source_ref": info.source_ref,
                            })
                        })
                        .collect();

                    return Ok(ToolOutput::success(json!({
                        "available_skills": skills,
                        "count": skills.len(),
                    })));
                }

                let skill_id = params
                    .id
                    .ok_or_else(|| ToolError::Tool("Missing 'id' parameter".to_string()))?;

                if let Some(message) = self
                    .ensure_allowed(ToolAction {
                        tool_name: "load_skill".to_string(),
                        operation: "read".to_string(),
                        target: skill_id.clone(),
                        summary: format!("Read skill '{}'", skill_id),
                    })
                    .await?
                {
                    return Ok(ToolOutput::error(message));
                }

                match self.provider.get_skill(&skill_id) {
                    Some(content) => Ok(ToolOutput::success(json!({
                        "loaded": true,
                        "skill_id": content.id,
                        "name": content.name,
                        "content": content.content,
                        "kind": content.kind,
                        "executable": content.executable,
                        "suggested_tools": content.suggested_tools,
                        "source": content.source,
                        "read_only": content.read_only,
                        "source_ref": content.source_ref,
                    }))),
                    None => Ok(ToolOutput::error(format!("Skill '{}' not found", skill_id))),
                }
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use crate::{SecurityDecision, SecurityGate, ToolAction};
            use async_trait::async_trait;
            use std::sync::{Arc, Mutex};
            use types::skill::{SkillContent, SkillInfo, SkillSource};

            struct MockProvider;

            impl SkillProvider for MockProvider {
                fn list_skills(&self) -> Vec<SkillInfo> {
                    vec![SkillInfo {
                        id: "test-skill".to_string(),
                        name: "Test Skill".to_string(),
                        description: Some("A test skill".to_string()),
                        tags: None,
                        kind: None,
                        executable: false,
                        suggested_tools: Vec::new(),
                        source: SkillSource::User,
                        read_only: false,
                        source_ref: None,
                    }]
                }

                fn get_skill(&self, id: &str) -> Option<SkillContent> {
                    if id == "test-skill" {
                        Some(SkillContent {
                            id: "test-skill".to_string(),
                            name: "Test Skill".to_string(),
                            content: "# Test Skill\nDo something useful.".to_string(),
                            kind: None,
                            executable: false,
                            suggested_tools: Vec::new(),
                            source: SkillSource::User,
                            read_only: false,
                            source_ref: None,
                        })
                    } else {
                        None
                    }
                }

                fn export_skill(&self, _: &str) -> std::result::Result<String, String> {
                    Err("not implemented".to_string())
                }
            }

            #[test]
            fn test_params_list() {
                let params: LoadSkillParams =
                    serde_json::from_str(r#"{"action": "list"}"#).unwrap();
                assert_eq!(params.action.as_deref(), Some("list"));
                assert!(params.id.is_none());
            }

            #[test]
            fn test_params_read() {
                let params: LoadSkillParams =
                    serde_json::from_str(r#"{"action": "read", "id": "api-testing"}"#).unwrap();
                assert_eq!(params.action.as_deref(), Some("read"));
                assert_eq!(params.id.as_deref(), Some("api-testing"));
            }

            #[tokio::test]
            async fn test_list_skills() {
                let tool = LoadSkillTool::new(Arc::new(MockProvider));
                let result = tool.execute(json!({"action": "list"})).await.unwrap();
                assert!(result.success);
                assert_eq!(result.result["count"], 1);
                assert_eq!(result.result["available_skills"][0]["id"], "test-skill");
            }

            #[tokio::test]
            async fn test_load_skill() {
                let tool = LoadSkillTool::new(Arc::new(MockProvider));
                let result = tool
                    .execute(json!({"action": "read", "id": "test-skill"}))
                    .await
                    .unwrap();
                assert!(result.success);
                assert_eq!(result.result["loaded"], true);
                assert_eq!(result.result["skill_id"], "test-skill");
                assert!(
                    result.result["content"]
                        .as_str()
                        .unwrap()
                        .contains("Do something useful")
                );
            }

            #[tokio::test]
            async fn test_load_skill_not_found() {
                let tool = LoadSkillTool::new(Arc::new(MockProvider));
                let result = tool
                    .execute(json!({"action": "read", "id": "nonexistent"}))
                    .await
                    .unwrap();
                assert!(!result.success);
            }

            #[tokio::test]
            async fn test_load_skill_missing_id() {
                let tool = LoadSkillTool::new(Arc::new(MockProvider));
                // Missing explicit action and id field.
                let result = tool.execute(json!({})).await;
                assert!(result.is_ok());
                assert!(!result.unwrap().success);
            }

            struct EmptyProvider;

            impl SkillProvider for EmptyProvider {
                fn list_skills(&self) -> Vec<SkillInfo> {
                    vec![]
                }
                fn get_skill(&self, _: &str) -> Option<SkillContent> {
                    None
                }
                fn export_skill(&self, _: &str) -> std::result::Result<String, String> {
                    Err("not implemented".to_string())
                }
            }

            #[tokio::test]
            async fn test_list_skills_empty() {
                let tool = LoadSkillTool::new(Arc::new(EmptyProvider));
                let result = tool.execute(json!({"action": "list"})).await.unwrap();
                assert!(result.success);
                assert_eq!(result.result["count"], 0);
                assert_eq!(
                    result.result["available_skills"].as_array().unwrap().len(),
                    0
                );
            }

            struct RecordingGate {
                calls: Arc<Mutex<Vec<ToolAction>>>,
            }

            impl RecordingGate {
                fn new() -> Self {
                    Self {
                        calls: Arc::new(Mutex::new(Vec::new())),
                    }
                }

                fn calls(&self) -> Arc<Mutex<Vec<ToolAction>>> {
                    self.calls.clone()
                }
            }

            #[async_trait]
            impl SecurityGate for RecordingGate {
                async fn check_command(
                    &self,
                    _: &str,
                    _: &str,
                    _: &str,
                    _: Option<&str>,
                ) -> crate::Result<SecurityDecision> {
                    Ok(SecurityDecision::allowed(None))
                }

                async fn check_tool_action(
                    &self,
                    action: &ToolAction,
                    _: Option<&str>,
                    _: Option<&str>,
                ) -> crate::Result<SecurityDecision> {
                    self.calls.lock().unwrap().push(action.clone());
                    Ok(SecurityDecision::blocked(Some("blocked".into())))
                }
            }

            #[tokio::test]
            async fn test_security_gate_blocks_load_skill() {
                let gate = Arc::new(RecordingGate::new());
                let calls = gate.calls();
                let tool = LoadSkillTool::new(Arc::new(MockProvider))
                    .with_security(gate, "agent-1", "task-1");
                let result = tool.execute(json!({"action": "list"})).await.unwrap();
                assert!(
                    !result.success,
                    "security gate should block execution and return error"
                );
                let recorded = calls.lock().unwrap();
                assert_eq!(recorded.len(), 1);
                assert_eq!(recorded[0].operation, "list");
            }

            #[tokio::test]
            async fn test_reject_execute_action() {
                let tool = LoadSkillTool::new(Arc::new(MockProvider));
                let result = tool
                    .execute(json!({"action": "execute", "id": "test-skill"}))
                    .await
                    .unwrap();
                assert!(!result.success);
                assert!(
                    result
                        .error
                        .unwrap_or_default()
                        .contains("skill execution not supported in this tool")
                );
            }

            #[tokio::test]
            async fn test_reject_run_action() {
                let tool = LoadSkillTool::new(Arc::new(MockProvider));
                let result = tool
                    .execute(json!({"action": "run", "id": "test-skill"}))
                    .await
                    .unwrap();
                assert!(!result.success);
                assert!(
                    result
                        .error
                        .unwrap_or_default()
                        .contains("skill execution not supported in this tool")
                );
            }
        }
    }

    pub mod registry_builder {
        // Tool registry builder with configuration types.
        //
        // Provides BashConfig, FileConfig, and ToolRegistryBuilder for constructing
        // a ToolRegistry with commonly used tools.

        use std::path::PathBuf;
        use std::sync::Arc;

        use crate::impls::agent_crud::AgentCrudTool;
        use crate::impls::batch::BatchTool;
        use crate::impls::config::ConfigTool;
        use crate::impls::edit::EditTool;
        use crate::impls::file_tracker::FileTracker;
        use crate::impls::glob_tool::GlobTool;
        use crate::impls::grep_tool::GrepTool;
        use crate::impls::list_subagents::ListSubagentsTool;
        use crate::impls::load_skill::LoadSkillTool;
        use crate::impls::manage_ops::ManageOpsTool;
        use crate::impls::multiedit::MultiEditTool;
        use crate::impls::patch::PatchTool;
        use crate::impls::secrets::{SecretGetPolicy, SecretsTool};
        use crate::impls::session::SessionTool;
        use crate::impls::skill::SkillTool;
        use crate::impls::spawn_subagent::SpawnSubagentTool;
        use crate::impls::wait_subagents::WaitSubagentsTool;
        use crate::impls::{BashTool, FileTool};
        use crate::{BashSecurityConfig, SecurityGate, ToolRegistry};
        use types::SubagentManager;
        use types::skill::SkillProvider;
        use types::store::{AgentStore, ConfigStore, OpsProvider, SecretStore, SessionStore};

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

        /// Builder for creating a fully configured ToolRegistry.
        pub struct ToolRegistryBuilder {
            pub registry: ToolRegistry,
            tracker: Arc<FileTracker>,
        }

        impl Default for ToolRegistryBuilder {
            fn default() -> Self {
                Self::new()
            }
        }

        impl ToolRegistryBuilder {
            pub fn new() -> Self {
                Self {
                    registry: ToolRegistry::new(),
                    tracker: Arc::new(FileTracker::new()),
                }
            }

            /// Get shared file tracker for external use.
            pub fn tracker(&self) -> Arc<FileTracker> {
                self.tracker.clone()
            }

            pub fn build(self) -> ToolRegistry {
                self.registry
            }

            /// Build the registry and automatically register the `batch` tool.
            ///
            /// This is a convenience for the two-phase setup required by `BatchTool`,
            /// which needs an `Arc<ToolRegistry>` containing the base tools it can call.
            pub fn build_with_batch(self) -> ToolRegistry {
                let mut registry = self.build();
                if registry.has("batch") {
                    return registry;
                }

                let registry_arc = Arc::new(std::mem::take(&mut registry));
                for name in registry_arc.list() {
                    if let Some(tool) = registry_arc.get(name) {
                        registry.register_arc(tool);
                    }
                }
                registry.register(BatchTool::new(registry_arc));
                registry
            }
        }

        /// Create a registry with default core tools.
        pub fn default_registry() -> anyhow::Result<ToolRegistry> {
            Ok(ToolRegistryBuilder::new()
                .with_bash(BashConfig::default())
                .with_file(FileConfig::default())
                .build())
        }

        impl ToolRegistryBuilder {
            pub fn with_bash(mut self, config: BashConfig) -> Self {
                self.registry.register(config.into_bash_tool());
                self
            }

            pub fn with_file(mut self, config: FileConfig) -> Self {
                self.registry
                    .register(config.into_file_tool_with_tracker(self.tracker.clone()));
                self
            }

            pub fn with_patch(mut self) -> Self {
                self.registry.register(PatchTool::new(self.tracker.clone()));
                self
            }

            pub fn with_patch_and_base_dir(mut self, base_dir: Option<PathBuf>) -> Self {
                let mut tool = PatchTool::new(self.tracker.clone()).require_base_dir();
                if let Some(base_dir) = base_dir {
                    tool = tool.with_base_dir(base_dir);
                }
                self.registry.register(tool);
                self
            }

            pub fn with_edit(mut self) -> Self {
                let tool = EditTool::with_tracker(self.tracker.clone());
                self.registry.register(tool);
                self
            }

            pub fn with_edit_and_base_dir(mut self, base_dir: Option<PathBuf>) -> Self {
                let mut tool = EditTool::with_tracker(self.tracker.clone()).require_base_dir();
                if let Some(base_dir) = base_dir {
                    tool = tool.with_base_dir(base_dir);
                }
                self.registry.register(tool);
                self
            }

            pub fn with_multiedit(mut self) -> Self {
                let tool = MultiEditTool::with_tracker(self.tracker.clone());
                self.registry.register(tool);
                self
            }

            pub fn with_multiedit_and_base_dir(mut self, base_dir: Option<PathBuf>) -> Self {
                let mut tool = MultiEditTool::with_tracker(self.tracker.clone()).require_base_dir();
                if let Some(base_dir) = base_dir {
                    tool = tool.with_base_dir(base_dir);
                }
                self.registry.register(tool);
                self
            }

            pub fn with_glob(mut self) -> Self {
                self.registry.register(GlobTool::new());
                self
            }

            pub fn with_glob_and_base_dir(mut self, base_dir: Option<PathBuf>) -> Self {
                let mut tool = GlobTool::new().require_base_dir();
                if let Some(base_dir) = base_dir {
                    tool = tool.with_base_dir(base_dir);
                }
                self.registry.register(tool);
                self
            }

            pub fn with_grep(mut self) -> Self {
                self.registry.register(GrepTool::new());
                self
            }

            pub fn with_grep_and_base_dir(mut self, base_dir: Option<PathBuf>) -> Self {
                let mut tool = GrepTool::new().require_base_dir();
                if let Some(base_dir) = base_dir {
                    tool = tool.with_base_dir(base_dir);
                }
                self.registry.register(tool);
                self
            }

            /// Register the batch tool. This requires an `Arc<ToolRegistry>` containing
            /// the tools the batch tool can invoke.
            pub fn with_batch(mut self, tools: Arc<ToolRegistry>) -> Self {
                self.registry.register(BatchTool::new(tools));
                self
            }

            pub fn with_spawn_subagent(mut self, manager: Arc<dyn SubagentManager>) -> Self {
                self.registry.register(SpawnSubagentTool::new(manager));
                self
            }

            pub fn with_wait_subagents(mut self, manager: Arc<dyn SubagentManager>) -> Self {
                self.registry.register(WaitSubagentsTool::new(manager));
                self
            }

            pub fn with_list_subagents(mut self, manager: Arc<dyn SubagentManager>) -> Self {
                self.registry.register(ListSubagentsTool::new(manager));
                self
            }

            pub fn with_load_skill(mut self, provider: Arc<dyn SkillProvider>) -> Self {
                self.registry.register(LoadSkillTool::new(provider));
                self
            }

            pub fn with_load_skill_with_security(
                mut self,
                provider: Arc<dyn SkillProvider>,
                security_gate: Arc<dyn SecurityGate>,
                agent_id: impl Into<String>,
                task_id: impl Into<String>,
            ) -> Self {
                self.registry
                    .register(LoadSkillTool::new(provider).with_security(
                        security_gate,
                        agent_id,
                        task_id,
                    ));
                self
            }

            pub fn with_skill_tool(mut self, provider: Arc<dyn SkillProvider>) -> Self {
                self.registry.register(SkillTool::new(provider));
                self
            }

            pub fn with_skill_tool_with_security(
                mut self,
                provider: Arc<dyn SkillProvider>,
                security_gate: Arc<dyn SecurityGate>,
                agent_id: impl Into<String>,
                task_id: impl Into<String>,
            ) -> Self {
                self.registry
                    .register(SkillTool::new(provider).with_security(
                        security_gate,
                        agent_id,
                        task_id,
                    ));
                self
            }

            pub fn with_session(mut self, store: Arc<dyn SessionStore>) -> Self {
                self.registry
                    .register(SessionTool::new(store).with_write(true));
                self
            }

            pub fn with_ops(mut self, provider: Arc<dyn OpsProvider>) -> Self {
                self.registry.register(ManageOpsTool::new(provider));
                self
            }

            pub fn with_secrets(mut self, store: Arc<dyn SecretStore>) -> Self {
                self = self.with_secrets_config(store, SecretsConfig::default());
                self
            }

            pub fn with_secrets_config(
                mut self,
                store: Arc<dyn SecretStore>,
                config: SecretsConfig,
            ) -> Self {
                self.registry.register(
                    SecretsTool::new(store)
                        .with_write(config.allow_write)
                        .with_get_policy(config.get_policy),
                );
                self
            }

            pub fn with_config(mut self, store: Arc<dyn ConfigStore>) -> Self {
                self.registry.register(ConfigTool::new(store));
                self
            }

            pub fn with_agent_crud(mut self, store: Arc<dyn AgentStore>) -> Self {
                self.registry
                    .register(AgentCrudTool::new(store).with_write(true));
                self
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            #[test]
            fn test_build_with_batch_registers_batch_and_preserves_tools() {
                let registry = ToolRegistryBuilder::new()
                    .with_bash(BashConfig::default())
                    .build_with_batch();
                assert!(registry.has("batch"));
                assert!(registry.has("bash"));
            }
        }
    }

    pub mod spawn_subagent {
        // spawn_subagent tool - Spawn a sub-agent to work on a task in parallel.

        use async_trait::async_trait;
        use serde::{Deserialize, Serialize};
        use serde_json::{Value, json};
        use std::sync::Arc;
        use tokio::time::{Duration, timeout};

        use crate::impls::spawn_subagent_batch::{
            BatchSubagentSpec, SpawnSubagentBatchOperation, SpawnSubagentBatchTool,
        };
        use crate::{Result, Tool, ToolError, ToolOutput};
        use ::types::{SubagentManager, subagent::SubagentDefSummary};
        use types::request::{
            InlineAgentRunConfig as ContractInlineAgentRunConfig,
            RunSpawnRequest as ContractRunSpawnRequest,
        };
        use types::{DEFAULT_SUBAGENT_TIMEOUT_SECS, SubagentCompletion, SubagentStatus};

        /// Parameters for spawn_subagent tool.
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct SpawnSubagentParams {
            /// Operation to perform. Defaults to `spawn`.
            #[serde(default)]
            pub operation: SpawnSubagentBatchOperation,

            /// Agent type to spawn. When omitted, runtime creates a temporary sub-agent
            /// from inline config.
            #[serde(default)]
            pub agent: Option<String>,

            /// Task description for single spawn, or transient fallback task for batch spawn.
            #[serde(default)]
            pub task: Option<String>,

            /// Transient per-instance task list for batch spawn.
            #[serde(default)]
            pub tasks: Option<Vec<String>>,

            /// If true, wait for completion. If false (default), run concurrently.
            #[serde(default)]
            pub wait: bool,

            /// Timeout in seconds. If omitted, uses sub-agent manager default timeout.
            #[serde(default)]
            pub timeout_secs: Option<u64>,

            /// Optional model override for this spawn.
            #[serde(default)]
            pub model: Option<String>,

            /// Optional provider selector paired with model.
            #[serde(default)]
            pub provider: Option<String>,

            /// Optional parent run ID (runtime-injected, internal use).
            #[serde(default)]
            pub parent_run_id: Option<String>,

            /// Optional name for temporary sub-agent creation.
            #[serde(default)]
            pub inline_name: Option<String>,

            /// Optional system prompt for temporary sub-agent creation.
            #[serde(default)]
            pub inline_system_prompt: Option<String>,

            /// Optional allowlist for temporary sub-agent tools.
            #[serde(default)]
            pub inline_allowed_tools: Option<Vec<String>>,

            /// Optional max iterations override for temporary sub-agent creation.
            #[serde(default)]
            pub inline_max_iterations: Option<u32>,

            /// Optional list-based worker specs for unified single/multi spawn.
            #[serde(default)]
            pub workers: Option<Vec<BatchSubagentSpec>>,
        }

        /// spawn_subagent tool for the shared agent execution engine.
        pub struct SpawnSubagentTool {
            manager: Arc<dyn SubagentManager>,
        }

        impl SpawnSubagentTool {
            pub fn new(manager: Arc<dyn SubagentManager>) -> Self {
                Self { manager }
            }

            fn available_agents(&self) -> Vec<SubagentDefSummary> {
                self.manager.list_callable()
            }
        }

        #[async_trait]
        impl Tool for SpawnSubagentTool {
            fn name(&self) -> &str {
                "spawn_subagent"
            }

            fn description(&self) -> &str {
                "Spawn a specialized sub-agent to work on a task in parallel. Use wait_subagents to check completion."
            }

            fn parameters_schema(&self) -> Value {
                parameters_schema(&self.available_agents())
            }

            async fn execute(&self, input: Value) -> Result<ToolOutput> {
                let params: SpawnSubagentParams = serde_json::from_value(input)
                    .map_err(|e| ToolError::Tool(format!("Invalid parameters: {}", e)))?;
                execute_spawn(self, params).await
            }
        }

        fn parameters_schema(available: &[SubagentDefSummary]) -> Value {
            let agent_property = if available.is_empty() {
                json!({
                    "type": "string",
                    "description": "Optional agent ID or name. Omit to create a temporary sub-agent. Call list_subagents to discover available agents."
                })
            } else {
                let enum_values: Vec<String> =
                    available.iter().map(|agent| agent.id.clone()).collect();
                let enum_labels: Vec<String> = available
                    .iter()
                    .map(|agent| format!("{} ({})", agent.name, agent.id))
                    .collect();
                json!({
                    "type": "string",
                    "enum": enum_values,
                    "x-enumNames": enum_labels,
                    "description": "Optional agent ID. You can also pass agent name at runtime. Omit to create a temporary sub-agent."
                })
            };

            json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["spawn"],
                        "default": "spawn",
                        "description": "Operation to perform."
                    },
                    "agent": agent_property,
                    "task": {
                        "type": "string",
                        "description": "Detailed task description for single spawn, or transient fallback task for batch worker specs. Required for single spawn."
                    },
                    "tasks": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Transient per-instance task list for batch spawn. Tasks are assigned in worker order."
                    },
                    "wait": {
                        "type": "boolean",
                        "default": false,
                        "description": "If true, wait for completion. Applies to spawn only."
                    },
                    "timeout_secs": {
                        "type": "integer",
                        "default": DEFAULT_SUBAGENT_TIMEOUT_SECS,
                        "description": format!(
                            "Timeout in seconds for single spawn or batch spawn (default: {})",
                            DEFAULT_SUBAGENT_TIMEOUT_SECS
                        )
                    },
                    "model": {
                        "type": "string",
                        "description": "Optional model override for this sub-agent (e.g., 'minimax/coding-plan')"
                    },
                    "provider": {
                        "type": "string",
                        "description": "Provider selector paired with model override (e.g., 'openai-codex'). Required when model is set."
                    },
                    "parent_run_id": {
                        "type": "string",
                        "description": "Optional parent run ID for context propagation (runtime-injected)"
                    },
                    "inline_name": {
                        "type": "string",
                        "description": "Optional temporary sub-agent name when 'agent' is omitted."
                    },
                    "inline_system_prompt": {
                        "type": "string",
                        "description": "Optional system prompt for temporary sub-agent when 'agent' is omitted."
                    },
                    "inline_allowed_tools": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional tool allowlist for temporary sub-agent when 'agent' is omitted."
                    },
                    "inline_max_iterations": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Optional max iterations for temporary sub-agent when 'agent' is omitted."
                    },
                    "workers": {
                        "type": "array",
                        "description": "Optional unified list-based batch specs. Use for batch spawn.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "agent": { "type": "string", "description": "Optional agent ID or name." },
                                "count": { "type": "integer", "minimum": 1, "default": 1, "description": "Number of instances for this worker spec." },
                                "task": { "type": "string", "description": "Optional transient per-worker task override." },
                                "tasks": { "type": "array", "items": { "type": "string" }, "description": "Optional transient per-instance task list for distinct prompts." },
                                "timeout_secs": { "type": "integer", "minimum": 0, "description": "Optional per-worker timeout." },
                                "model": { "type": "string", "description": "Optional model override for this worker." },
                                "provider": { "type": "string", "description": "Optional provider paired with model." },
                                "inline_name": { "type": "string", "description": "Optional temporary sub-agent name." },
                                "inline_system_prompt": { "type": "string", "description": "Optional temporary sub-agent system prompt." },
                                "inline_allowed_tools": { "type": "array", "items": { "type": "string" }, "description": "Optional temporary sub-agent tool allowlist." },
                                "inline_max_iterations": { "type": "integer", "minimum": 1, "description": "Optional temporary sub-agent max iterations." }
                            }
                        }
                    }
                }
            })
        }

        fn completion_output(
            task_id: &str,
            agent_name: &str,
            completion: SubagentCompletion,
            effective_limits: &types::SubagentEffectiveLimits,
        ) -> Value {
            let status = match completion.status {
                SubagentStatus::Completed => "completed",
                SubagentStatus::Failed => "failed",
                SubagentStatus::Interrupted => "interrupted",
                SubagentStatus::TimedOut => "timed_out",
                SubagentStatus::Pending => "pending",
                SubagentStatus::Running => "running",
            };

            let mut output = json!({
                "task_id": task_id,
                "agent": agent_name,
                "status": status,
                "effective_limits": effective_limits,
            });

            if let Some(result) = completion.result {
                output["duration_ms"] = json!(result.duration_ms);
                if result.success {
                    output["output"] = json!(result.output);
                } else {
                    output["error"] =
                        json!(result.error.unwrap_or_else(|| "Unknown error".to_string()));
                    if !result.output.is_empty() {
                        output["output"] = json!(result.output);
                    }
                }
            }

            output
        }

        fn build_inline_config(
            params: &SpawnSubagentParams,
        ) -> Option<ContractInlineAgentRunConfig> {
            let config = ContractInlineAgentRunConfig {
                name: params.inline_name.clone(),
                system_prompt: params.inline_system_prompt.clone(),
                allowed_tools: params.inline_allowed_tools.clone(),
                max_iterations: params.inline_max_iterations,
            };

            if config.name.is_none()
                && config.system_prompt.is_none()
                && config.allowed_tools.is_none()
                && config.max_iterations.is_none()
            {
                None
            } else {
                Some(config)
            }
        }

        fn uses_batch_mode(params: &SpawnSubagentParams) -> bool {
            params.workers.is_some() || params.tasks.is_some()
        }

        fn routes_to_batch_tool(params: &SpawnSubagentParams) -> bool {
            params.operation != SpawnSubagentBatchOperation::Spawn || uses_batch_mode(params)
        }

        fn normalize_optional_text(value: Option<&str>) -> Option<String> {
            value
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        }

        fn build_contract_request(
            params: &SpawnSubagentParams,
            task: String,
        ) -> ContractRunSpawnRequest {
            ContractRunSpawnRequest {
                agent_id: params.agent.clone(),
                inline: build_inline_config(params),
                task,
                timeout_secs: params.timeout_secs,
                max_iterations: None,
                priority: None,
                model: params.model.clone(),
                model_provider: params.provider.clone(),
                parent_run_id: params.parent_run_id.clone(),
            }
        }

        async fn execute_spawn(
            tool: &SpawnSubagentTool,
            params: SpawnSubagentParams,
        ) -> Result<ToolOutput> {
            if routes_to_batch_tool(&params) {
                if params.agent.is_some()
                    || params.model.is_some()
                    || params.provider.is_some()
                    || params.inline_name.is_some()
                    || params.inline_system_prompt.is_some()
                    || params.inline_allowed_tools.is_some()
                    || params.inline_max_iterations.is_some()
                {
                    return Err(ToolError::Tool(
                        "Batch mode uses 'workers'; do not combine with single-spawn fields like 'agent', top-level model/provider, or top-level inline settings.".to_string(),
                    ));
                }

                let batch_tool = SpawnSubagentBatchTool::new(tool.manager.clone());

                let operation = params.operation.clone();
                let task = normalize_optional_text(params.task.as_deref());
                let tasks = params.tasks.clone();

                return batch_tool
                    .execute(json!({
                        "operation": operation,
                        "specs": params.workers,
                        "task": task,
                        "tasks": tasks,
                        "wait": params.wait,
                        "timeout_secs": params.timeout_secs,
                        "parent_run_id": params.parent_run_id,
                    }))
                    .await;
            }

            let request = build_contract_request(
                &params,
                normalize_optional_text(params.task.as_deref()).unwrap_or_default(),
            );

            let handle = tool.manager.spawn(request)?;

            if params.wait {
                let wait_timeout = params
                    .timeout_secs
                    .unwrap_or(tool.manager.config().subagent_timeout_secs);

                let result = if wait_timeout == 0 {
                    match tool.manager.wait(&handle.id).await {
                        Some(result) => result,
                        None => return Ok(ToolOutput::error("Sub-agent not found")),
                    }
                } else {
                    match timeout(
                        Duration::from_secs(wait_timeout),
                        tool.manager.wait(&handle.id),
                    )
                    .await
                    {
                        Ok(Some(result)) => result,
                        Ok(None) => return Ok(ToolOutput::error("Sub-agent not found")),
                        Err(_) => {
                            return Ok(ToolOutput::success(json!({
                                "task_id": handle.id,
                                "agent": handle.agent_name,
                                "status": "timeout",
                                "message": "Timeout waiting for sub-agent",
                                "effective_limits": handle.effective_limits,
                            })));
                        }
                    }
                };

                Ok(ToolOutput::success(completion_output(
                    &handle.id,
                    &handle.agent_name,
                    result,
                    &handle.effective_limits,
                )))
            } else {
                Ok(ToolOutput::success(json!({
                    "task_id": handle.id,
                    "agent": handle.agent_name,
                    "status": "spawned",
                    "effective_limits": handle.effective_limits,
                    "message": format!(
                        "Agent '{}' is now working on the task concurrently. Use wait_subagents to check completion.",
                        handle.agent_name
                    )
                })))
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use crate::Tool;
            use crate::ToolRegistry;
            use ::agent::agent::{
                SubagentConfig, SubagentDefLookup, SubagentDefSnapshot, SubagentDefSummary,
                SubagentManagerImpl, SubagentTracker,
            };
            use ::agent::llm::{MockLlmClient, MockStep};
            use ::types::request::RunSpawnRequest as ContractRunSpawnRequest;
            use ::types::{DEFAULT_SUBAGENT_TIMEOUT_SECS, SubagentManager};
            use ::types::{SpawnHandle, SubagentCompletion, SubagentState};
            use serde_json::json;
            use std::collections::HashMap;
            use std::sync::{Arc, Mutex};
            use tokio::sync::mpsc;

            use crate::impls::spawn_subagent_batch::SpawnSubagentBatchOperation;

            struct MockDefLookup {
                defs: HashMap<String, SubagentDefSnapshot>,
                summaries: Vec<SubagentDefSummary>,
            }

            impl MockDefLookup {
                fn with_agents(agents: Vec<(&str, &str)>) -> Self {
                    let mut defs = HashMap::new();
                    let mut summaries = Vec::new();
                    for (id, name) in agents {
                        defs.insert(
                            id.to_string(),
                            SubagentDefSnapshot {
                                name: name.to_string(),
                                system_prompt: format!("You are a {} agent.", name),
                                allowed_tools: vec![],
                                max_iterations: Some(1),
                                default_model: None,
                            },
                        );
                        summaries.push(SubagentDefSummary {
                            id: id.to_string(),
                            name: name.to_string(),
                            description: format!("{} agent", name),
                            tags: vec![],
                        });
                    }
                    Self { defs, summaries }
                }
            }

            impl SubagentDefLookup for MockDefLookup {
                fn lookup(&self, id: &str) -> Option<SubagentDefSnapshot> {
                    self.defs.get(id).cloned()
                }
                fn list_callable(&self) -> Vec<SubagentDefSummary> {
                    self.summaries.clone()
                }
            }

            struct RecordingSubagentManager {
                inner: Arc<dyn SubagentManager>,
                last_request: Mutex<Option<ContractRunSpawnRequest>>,
            }

            impl RecordingSubagentManager {
                fn new(inner: Arc<dyn SubagentManager>) -> Self {
                    Self {
                        inner,
                        last_request: Mutex::new(None),
                    }
                }

                fn last_request(&self) -> Option<ContractRunSpawnRequest> {
                    self.last_request
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .clone()
                }
            }

            #[async_trait::async_trait]
            impl SubagentManager for RecordingSubagentManager {
                fn spawn(
                    &self,
                    request: ContractRunSpawnRequest,
                ) -> std::result::Result<SpawnHandle, crate::ToolError> {
                    *self
                        .last_request
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(request.clone());
                    self.inner.spawn(request)
                }

                fn list_callable(&self) -> Vec<SubagentDefSummary> {
                    self.inner.list_callable()
                }

                fn list_running(&self) -> Vec<SubagentState> {
                    self.inner.list_running()
                }

                fn running_count(&self) -> usize {
                    self.inner.running_count()
                }

                async fn wait(&self, task_id: &str) -> Option<SubagentCompletion> {
                    self.inner.wait(task_id).await
                }

                async fn wait_for_parent_owned_task(
                    &self,
                    task_id: &str,
                    parent_run_id: &str,
                ) -> Option<SubagentCompletion> {
                    self.inner
                        .wait_for_parent_owned_task(task_id, parent_run_id)
                        .await
                }

                fn config(&self) -> &SubagentConfig {
                    self.inner.config()
                }
            }

            fn make_test_deps(
                agents: Vec<(&str, &str)>,
                mock_steps: Vec<MockStep>,
            ) -> Arc<dyn SubagentManager> {
                let (tx, rx) = mpsc::channel(16);
                let tracker = Arc::new(SubagentTracker::new(tx, rx));
                let definitions: Arc<dyn SubagentDefLookup> =
                    Arc::new(MockDefLookup::with_agents(agents));
                let llm_client = Arc::new(MockLlmClient::from_steps("mock", mock_steps));
                let tool_registry = Arc::new(ToolRegistry::new());
                let config = SubagentConfig {
                    max_parallel_agents: 5,
                    subagent_timeout_secs: 10,
                    max_iterations: 5,
                    max_depth: 1,
                };
                Arc::new(SubagentManagerImpl::new(
                    tracker,
                    definitions,
                    llm_client,
                    tool_registry,
                    config,
                ))
            }

            #[test]
            fn test_params_deserialization() {
                let json = r#"{"agent": "researcher", "task": "Research topic X"}"#;
                let params: SpawnSubagentParams = serde_json::from_str(json).unwrap();
                assert_eq!(params.operation, SpawnSubagentBatchOperation::Spawn);
                assert_eq!(params.agent.as_deref(), Some("researcher"));
                assert_eq!(params.task.as_deref(), Some("Research topic X"));
                assert!(params.tasks.is_none());
                assert!(!params.wait);
            }

            #[test]
            fn test_params_with_wait() {
                let json = r#"{"agent": "coder", "task": "Write function Y", "wait": true, "timeout_secs": 600}"#;
                let params: SpawnSubagentParams = serde_json::from_str(json).unwrap();
                assert_eq!(params.agent.as_deref(), Some("coder"));
                assert_eq!(params.task.as_deref(), Some("Write function Y"));
                assert!(params.wait);
                assert_eq!(params.timeout_secs, Some(600));
            }

            #[test]
            fn test_params_with_model_and_provider() {
                let json = r#"{"agent":"coder","task":"Write function","model":"gpt-5.3-codex","provider":"openai-codex"}"#;
                let params: SpawnSubagentParams = serde_json::from_str(json).unwrap();
                assert_eq!(params.model.as_deref(), Some("gpt-5.3-codex"));
                assert_eq!(params.provider.as_deref(), Some("openai-codex"));
            }

            #[test]
            fn test_params_serialize_canonical_parent_run_id() {
                let params = SpawnSubagentParams {
                    operation: SpawnSubagentBatchOperation::Spawn,
                    agent: None,
                    task: Some("Research topic X".to_string()),
                    tasks: None,
                    wait: false,
                    timeout_secs: None,
                    model: None,
                    provider: None,
                    parent_run_id: Some("run-456".to_string()),
                    inline_name: None,
                    inline_system_prompt: None,
                    inline_allowed_tools: None,
                    inline_max_iterations: None,
                    workers: None,
                };

                let serialized = serde_json::to_value(&params).unwrap();
                assert_eq!(serialized["parent_run_id"], "run-456");
            }

            #[tokio::test]
            async fn test_spawn_subagent_preserves_parent_run_id() {
                let manager = Arc::new(RecordingSubagentManager::new(make_test_deps(
                    vec![("researcher", "Researcher")],
                    vec![MockStep::text("research done")],
                )));
                let tool = SpawnSubagentTool::new(manager.clone());

                let result = tool
                    .execute(json!({
                        "agent": "researcher",
                        "task": "Find info",
                        "parent_run_id": "run-789"
                    }))
                    .await
                    .unwrap();

                assert!(result.success);
                assert_eq!(
                    manager
                        .last_request()
                        .expect("request should be recorded")
                        .parent_run_id
                        .as_deref(),
                    Some("run-789")
                );
            }

            #[tokio::test]
            async fn test_spawn_subagent_background() {
                let deps = make_test_deps(
                    vec![("researcher", "Researcher")],
                    vec![MockStep::text("research done")],
                );
                let tool = SpawnSubagentTool::new(deps);
                let result = tool
                    .execute(json!({"agent": "researcher", "task": "Find info", "wait": false}))
                    .await
                    .unwrap();
                assert!(result.success);
                assert_eq!(result.result["status"], "spawned");
                assert!(result.result["task_id"].as_str().is_some());
            }

            #[tokio::test]
            async fn test_spawn_subagent_wait_success() {
                let deps = make_test_deps(
                    vec![("coder", "Coder")],
                    vec![MockStep::text("function written")],
                );
                let tool = SpawnSubagentTool::new(deps);
                let result = tool
                    .execute(json!({"agent": "coder", "task": "Write code", "wait": true, "timeout_secs": 10}))
                    .await
                    .unwrap();
                assert!(result.success);
                assert_eq!(result.result["status"], "completed");
                assert!(
                    result.result["output"]
                        .as_str()
                        .unwrap()
                        .contains("function written")
                );
            }

            #[tokio::test]
            async fn test_spawn_subagent_wait_failure() {
                let deps =
                    make_test_deps(vec![("coder", "Coder")], vec![MockStep::error("LLM error")]);
                let tool = SpawnSubagentTool::new(deps);
                let result = tool
                    .execute(json!({"agent": "coder", "task": "Write code", "wait": true, "timeout_secs": 10}))
                    .await
                    .unwrap();
                assert!(result.success); // ToolOutput is success, but status indicates failure
                assert_eq!(result.result["status"], "failed");
                assert!(result.result["error"].as_str().is_some());
            }

            #[tokio::test]
            async fn test_spawn_subagent_wait_timeout_returns_task_id() {
                let deps = make_test_deps(
                    vec![("coder", "Coder")],
                    vec![MockStep::text("slow").with_delay(2_000)],
                );
                let tool = SpawnSubagentTool::new(deps);
                let result = tool
                    .execute(json!({"agent": "coder", "task": "Write code", "wait": true, "timeout_secs": 1}))
                    .await
                    .unwrap();

                assert!(result.success);
                assert_eq!(result.result["status"], "timeout");
                assert!(result.result["task_id"].as_str().is_some());
            }

            #[tokio::test]
            async fn test_spawn_subagent_unknown_agent() {
                let deps = make_test_deps(vec![], vec![]);
                let tool = SpawnSubagentTool::new(deps);
                let result = tool
                    .execute(json!({"agent": "nonexistent", "task": "Do something"}))
                    .await;
                assert!(result.is_err());
                let err_msg = result.unwrap_err().to_string();
                assert!(err_msg.contains("No callable sub-agents available"));
            }

            #[tokio::test]
            async fn test_spawn_subagent_invalid_params() {
                let deps = make_test_deps(vec![], vec![]);
                let tool = SpawnSubagentTool::new(deps);
                let result = tool.execute(json!({"wait": true})).await;
                assert!(result.is_err());
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("Single spawn requires non-empty 'task'")
                );
            }

            #[tokio::test]
            async fn test_spawn_subagent_rejects_model_without_provider() {
                let deps = make_test_deps(vec![("coder", "Coder")], vec![MockStep::text("done")]);
                let tool = SpawnSubagentTool::new(deps);
                let result = tool
                    .execute(
                        json!({"agent": "coder", "task": "Write code", "model": "gpt-5.3-codex"}),
                    )
                    .await;
                assert!(result.is_err());
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("requires both 'model' and 'provider'")
                );
            }

            #[tokio::test]
            async fn test_spawn_subagent_rejects_provider_without_model() {
                let deps = make_test_deps(vec![("coder", "Coder")], vec![MockStep::text("done")]);
                let tool = SpawnSubagentTool::new(deps);
                let result = tool
                    .execute(
                        json!({"agent": "coder", "task": "Write code", "provider": "openai-codex"}),
                    )
                    .await;
                assert!(result.is_err());
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("requires both 'model' and 'provider'")
                );
            }

            #[tokio::test]
            async fn test_spawn_subagent_resolves_by_name() {
                let deps = make_test_deps(
                    vec![("agent-123", "Code Planner")],
                    vec![MockStep::text("planned")],
                );
                let tool = SpawnSubagentTool::new(deps);
                let result = tool
                    .execute(json!({"agent": "code planner", "task": "plan task", "wait": true}))
                    .await
                    .unwrap();
                assert!(result.success);
                assert_eq!(result.result["status"], "completed");
            }

            #[tokio::test]
            async fn test_spawn_subagent_without_agent_uses_temporary_mode() {
                let deps = make_test_deps(
                    vec![("agent-123", "Code Planner")],
                    vec![MockStep::text("planned")],
                );
                let tool = SpawnSubagentTool::new(deps);
                let result = tool
                    .execute(json!({"task": "plan task", "wait": true}))
                    .await
                    .unwrap();
                assert!(result.success);
                assert_eq!(result.result["status"], "completed");
            }

            #[tokio::test]
            async fn test_spawn_subagent_rejects_inline_fields_with_agent() {
                let deps = make_test_deps(vec![("coder", "Coder")], vec![MockStep::text("done")]);
                let tool = SpawnSubagentTool::new(deps);
                let result = tool
                    .execute(json!({
                        "agent": "coder",
                        "task": "Write code",
                        "inline_system_prompt": "You are temporary"
                    }))
                    .await;
                assert!(result.is_err());
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("cannot be combined")
                );
            }

            #[tokio::test]
            async fn test_spawn_subagent_supports_workers_list_mode() {
                let deps = make_test_deps(
                    vec![("coder", "Coder")],
                    vec![MockStep::text("done-1"), MockStep::text("done-2")],
                );
                let tool = SpawnSubagentTool::new(deps);
                let result = tool
                    .execute(json!({
                        "task": "batch task",
                        "wait": true,
                        "workers": [
                            { "agent": "coder", "count": 2 }
                        ]
                    }))
                    .await
                    .unwrap();
                assert!(result.success);
                assert_eq!(result.result["status"], "completed");
                assert_eq!(result.result["spawned_count"], 2);
            }

            #[tokio::test]
            async fn test_spawn_subagent_rejects_mixed_single_and_workers_mode_fields() {
                let deps = make_test_deps(vec![("coder", "Coder")], vec![MockStep::text("done")]);
                let tool = SpawnSubagentTool::new(deps);
                let result = tool
                    .execute(json!({
                        "task": "batch task",
                        "agent": "coder",
                        "workers": [
                            { "agent": "coder", "count": 1 }
                        ]
                    }))
                    .await;
                assert!(result.is_err());
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("Batch mode uses 'workers'")
                );
            }

            #[tokio::test]
            async fn test_spawn_subagent_workers_support_distinct_tasks_list() {
                let deps = make_test_deps(
                    vec![("coder", "Coder")],
                    vec![MockStep::text("done-a"), MockStep::text("done-b")],
                );
                let tool = SpawnSubagentTool::new(deps);
                let result = tool
                    .execute(json!({
                        "task": "",
                        "wait": true,
                        "workers": [
                            { "agent": "coder", "tasks": ["task-A", "task-B"] }
                        ]
                    }))
                    .await
                    .unwrap();

                assert!(result.success);
                assert_eq!(result.result["status"], "completed");
                assert_eq!(result.result["spawned_count"], 2);
                let results = result.result["results"]
                    .as_array()
                    .expect("results should be array");
                assert_eq!(results.len(), 2);
                assert!(results.iter().all(|entry| entry["status"] == "completed"));
            }

            #[test]
            fn test_parameters_schema_uses_dynamic_agent_ids() {
                let deps = make_test_deps(
                    vec![("agent-1", "Researcher"), ("agent-2", "Coder")],
                    vec![],
                );
                let tool = SpawnSubagentTool::new(deps);
                let schema = tool.parameters_schema();
                let values = schema["properties"]["agent"]["enum"]
                    .as_array()
                    .expect("agent enum should exist");
                let ids = values
                    .iter()
                    .filter_map(|value| value.as_str())
                    .collect::<Vec<_>>();
                assert!(ids.contains(&"agent-1"));
                assert!(ids.contains(&"agent-2"));
                assert_eq!(
                    schema["properties"]["timeout_secs"]["default"],
                    json!(DEFAULT_SUBAGENT_TIMEOUT_SECS)
                );
            }
        }
    }

    pub mod spawn_subagent_batch {
        // spawn_subagent_batch tool - Batch spawn sub-agents.

        use async_trait::async_trait;
        use serde::{Deserialize, Serialize};
        use serde_json::{Value, json};
        use std::sync::Arc;
        use tokio::time::{Duration, timeout};

        use crate::{Result, Tool, ToolError, ToolOutput};
        use ::types::{SubagentManager, subagent::SubagentDefSummary};
        use types::request::{
            InlineAgentRunConfig as ContractInlineAgentRunConfig,
            RunSpawnRequest as ContractRunSpawnRequest,
        };
        use types::subagent::spawn_request_from_contract;
        use types::{SubagentCompletion, SubagentEffectiveLimits, SubagentResult, SubagentStatus};

        /// Operation for spawn_subagent_batch tool.
        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
        #[serde(rename_all = "snake_case")]
        pub enum SpawnSubagentBatchOperation {
            /// Spawn one batch of sub-agents immediately.
            #[default]
            Spawn,
        }

        fn default_member_count() -> u32 {
            1
        }

        /// One batch member specification.
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct BatchSubagentSpec {
            /// Optional agent ID or name. If omitted, a temporary sub-agent is created.
            #[serde(default)]
            pub agent: Option<String>,
            /// Number of identical sub-agents to spawn for this spec.
            #[serde(default = "default_member_count")]
            pub count: u32,
            /// Optional transient per-spec task override.
            #[serde(default)]
            pub task: Option<String>,
            /// Optional transient per-instance task list.
            #[serde(default)]
            pub tasks: Option<Vec<String>>,
            /// Optional per-spec timeout passed to sub-agent execution.
            #[serde(default)]
            pub timeout_secs: Option<u64>,
            /// Optional model override.
            #[serde(default)]
            pub model: Option<String>,
            /// Optional provider override paired with model.
            #[serde(default)]
            pub provider: Option<String>,
            /// Optional name for temporary sub-agent creation.
            #[serde(default)]
            pub inline_name: Option<String>,
            /// Optional system prompt for temporary sub-agent creation.
            #[serde(default)]
            pub inline_system_prompt: Option<String>,
            /// Optional allowlist for temporary sub-agent tools.
            #[serde(default)]
            pub inline_allowed_tools: Option<Vec<String>>,
            /// Optional max iterations override for temporary sub-agent creation.
            #[serde(default)]
            pub inline_max_iterations: Option<u32>,
        }

        /// Parameters for spawn_subagent_batch tool.
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct SpawnSubagentBatchParams {
            /// Operation to perform.
            #[serde(default)]
            pub operation: SpawnSubagentBatchOperation,
            /// Batch member specs. Required for spawn.
            #[serde(default)]
            pub specs: Option<Vec<BatchSubagentSpec>>,
            /// Default transient task for specs that do not set per-spec task.
            #[serde(default)]
            pub task: Option<String>,
            /// Transient per-instance task list for this spawn.
            #[serde(default)]
            pub tasks: Option<Vec<String>>,
            /// If true, wait for all spawned tasks to complete.
            #[serde(default)]
            pub wait: bool,
            /// Timeout in seconds for wait and as fallback spawn timeout.
            #[serde(default)]
            pub timeout_secs: Option<u64>,
            /// Optional parent run ID for context propagation.
            #[serde(default)]
            pub parent_run_id: Option<String>,
        }

        #[derive(Debug, Clone)]
        struct SpawnedTask {
            task_id: String,
            agent_name: String,
            spec_index: usize,
            instance_index: u32,
            effective_limits: SubagentEffectiveLimits,
        }

        #[derive(Debug, Clone)]
        struct PreparedSpawnRequest {
            spec_index: usize,
            instance_index: u32,
            request: ContractRunSpawnRequest,
        }

        #[derive(Debug)]
        struct SpawnFailure {
            spec_index: usize,
            instance_index: u32,
            error: ToolError,
        }

        /// spawn_subagent_batch tool for shared agent execution engine.
        pub struct SpawnSubagentBatchTool {
            manager: Arc<dyn SubagentManager>,
        }

        impl SpawnSubagentBatchTool {
            pub fn new(manager: Arc<dyn SubagentManager>) -> Self {
                Self { manager }
            }

            fn available_agents(&self) -> Vec<SubagentDefSummary> {
                self.manager.list_callable()
            }
        }

        #[async_trait]
        impl Tool for SpawnSubagentBatchTool {
            fn name(&self) -> &str {
                "spawn_subagent_batch"
            }

            fn description(&self) -> &str {
                "Batch spawn sub-agents with explicit model/count specs."
            }

            fn parameters_schema(&self) -> Value {
                parameters_schema()
            }

            async fn execute(&self, input: Value) -> Result<ToolOutput> {
                let params: SpawnSubagentBatchParams = serde_json::from_value(input)
                    .map_err(|err| ToolError::Tool(format!("Invalid parameters: {}", err)))?;

                match params.operation {
                    SpawnSubagentBatchOperation::Spawn => spawn_batch(self, params).await,
                }
            }
        }

        fn parameters_schema() -> Value {
            json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["spawn"],
                        "default": "spawn",
                        "description": "Operation to perform."
                    },
                    "specs": {
                        "type": "array",
                        "description": "Batch member specs. Required for spawn.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "agent": {
                                    "type": "string",
                                    "description": "Optional agent ID or name. Omit for a temporary child run."
                                },
                                "count": {
                                    "type": "integer",
                                    "minimum": 1,
                                    "default": 1,
                                    "description": "How many child runs to spawn for this spec."
                                },
                                "task": {
                                    "type": "string",
                                    "description": "Optional per-spec task override."
                                },
                                "tasks": {
                                    "type": "array",
                                    "items": { "type": "string" },
                                    "description": "Optional per-instance task list. When set, each spawned instance uses one prompt from this list."
                                },
                                "timeout_secs": {
                                    "type": "integer",
                                    "minimum": 0,
                                    "description": "Optional per-spec timeout in seconds."
                                },
                                "model": {
                                    "type": "string",
                                    "description": "Optional model override."
                                },
                                "provider": {
                                    "type": "string",
                                    "description": "Optional provider paired with model."
                                },
                                "inline_name": {
                                    "type": "string",
                                    "description": "Optional temporary child-run name."
                                },
                                "inline_system_prompt": {
                                    "type": "string",
                                    "description": "Optional temporary child-run system prompt."
                                },
                                "inline_allowed_tools": {
                                    "type": "array",
                                    "items": { "type": "string" },
                                    "description": "Optional temporary child-run tool allowlist."
                                },
                                "inline_max_iterations": {
                                    "type": "integer",
                                    "minimum": 1,
                                    "description": "Optional temporary child-run max iterations."
                                }
                            }
                        }
                    },
                    "task": {
                        "type": "string",
                        "description": "Default task for specs that do not define per-spec 'task' or 'tasks'."
                    },
                    "tasks": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Per-instance task list for this spawn. Tasks are assigned in spec order."
                    },
                    "wait": {
                        "type": "boolean",
                        "default": false,
                        "description": "If true, wait for all spawned tasks."
                    },
                    "timeout_secs": {
                        "type": "integer",
                        "minimum": 0,
                        "description": "Wait timeout and fallback child-run timeout (seconds). Use 0 for no wait timeout."
                    },
                    "parent_run_id": {
                        "type": "string",
                        "description": "Optional parent run ID for context propagation (runtime-injected)."
                    }
                }
            })
        }

        fn build_inline_config(spec: &BatchSubagentSpec) -> Option<ContractInlineAgentRunConfig> {
            let config = ContractInlineAgentRunConfig {
                name: spec.inline_name.clone(),
                system_prompt: spec.inline_system_prompt.clone(),
                allowed_tools: spec.inline_allowed_tools.clone(),
                max_iterations: spec.inline_max_iterations,
            };
            if config.name.is_none()
                && config.system_prompt.is_none()
                && config.allowed_tools.is_none()
                && config.max_iterations.is_none()
            {
                None
            } else {
                Some(config)
            }
        }

        fn preview_request_from_spec(spec: &BatchSubagentSpec) -> ContractRunSpawnRequest {
            ContractRunSpawnRequest {
                agent_id: spec.agent.clone(),
                inline: build_inline_config(spec),
                task: "Structural team preview".to_string(),
                timeout_secs: spec.timeout_secs,
                max_iterations: None,
                priority: None,
                model: spec.model.clone(),
                model_provider: spec.provider.clone(),
                parent_run_id: None,
            }
        }

        fn spawn_request_from_spec(
            spec: &BatchSubagentSpec,
            task: String,
            params: &SpawnSubagentBatchParams,
        ) -> ContractRunSpawnRequest {
            ContractRunSpawnRequest {
                agent_id: spec.agent.clone(),
                inline: build_inline_config(spec),
                task,
                timeout_secs: spec.timeout_secs.or(params.timeout_secs),
                max_iterations: None,
                priority: None,
                model: spec.model.clone(),
                model_provider: spec.provider.clone(),
                parent_run_id: params.parent_run_id.clone(),
            }
        }

        fn total_instances(specs: &[BatchSubagentSpec]) -> Result<usize> {
            let mut total: usize = 0;
            for (spec_index, spec) in specs.iter().enumerate() {
                if spec.task.is_some() && spec.tasks.is_some() {
                    return Err(ToolError::Tool(format!(
                        "Spec index {} cannot set both 'task' and 'tasks'.",
                        spec_index
                    )));
                }

                if let Some(tasks) = &spec.tasks {
                    if tasks.is_empty() {
                        return Err(ToolError::Tool(format!(
                            "Spec index {} has empty 'tasks'.",
                            spec_index
                        )));
                    }

                    for (task_index, task) in tasks.iter().enumerate() {
                        if task.trim().is_empty() {
                            return Err(ToolError::Tool(format!(
                                "Spec index {} has empty task at tasks[{}].",
                                spec_index, task_index
                            )));
                        }
                    }

                    if spec.count != 1 && spec.count as usize != tasks.len() {
                        return Err(ToolError::Tool(format!(
                            "Spec index {} has count={} but tasks.len()={}. Set count to 1 (default) or match tasks length.",
                            spec_index,
                            spec.count,
                            tasks.len()
                        )));
                    }

                    total = total.saturating_add(tasks.len());
                    continue;
                }

                if spec.count == 0 {
                    return Err(ToolError::Tool("Each spec count must be >= 1.".to_string()));
                }
                total = total.saturating_add(spec.count as usize);
            }
            if total == 0 {
                return Err(ToolError::Tool("No sub-agents requested.".to_string()));
            }
            Ok(total)
        }

        fn validate_structural_specs(
            tool: &SpawnSubagentBatchTool,
            specs: &[BatchSubagentSpec],
        ) -> Result<()> {
            let _ = total_instances(specs)?;
            for spec in specs {
                let _ = spawn_request_from_contract(
                    &tool.available_agents(),
                    preview_request_from_spec(spec),
                )?;
            }
            Ok(())
        }

        fn structural_count(spec: &BatchSubagentSpec, spec_index: usize) -> Result<u32> {
            if spec.count == 0 {
                return Err(ToolError::Tool(format!(
                    "Spec index {} count must be >= 1.",
                    spec_index
                )));
            }
            Ok(spec
                .tasks
                .as_ref()
                .map_or(spec.count, |tasks| tasks.len() as u32))
        }

        fn resolve_instance_tasks(
            spec: &BatchSubagentSpec,
            fallback_task: Option<&str>,
            spec_index: usize,
        ) -> Result<Vec<String>> {
            if spec.task.is_some() && spec.tasks.is_some() {
                return Err(ToolError::Tool(format!(
                    "Spec index {} cannot set both 'task' and 'tasks'.",
                    spec_index
                )));
            }

            if let Some(tasks) = &spec.tasks {
                if tasks.is_empty() {
                    return Err(ToolError::Tool(format!(
                        "Spec index {} has empty 'tasks'.",
                        spec_index
                    )));
                }

                let mut resolved = Vec::with_capacity(tasks.len());
                for (task_index, task) in tasks.iter().enumerate() {
                    let trimmed = task.trim();
                    if trimmed.is_empty() {
                        return Err(ToolError::Tool(format!(
                            "Spec index {} has empty task at tasks[{}].",
                            spec_index, task_index
                        )));
                    }
                    resolved.push(trimmed.to_string());
                }
                return Ok(resolved);
            }

            let task = spec.task.as_deref().or(fallback_task).ok_or_else(|| {
                ToolError::Tool(format!(
                    "Missing task for spec index {}. Provide top-level 'task', top-level 'tasks', per-spec 'task', or per-spec 'tasks'.",
                    spec_index
                ))
            })?;
            let trimmed = task.trim();
            if trimmed.is_empty() {
                return Err(ToolError::Tool(format!(
                    "Task for spec index {} must not be empty.",
                    spec_index
                )));
            }

            Ok((0..spec.count).map(|_| trimmed.to_string()).collect())
        }

        fn resolve_batch_tasks(
            specs: &[BatchSubagentSpec],
            fallback_task: Option<&str>,
            fallback_tasks: Option<&[String]>,
        ) -> Result<Vec<Vec<String>>> {
            if fallback_task.is_some() && fallback_tasks.is_some() {
                return Err(ToolError::Tool(
                    "Use either top-level 'task' or top-level 'tasks', not both.".to_string(),
                ));
            }

            if let Some(tasks) = fallback_tasks {
                if tasks.is_empty() {
                    return Err(ToolError::Tool(
                        "Top-level 'tasks' must not be empty.".to_string(),
                    ));
                }

                for (spec_index, spec) in specs.iter().enumerate() {
                    if spec.task.is_some() || spec.tasks.is_some() {
                        return Err(ToolError::Tool(format!(
                            "Top-level 'tasks' cannot be combined with per-spec 'task' or 'tasks' (spec index {}).",
                            spec_index
                        )));
                    }
                }

                let mut normalized = Vec::with_capacity(tasks.len());
                for (task_index, task) in tasks.iter().enumerate() {
                    let trimmed = task.trim();
                    if trimmed.is_empty() {
                        return Err(ToolError::Tool(format!(
                            "Top-level 'tasks' has empty task at index {}.",
                            task_index
                        )));
                    }
                    normalized.push(trimmed.to_string());
                }

                let expected = total_instances(specs)?;
                if normalized.len() != expected {
                    return Err(ToolError::Tool(format!(
                        "Top-level 'tasks' length {} does not match total requested instances {}.",
                        normalized.len(),
                        expected
                    )));
                }

                let mut offset = 0usize;
                let mut resolved = Vec::with_capacity(specs.len());
                for (spec_index, spec) in specs.iter().enumerate() {
                    let count =
                        usize::try_from(structural_count(spec, spec_index)?).map_err(|_| {
                            ToolError::Tool(format!(
                                "Spec index {} count exceeds supported runtime size.",
                                spec_index
                            ))
                        })?;
                    let end = offset + count;
                    resolved.push(normalized[offset..end].to_vec());
                    offset = end;
                }

                return Ok(resolved);
            }

            specs
                .iter()
                .enumerate()
                .map(|(spec_index, spec)| resolve_instance_tasks(spec, fallback_task, spec_index))
                .collect()
        }

        fn specs_for_spawn(
            tool: &SpawnSubagentBatchTool,
            params: &SpawnSubagentBatchParams,
        ) -> Result<Vec<BatchSubagentSpec>> {
            let specs = params
                .specs
                .clone()
                .ok_or_else(|| ToolError::Tool("Spawn requires non-empty 'specs'.".to_string()))?;

            if specs.is_empty() {
                return Err(ToolError::Tool("Specs must not be empty.".to_string()));
            }

            validate_structural_specs(tool, &specs)?;

            for spec in &specs {
                if spec.task.is_some() && spec.tasks.is_some() {
                    return Err(ToolError::Tool(
                        "Each spec can use either 'task' or 'tasks', not both.".to_string(),
                    ));
                }
            }

            Ok(specs)
        }

        async fn wait_result(
            tool: &SpawnSubagentBatchTool,
            task_id: &str,
            timeout_secs: u64,
        ) -> Option<SubagentCompletion> {
            if timeout_secs == 0 {
                return tool.manager.wait(task_id).await;
            }
            timeout(
                Duration::from_secs(timeout_secs),
                tool.manager.wait(task_id),
            )
            .await
            .unwrap_or_default()
        }

        fn task_entries(spawned: &[SpawnedTask]) -> Vec<Value> {
            spawned
                .iter()
                .map(|task| {
                    json!({
                        "task_id": task.task_id,
                        "agent": task.agent_name,
                        "spec_index": task.spec_index,
                        "instance_index": task.instance_index,
                        "effective_limits": task.effective_limits,
                    })
                })
                .collect()
        }

        async fn wait_for_spawned_tasks(
            tool: &SpawnSubagentBatchTool,
            spawned: &[SpawnedTask],
            wait_timeout: u64,
        ) -> Vec<Value> {
            let mut results = Vec::with_capacity(spawned.len());
            for task in spawned {
                let wait_result = wait_result(tool, &task.task_id, wait_timeout).await;
                match wait_result {
                    Some(completion) if completion.status == SubagentStatus::Completed => {
                        let result = completion.result.unwrap_or(SubagentResult {
                            success: true,
                            output: String::new(),
                            summary: None,
                            duration_ms: 0,
                            tokens_used: None,
                            cost_usd: None,
                            error: None,
                        });
                        results.push(json!({
                            "task_id": task.task_id,
                            "agent": task.agent_name,
                            "spec_index": task.spec_index,
                            "instance_index": task.instance_index,
                            "status": "completed",
                            "output": result.output,
                            "duration_ms": result.duration_ms,
                            "effective_limits": task.effective_limits,
                        }))
                    }
                    Some(completion) => {
                        let status = match completion.status {
                            SubagentStatus::Interrupted => "interrupted",
                            SubagentStatus::TimedOut => "timed_out",
                            SubagentStatus::Failed => "failed",
                            SubagentStatus::Pending => "pending",
                            SubagentStatus::Running => "running",
                            SubagentStatus::Completed => "completed",
                        };
                        let result = completion.result;
                        results.push(json!({
                            "task_id": task.task_id,
                            "agent": task.agent_name,
                            "spec_index": task.spec_index,
                            "instance_index": task.instance_index,
                            "status": status,
                            "error": result.as_ref().and_then(|value| value.error.clone()).unwrap_or_else(|| "Unknown error".to_string()),
                            "duration_ms": result.as_ref().map(|value| value.duration_ms).unwrap_or_default(),
                            "effective_limits": task.effective_limits,
                        }));
                    }
                    None => results.push(json!({
                        "task_id": task.task_id,
                        "agent": task.agent_name,
                        "spec_index": task.spec_index,
                        "instance_index": task.instance_index,
                        "status": "timeout",
                        "effective_limits": task.effective_limits,
                    })),
                }
            }
            results
        }

        async fn spawn_batch(
            tool: &SpawnSubagentBatchTool,
            params: SpawnSubagentBatchParams,
        ) -> Result<ToolOutput> {
            let specs = specs_for_spawn(tool, &params)?;
            let total_requested = total_instances(&specs)?;
            let max_parallel = tool.manager.config().max_parallel_agents;
            let running_now = tool.manager.running_count();
            let available_slots = max_parallel.saturating_sub(running_now);
            if total_requested > available_slots {
                return Err(ToolError::Tool(format!(
                    "Requested {} sub-agents, but only {} slots are available (running: {}, max_parallel: {}).",
                    total_requested, available_slots, running_now, max_parallel
                )));
            }

            let resolved_tasks =
                resolve_batch_tasks(&specs, params.task.as_deref(), params.tasks.as_deref())?;

            let mut prepared = Vec::with_capacity(total_requested);
            for (spec_index, (spec, instance_tasks)) in specs.iter().zip(resolved_tasks).enumerate()
            {
                for (instance_index, task) in instance_tasks.into_iter().enumerate() {
                    if instance_index > u32::MAX as usize {
                        return Err(ToolError::Tool(format!(
                            "Spec index {} has too many instances to index as u32.",
                            spec_index
                        )));
                    }
                    let request = spawn_request_from_spec(spec, task, &params);
                    prepared.push(PreparedSpawnRequest {
                        spec_index,
                        instance_index: instance_index as u32,
                        request,
                    });
                }
            }

            let mut spawned = Vec::with_capacity(prepared.len());
            let mut spawn_failure = None;
            for item in prepared {
                match tool.manager.spawn(item.request) {
                    Ok(handle) => spawned.push(SpawnedTask {
                        task_id: handle.id,
                        agent_name: handle.agent_name,
                        spec_index: item.spec_index,
                        instance_index: item.instance_index,
                        effective_limits: handle.effective_limits,
                    }),
                    Err(error) => {
                        spawn_failure = Some(SpawnFailure {
                            spec_index: item.spec_index,
                            instance_index: item.instance_index,
                            error,
                        });
                        break;
                    }
                }
            }

            if let Some(failure) = spawn_failure {
                if spawned.is_empty() {
                    return Err(failure.error);
                }

                let wait_timeout = params
                    .timeout_secs
                    .unwrap_or(tool.manager.config().subagent_timeout_secs);
                let tasks = task_entries(&spawned);
                let task_ids = spawned
                    .iter()
                    .map(|task| task.task_id.clone())
                    .collect::<Vec<_>>();
                let mut payload = json!({
                    "operation": "spawn",
                    "status": "partial_failure",
                    "spawned_count": spawned.len(),
                    "running_before": running_now,
                    "max_parallel": max_parallel,
                    "task_ids": task_ids,
                    "tasks": tasks,
                    "failed_spec_index": failure.spec_index,
                    "failed_instance_index": failure.instance_index,
                    "error": failure.error.to_string(),
                });

                if params.wait {
                    payload["results"] =
                        Value::Array(wait_for_spawned_tasks(tool, &spawned, wait_timeout).await);
                }

                return Ok(ToolOutput::success(payload));
            }

            if !params.wait {
                let tasks = task_entries(&spawned);
                return Ok(ToolOutput::success(json!({
                    "operation": "spawn",
                    "status": "spawned",
                    "spawned_count": spawned.len(),
                    "running_before": running_now,
                    "max_parallel": max_parallel,
                    "task_ids": spawned.iter().map(|task| task.task_id.clone()).collect::<Vec<_>>(),
                    "tasks": tasks
                })));
            }

            let wait_timeout = params
                .timeout_secs
                .unwrap_or(tool.manager.config().subagent_timeout_secs);
            let results = wait_for_spawned_tasks(tool, &spawned, wait_timeout).await;

            Ok(ToolOutput::success(json!({
                "operation": "spawn",
                "status": "completed",
                "spawned_count": spawned.len(),
                "results": results
            })))
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use crate::Tool;
            use crate::ToolRegistry;
            use crate::impls::spawn_subagent_batch::SpawnSubagentBatchParams;
            use ::agent::agent::{
                SubagentConfig, SubagentDefLookup, SubagentDefSnapshot, SubagentDefSummary,
                SubagentManagerImpl, SubagentTracker,
            };
            use ::agent::llm::{MockLlmClient, MockStep};
            use ::types::request::RunSpawnRequest as ContractRunSpawnRequest;
            use ::types::{SpawnHandle, SubagentCompletion, SubagentManager, SubagentState};
            use serde_json::json;
            use std::collections::HashMap;
            use std::sync::{Arc, Mutex};
            use tokio::sync::mpsc;

            struct MockDefLookup {
                defs: HashMap<String, SubagentDefSnapshot>,
                summaries: Vec<SubagentDefSummary>,
            }

            impl MockDefLookup {
                fn with_agents(agents: Vec<(&str, &str)>) -> Self {
                    let mut defs = HashMap::new();
                    let mut summaries = Vec::new();
                    for (id, name) in agents {
                        defs.insert(
                            id.to_string(),
                            SubagentDefSnapshot {
                                name: name.to_string(),
                                system_prompt: format!("You are a {} agent.", name),
                                allowed_tools: vec![],
                                max_iterations: Some(1),
                                default_model: None,
                            },
                        );
                        summaries.push(SubagentDefSummary {
                            id: id.to_string(),
                            name: name.to_string(),
                            description: format!("{} agent", name),
                            tags: vec![],
                        });
                    }
                    Self { defs, summaries }
                }
            }

            impl SubagentDefLookup for MockDefLookup {
                fn lookup(&self, id: &str) -> Option<SubagentDefSnapshot> {
                    self.defs.get(id).cloned()
                }
                fn list_callable(&self) -> Vec<SubagentDefSummary> {
                    self.summaries.clone()
                }
            }

            fn make_test_manager(
                agents: Vec<(&str, &str)>,
                mock_steps: Vec<MockStep>,
            ) -> Arc<dyn SubagentManager> {
                let (tx, rx) = mpsc::channel(32);
                let tracker = Arc::new(SubagentTracker::new(tx, rx));
                let definitions: Arc<dyn SubagentDefLookup> =
                    Arc::new(MockDefLookup::with_agents(agents));
                let llm_client = Arc::new(MockLlmClient::from_steps("mock", mock_steps));
                let tool_registry = Arc::new(ToolRegistry::new());
                let config = SubagentConfig {
                    max_parallel_agents: 20,
                    subagent_timeout_secs: 10,
                    max_iterations: 5,
                    max_depth: 1,
                };
                Arc::new(SubagentManagerImpl::new(
                    tracker,
                    definitions,
                    llm_client,
                    tool_registry,
                    config,
                ))
            }

            struct FailingSpawnManager {
                inner: Arc<dyn SubagentManager>,
                fail_on_attempt: usize,
                attempts: Mutex<usize>,
            }

            impl FailingSpawnManager {
                fn new(inner: Arc<dyn SubagentManager>, fail_on_attempt: usize) -> Self {
                    Self {
                        inner,
                        fail_on_attempt,
                        attempts: Mutex::new(0),
                    }
                }
            }

            #[async_trait]
            impl SubagentManager for FailingSpawnManager {
                fn spawn(
                    &self,
                    request: ContractRunSpawnRequest,
                ) -> std::result::Result<SpawnHandle, ToolError> {
                    let mut attempts = self
                        .attempts
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    *attempts += 1;
                    if *attempts == self.fail_on_attempt {
                        return Err(ToolError::Tool("Injected spawn failure".to_string()));
                    }
                    self.inner.spawn(request)
                }

                fn list_callable(&self) -> Vec<SubagentDefSummary> {
                    self.inner.list_callable()
                }

                fn list_running(&self) -> Vec<SubagentState> {
                    self.inner.list_running()
                }

                fn running_count(&self) -> usize {
                    self.inner.running_count()
                }

                async fn wait(&self, task_id: &str) -> Option<SubagentCompletion> {
                    self.inner.wait(task_id).await
                }

                async fn wait_for_parent_owned_task(
                    &self,
                    task_id: &str,
                    parent_run_id: &str,
                ) -> Option<SubagentCompletion> {
                    self.inner
                        .wait_for_parent_owned_task(task_id, parent_run_id)
                        .await
                }

                fn config(&self) -> &SubagentConfig {
                    self.inner.config()
                }
            }

            #[tokio::test]
            async fn test_spawn_batch_waits_for_all_instances() {
                let manager = make_test_manager(
                    vec![("coder", "Coder")],
                    vec![
                        MockStep::text("done-1"),
                        MockStep::text("done-2"),
                        MockStep::text("done-3"),
                    ],
                );
                let tool = SpawnSubagentBatchTool::new(manager);
                let output = tool
                    .execute(json!({
                        "operation": "spawn",
                        "task": "Implement fixes",
                        "wait": true,
                        "specs": [
                            { "agent": "coder", "count": 3 }
                        ]
                    }))
                    .await
                    .unwrap();

                assert!(output.success);
                assert_eq!(output.result["status"], "completed");
                assert_eq!(output.result["spawned_count"], 3);
                assert_eq!(output.result["results"].as_array().unwrap().len(), 3);
            }

            #[tokio::test]
            async fn test_spawn_batch_rejects_provider_without_model() {
                let manager =
                    make_test_manager(vec![("coder", "Coder")], vec![MockStep::text("done")]);
                let tool = SpawnSubagentBatchTool::new(manager);
                let result = tool
                    .execute(json!({
                        "operation": "spawn",
                        "task": "Implement fixes",
                        "specs": [
                            { "agent": "coder", "count": 1, "provider": "openai-codex" }
                        ]
                    }))
                    .await;
                assert!(result.is_err());
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("requires both 'model' and 'provider'")
                );
            }

            #[tokio::test]
            async fn test_spawn_batch_supports_distinct_tasks_list() {
                let manager = make_test_manager(
                    vec![("coder", "Coder")],
                    vec![
                        MockStep::text("done-1"),
                        MockStep::text("done-2"),
                        MockStep::text("done-3"),
                    ],
                );
                let tool = SpawnSubagentBatchTool::new(manager);
                let output = tool
                    .execute(json!({
                        "operation": "spawn",
                        "wait": true,
                        "specs": [
                            { "agent": "coder", "tasks": ["task-1", "task-2", "task-3"] }
                        ]
                    }))
                    .await
                    .unwrap();

                assert!(output.success);
                assert_eq!(output.result["status"], "completed");
                assert_eq!(output.result["spawned_count"], 3);
                let results = output.result["results"]
                    .as_array()
                    .expect("results should be array");
                assert_eq!(results.len(), 3);
                assert!(results.iter().all(|entry| entry["status"] == "completed"));
            }

            #[tokio::test]
            async fn test_spawn_batch_rejects_task_and_tasks_together() {
                let manager =
                    make_test_manager(vec![("coder", "Coder")], vec![MockStep::text("done")]);
                let tool = SpawnSubagentBatchTool::new(manager);

                let result = tool
                    .execute(json!({
                        "operation": "spawn",
                        "specs": [
                            { "agent": "coder", "task": "single", "tasks": ["task-1"] }
                        ]
                    }))
                    .await;

                assert!(result.is_err());
                let message = result.unwrap_err().to_string();
                assert!(
                    message.contains("either 'task' or 'tasks'")
                        || message.contains("both 'task' and 'tasks'")
                );
            }

            #[tokio::test]
            async fn test_spawn_batch_rejects_tasks_count_mismatch() {
                let manager =
                    make_test_manager(vec![("coder", "Coder")], vec![MockStep::text("done")]);
                let tool = SpawnSubagentBatchTool::new(manager);

                let result = tool
                    .execute(json!({
                        "operation": "spawn",
                        "specs": [
                            { "agent": "coder", "count": 2, "tasks": ["task-1", "task-2", "task-3"] }
                        ]
                    }))
                    .await;

                assert!(result.is_err());
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("Set count to 1 (default) or match tasks length")
                );
            }

            #[tokio::test]
            async fn test_spawn_batch_requires_task_when_spec_has_no_override() {
                let manager =
                    make_test_manager(vec![("coder", "Coder")], vec![MockStep::text("done")]);
                let tool = SpawnSubagentBatchTool::new(manager);

                let result = tool
                    .execute(json!({
                        "operation": "spawn",
                        "specs": [
                            { "agent": "coder", "count": 1 }
                        ]
                    }))
                    .await;

                assert!(result.is_err());
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("Missing task for spec index 0")
                );
            }

            #[tokio::test]
            async fn test_spawn_batch_rejects_when_requested_instances_exceed_slots() {
                let manager =
                    make_test_manager(vec![("coder", "Coder")], vec![MockStep::text("done")]);
                let tool = SpawnSubagentBatchTool::new(manager);

                let result = tool
                    .execute(json!({
                        "operation": "spawn",
                        "task": "Implement fixes",
                        "specs": [
                            { "agent": "coder", "count": 21 }
                        ]
                    }))
                    .await;

                assert!(result.is_err());
                let message = result.unwrap_err().to_string();
                assert!(message.contains("Requested 21 sub-agents"));
                assert!(message.contains("max_parallel: 20"));
            }

            #[tokio::test]
            async fn test_spawn_batch_returns_spawned_tasks_on_partial_failure() {
                let inner = make_test_manager(
                    vec![("coder", "Coder")],
                    vec![MockStep::text("done-1"), MockStep::text("done-2")],
                );
                let manager: Arc<dyn SubagentManager> =
                    Arc::new(FailingSpawnManager::new(inner, 2));
                let tool = SpawnSubagentBatchTool::new(manager);

                let output = tool
                    .execute(json!({
                        "operation": "spawn",
                        "task": "Implement fixes",
                        "specs": [
                            { "agent": "coder", "count": 2 }
                        ]
                    }))
                    .await
                    .expect("partial failure should still produce output");

                assert!(output.success);
                assert_eq!(output.result["status"], "partial_failure");
                assert_eq!(output.result["spawned_count"], 1);
                assert_eq!(output.result["failed_spec_index"], 0);
                assert_eq!(output.result["failed_instance_index"], 1);
                assert!(
                    output.result["error"]
                        .as_str()
                        .expect("error message")
                        .contains("Injected spawn failure")
                );
                let task_ids = output.result["task_ids"]
                    .as_array()
                    .expect("task_ids should be array");
                assert_eq!(task_ids.len(), 1);
                let tasks = output.result["tasks"]
                    .as_array()
                    .expect("tasks should be array");
                assert_eq!(tasks.len(), 1);
                assert_eq!(tasks[0]["instance_index"], 0);
            }

            #[test]
            fn test_batch_schema_exposes_parent_run_id() {
                let schema = super::parameters_schema();
                let properties = schema["properties"]
                    .as_object()
                    .expect("schema properties should be an object");
                assert!(properties.contains_key("parent_run_id"));
            }

            #[test]
            fn test_batch_params_use_canonical_parent_run_id() {
                let params: SpawnSubagentBatchParams = serde_json::from_value(json!({
                    "parent_run_id": "run-123"
                }))
                .expect("params should deserialize");

                assert_eq!(params.parent_run_id.as_deref(), Some("run-123"));

                let serialized = serde_json::to_value(&params).expect("params should serialize");
                assert_eq!(serialized["parent_run_id"], "run-123");
            }
        }
    }

    pub mod wait_subagents {
        // wait_subagents tool - Wait for sub-agents to finish and return results.

        use async_trait::async_trait;
        use serde::{Deserialize, Serialize};
        use serde_json::{Value, json};
        use std::sync::Arc;
        use tokio::time::{Duration, timeout};

        use crate::impls::subagent_read_capability::SubagentReadCapabilityService;
        use crate::{Result, ToolError};
        use crate::{Tool, ToolOutput};
        use types::{DEFAULT_SUBAGENT_TIMEOUT_SECS, SubagentManager, SubagentStatus};

        /// Parameters for wait_subagents tool.
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct WaitSubagentsParams {
            /// Task IDs to wait for.
            pub task_ids: Vec<String>,

            /// Parent run scope that owns the requested tasks.
            #[serde(default)]
            pub parent_run_id: Option<String>,

            /// Timeout in seconds.
            /// - `Some(0)` means wait without timeout.
            /// - `None` uses subagent manager default timeout.
            #[serde(default)]
            pub timeout_secs: Option<u64>,
        }

        /// wait_subagents tool for the shared agent execution engine.
        pub struct WaitSubagentsTool {
            manager: Arc<dyn SubagentManager>,
            capability: SubagentReadCapabilityService,
        }

        impl WaitSubagentsTool {
            pub fn new(manager: Arc<dyn SubagentManager>) -> Self {
                let capability = SubagentReadCapabilityService::new(manager.clone());
                Self {
                    manager,
                    capability,
                }
            }

            fn completion_entry(task_id: &str, completion: types::SubagentCompletion) -> Value {
                let status = match completion.status {
                    SubagentStatus::Completed => "completed",
                    SubagentStatus::Failed => "failed",
                    SubagentStatus::Interrupted => "interrupted",
                    SubagentStatus::TimedOut => "timed_out",
                    SubagentStatus::Pending => "pending",
                    SubagentStatus::Running => "running",
                };

                let mut entry = json!({
                    "task_id": task_id,
                    "status": status,
                });

                if let Some(result) = completion.result {
                    entry["duration_ms"] = json!(result.duration_ms);
                    if result.success {
                        entry["output"] = json!(result.output);
                    } else {
                        entry["error"] =
                            json!(result.error.unwrap_or_else(|| "Unknown error".to_string()));
                        if !result.output.is_empty() {
                            entry["output"] = json!(result.output);
                        }
                    }
                }

                entry
            }
        }

        #[async_trait]
        impl Tool for WaitSubagentsTool {
            fn name(&self) -> &str {
                "wait_subagents"
            }

            fn description(&self) -> &str {
                "Wait for one or more sub-agents to finish and return their results."
            }

            fn parameters_schema(&self) -> Value {
                json!({
                    "type": "object",
                    "properties": {
                        "task_ids": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "List of sub-agent task IDs to wait for"
                        },
                        "parent_run_id": {
                            "type": "string",
                            "description": "Required parent run scope that owns the requested task IDs"
                        },
                        "timeout_secs": {
                            "type": "integer",
                            "default": DEFAULT_SUBAGENT_TIMEOUT_SECS,
                            "minimum": 0,
                            "description": "Timeout in seconds. Use 0 to wait without timeout. If omitted, uses subagent manager default timeout."
                        }
                    },
                    "required": ["task_ids"]
                })
            }

            async fn execute(&self, input: Value) -> Result<ToolOutput> {
                let params: WaitSubagentsParams = serde_json::from_value(input)
                    .map_err(|e| ToolError::Tool(format!("Invalid parameters: {}", e)))?;
                let parent_run_id = params
                    .parent_run_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        ToolError::Tool("parent_run_id is required for wait_subagents.".to_string())
                    })?;

                let wait_timeout = params
                    .timeout_secs
                    .unwrap_or(self.manager.config().subagent_timeout_secs);

                let mut results = Vec::new();
                for task_id in params.task_ids {
                    let wait_result = if wait_timeout == 0 {
                        self.capability
                            .wait_for_parent_owned_task(&task_id, Some(parent_run_id))
                            .await?
                    } else {
                        match timeout(
                            Duration::from_secs(wait_timeout),
                            self.capability
                                .wait_for_parent_owned_task(&task_id, Some(parent_run_id)),
                        )
                        .await
                        {
                            Ok(result) => result?,
                            Err(_) => {
                                results.push(json!({"task_id": task_id, "status": "timeout"}));
                                continue;
                            }
                        }
                    };

                    let completion = match wait_result {
                        Some(result) => result,
                        None => {
                            results.push(json!({"task_id": task_id, "status": "not_found"}));
                            continue;
                        }
                    };
                    results.push(Self::completion_entry(&task_id, completion));
                }

                Ok(ToolOutput::success(json!({ "results": results })))
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use crate::Tool;
            use crate::ToolRegistry;
            use ::agent::agent::{
                SubagentConfig, SubagentDefLookup, SubagentDefSnapshot, SubagentDefSummary,
                SubagentDeps, SubagentManagerImpl, SubagentTracker,
            };
            use ::agent::llm::{MockLlmClient, MockStep};
            use std::collections::HashMap;
            use tokio::sync::mpsc;
            use types::SubagentManager;
            use types::request::RunSpawnRequest as ContractRunSpawnRequest;

            struct MockDefLookup {
                defs: HashMap<String, SubagentDefSnapshot>,
            }

            impl MockDefLookup {
                fn with_agent(id: &str) -> Self {
                    let mut defs = HashMap::new();
                    defs.insert(
                        id.to_string(),
                        SubagentDefSnapshot {
                            name: id.to_string(),
                            system_prompt: "You are a test agent.".to_string(),
                            allowed_tools: vec![],
                            max_iterations: Some(1),
                            default_model: None,
                        },
                    );
                    Self { defs }
                }
            }

            impl SubagentDefLookup for MockDefLookup {
                fn lookup(&self, id: &str) -> Option<SubagentDefSnapshot> {
                    self.defs.get(id).cloned()
                }
                fn list_callable(&self) -> Vec<SubagentDefSummary> {
                    vec![SubagentDefSummary {
                        id: "tester".to_string(),
                        name: "tester".to_string(),
                        description: "test agent".to_string(),
                        tags: vec![],
                    }]
                }
            }

            fn make_deps(
                mock_steps: Vec<MockStep>,
            ) -> (Arc<SubagentDeps>, Arc<dyn SubagentManager>) {
                let (tx, rx) = mpsc::channel(16);
                let tracker = Arc::new(SubagentTracker::new(tx, rx));
                let definitions: Arc<dyn SubagentDefLookup> =
                    Arc::new(MockDefLookup::with_agent("tester"));
                let llm_client = Arc::new(MockLlmClient::from_steps("mock", mock_steps));
                let tool_registry = Arc::new(ToolRegistry::new());
                let config = SubagentConfig {
                    max_parallel_agents: 5,
                    subagent_timeout_secs: 10,
                    max_iterations: 5,
                    max_depth: 1,
                };
                let deps = Arc::new(SubagentDeps {
                    tracker: tracker.clone(),
                    definitions,
                    llm_client,
                    tool_registry,
                    config,
                    llm_client_factory: None,
                    orchestrator: None,
                });
                let manager: Arc<dyn SubagentManager> =
                    Arc::new(SubagentManagerImpl::from_deps(&deps));
                (deps, manager)
            }

            /// Spawn a subagent that immediately completes (via MockLlmClient) and return its task_id.
            fn spawn_test_agent(manager: &Arc<dyn SubagentManager>) -> String {
                let handle = manager
                    .spawn(ContractRunSpawnRequest {
                        agent_id: Some("tester".to_string()),
                        task: "test task".to_string(),
                        timeout_secs: Some(10),
                        parent_run_id: Some("parent-1".to_string()),
                        ..ContractRunSpawnRequest::default()
                    })
                    .expect("spawn should succeed");
                handle.id
            }

            #[test]
            fn test_params_deserialization() {
                let json = r#"{"task_ids": ["task-1", "task-2"], "parent_run_id": "parent-1", "timeout_secs": 120}"#;
                let params: WaitSubagentsParams = serde_json::from_str(json).unwrap();
                assert_eq!(params.task_ids.len(), 2);
                assert_eq!(params.parent_run_id.as_deref(), Some("parent-1"));
                assert_eq!(params.timeout_secs, Some(120));
            }

            #[test]
            fn test_parameters_schema_uses_shared_timeout_default() {
                let (_deps, manager) = make_deps(vec![]);
                let tool = WaitSubagentsTool::new(manager);
                let schema = tool.parameters_schema();
                assert_eq!(
                    schema["properties"]["timeout_secs"]["default"],
                    json!(DEFAULT_SUBAGENT_TIMEOUT_SECS)
                );
            }

            #[tokio::test]
            async fn test_wait_completed_task() {
                let (_deps, manager) = make_deps(vec![MockStep::text("done")]);
                let task_id = spawn_test_agent(&manager);

                let tool = WaitSubagentsTool::new(manager);
                let result = tool
                    .execute(json!({"task_ids": [task_id], "parent_run_id": "parent-1", "timeout_secs": 1}))
                    .await
                    .unwrap();
                assert!(result.success);
                let results = result.result["results"].as_array().unwrap();
                assert_eq!(results.len(), 1);
                assert_eq!(results[0]["status"], "completed");
            }

            #[tokio::test]
            async fn test_wait_nonexistent_task() {
                let (_deps, manager) = make_deps(vec![]);
                let tool = WaitSubagentsTool::new(manager);
                let result = tool
                    .execute(json!({"task_ids": ["no-such-task"], "parent_run_id": "parent-1", "timeout_secs": 1}))
                    .await
                    .unwrap();
                assert!(result.success);
                let results = result.result["results"].as_array().unwrap();
                assert_eq!(results[0]["status"], "not_found");
            }

            #[tokio::test]
            async fn test_wait_timeout() {
                // Use a delayed step that exceeds the wait timeout.
                let (_deps, manager) = make_deps(vec![MockStep::text("slow").with_delay(2_000)]);
                let task_id = spawn_test_agent(&manager);

                let tool = WaitSubagentsTool::new(manager);
                let result = tool
                    .execute(json!({"task_ids": [task_id], "parent_run_id": "parent-1", "timeout_secs": 1}))
                    .await
                    .unwrap();
                assert!(result.success);
                let results = result.result["results"].as_array().unwrap();
                assert_eq!(results[0]["status"], "timeout");
            }

            #[tokio::test]
            async fn test_wait_interrupted_task() {
                let (deps, manager) = make_deps(vec![MockStep::text("slow").with_delay(2_000)]);
                let task_id = spawn_test_agent(&manager);
                tokio::time::sleep(Duration::from_millis(50)).await;
                assert!(deps.tracker.cancel(&task_id));

                let tool = WaitSubagentsTool::new(manager);
                let result = tool
                    .execute(json!({"task_ids": [task_id], "parent_run_id": "parent-1", "timeout_secs": 1}))
                    .await
                    .unwrap();

                assert!(result.success);
                let results = result.result["results"].as_array().unwrap();
                assert_eq!(results[0]["status"], "interrupted");
                assert_eq!(results[0]["error"], json!("Sub-agent interrupted"));
            }

            #[tokio::test]
            async fn test_wait_multiple_tasks() {
                let (_deps, manager) =
                    make_deps(vec![MockStep::text("result-1"), MockStep::text("result-2")]);
                let id1 = spawn_test_agent(&manager);
                let id2 = spawn_test_agent(&manager);

                let tool = WaitSubagentsTool::new(manager);
                let result = tool
                    .execute(json!({"task_ids": [id1, id2, "missing"], "parent_run_id": "parent-1", "timeout_secs": 1}))
                    .await
                    .unwrap();
                assert!(result.success);
                let results = result.result["results"].as_array().unwrap();
                assert_eq!(results.len(), 3);
                // First two should be completed, third not_found
                assert_eq!(results[2]["status"], "not_found");
            }

            #[tokio::test]
            async fn test_wait_with_zero_timeout_waits_for_completion() {
                let (_deps, manager) = make_deps(vec![MockStep::text("slow-done").with_delay(200)]);
                let task_id = spawn_test_agent(&manager);

                let tool = WaitSubagentsTool::new(manager);
                let result = tool
                    .execute(json!({"task_ids": [task_id], "parent_run_id": "parent-1", "timeout_secs": 0}))
                    .await
                    .unwrap();

                assert!(result.success);
                let results = result.result["results"].as_array().unwrap();
                assert_eq!(results.len(), 1);
                assert_eq!(results[0]["status"], "completed");
            }

            #[tokio::test]
            async fn test_wait_failed_task() {
                let (_deps, manager) = make_deps(vec![MockStep::error("LLM error")]);
                let task_id = spawn_test_agent(&manager);

                let tool = WaitSubagentsTool::new(manager);
                let result = tool
                    .execute(json!({"task_ids": [task_id], "parent_run_id": "parent-1", "timeout_secs": 1}))
                    .await
                    .unwrap();
                assert!(result.success);
                let results = result.result["results"].as_array().unwrap();
                assert_eq!(results[0]["status"], "failed");
                assert!(results[0]["error"].as_str().is_some());
            }

            #[tokio::test]
            async fn test_wait_requires_parent_scope() {
                let (_deps, manager) = make_deps(vec![]);
                let tool = WaitSubagentsTool::new(manager);
                let err = tool
                    .execute(json!({"task_ids": ["task-1"], "timeout_secs": 1}))
                    .await
                    .expect_err("missing parent scope should fail");
                assert!(err.to_string().contains("parent_run_id is required"));
            }

            #[tokio::test]
            async fn test_wait_rejects_foreign_parent_scope() {
                let (_deps, manager) = make_deps(vec![MockStep::text("done")]);
                let task_id = manager
                    .spawn(ContractRunSpawnRequest {
                        agent_id: Some("tester".to_string()),
                        task: "test task".to_string(),
                        timeout_secs: Some(10),
                        parent_run_id: Some("parent-1".to_string()),
                        ..ContractRunSpawnRequest::default()
                    })
                    .expect("spawn should succeed")
                    .id;

                let tool = WaitSubagentsTool::new(manager);
                let result = tool
                    .execute(json!({"task_ids": [task_id], "parent_run_id": "parent-2", "timeout_secs": 1}))
                    .await
                    .unwrap();
                assert!(result.success);
                let results = result.result["results"].as_array().unwrap();
                assert_eq!(results[0]["status"], "not_found");
            }
        }
    }

    // Built-in tool implementations.

    // Shared utilities

    // Migrated from agent

    // Migrated from runtime (tool_registry inline tools)

    // Search tools

    // Batch tool

    // Migrated from runtime

    // Re-export edit tools
    pub use edit::EditTool;
    pub use multiedit::MultiEditTool;

    // Re-export original 7
    pub use bash::{BashInput, BashOutput, BashTool};
    pub use file::{FileAction, FileTool};
    pub use skrun::RunSkillTool;

    // Re-export migrated tools
    pub use agent_crud::AgentCrudTool;
    pub use config::ConfigTool;
    pub use patch::PatchTool;
    pub use reply::ReplyTool;
    pub use secrets::{SecretGetPolicy, SecretsTool};
    pub use session::SessionTool;
    pub use skill::SkillTool;
    pub use switch_model::SwitchModelTool;

    // Re-export tool_registry inline migrated tools
    pub use manage_ops::ManageOpsTool;

    // Re-export search tools
    pub use glob_tool::GlobTool;
    pub use grep_tool::GrepTool;

    // Re-export batch tool
    pub use batch::BatchTool;

    // Re-export core-migrated tools
    pub use list_subagents::ListSubagentsTool;
    pub use load_skill::LoadSkillTool;
    pub use registry_builder::{
        BashConfig, FileConfig, SecretsConfig, ToolRegistryBuilder, default_registry,
    };
    pub use spawn_subagent::SpawnSubagentTool;
    pub use spawn_subagent_batch::SpawnSubagentBatchTool;
    pub use wait_subagents::WaitSubagentsTool;
}

// Re-export core types from types at crate root}
pub use types::error::{Result, ToolError};
pub use types::tool::{
    SecretResolver, Tool, ToolErrorCategory, ToolOutput, ToolSchema, check_security,
};
pub use types::toolset::ToolRegistry;
pub use types::toolset::{RateLimitWrapper, TimeoutWrapper, ToolWrapper, WrappedTool};
pub use types::toolset::{Toolset, ToolsetContext};

// Re-export security types from types
pub use types::network::{
    NetworkAllowlist, NetworkEcosystem, resolve_and_validate_url, validate_url,
};
pub use types::tool::{SecurityDecision, SecurityGate, ToolAction};

// Store traits are defined in types::store.
// Consumers should import them directly from types.

// Re-export core tool implementations.
pub use impls::{BashTool, FileTool};

// Re-export edit tools
pub use impls::{EditTool, MultiEditTool};

// Re-export migrated tool implementations
pub use impls::{
    AgentCrudTool, ConfigTool, PatchTool, ReplyTool, SecretGetPolicy, SecretsTool, SessionTool,
    SkillTool, SwitchModelTool,
};

// Re-export tool_registry inline migrated tools
pub use impls::ManageOpsTool;

// Re-export search tools
pub use impls::{GlobTool, GrepTool};

// Re-export batch tool
pub use impls::BatchTool;

// Re-export core-migrated tools
pub use impls::{
    BashConfig, FileConfig, ListSubagentsTool, LoadSkillTool, RunSkillTool, SecretsConfig,
    SpawnSubagentBatchTool, SpawnSubagentTool, ToolRegistryBuilder, WaitSubagentsTool,
    default_registry,
};

// Re-export skill types from types
pub use types::skill::{SkillContent, SkillInfo, SkillProvider};

/// Bash command security configuration.
#[derive(Debug, Clone)]
pub struct BashSecurityConfig {
    pub blocked_commands: Vec<String>,
    pub allow_sudo: bool,
}

impl Default for BashSecurityConfig {
    fn default() -> Self {
        Self {
            blocked_commands: vec![
                "rm -rf /".to_string(),
                "mkfs".to_string(),
                "dd if=/dev".to_string(),
                ":(){ :|:& };:".to_string(),
                "chmod -R 777 /".to_string(),
                "chown -R".to_string(),
                "> /dev/sda".to_string(),
                "shutdown".to_string(),
                "reboot".to_string(),
                "init 0".to_string(),
                "halt".to_string(),
            ],
            allow_sudo: false,
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn crate_root_exports_core_tool_surface() {
        let _ = std::mem::size_of::<super::BashTool>();
    }
}
