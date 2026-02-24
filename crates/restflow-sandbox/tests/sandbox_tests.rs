//! Integration tests for restflow-sandbox.

use restflow_sandbox::{SandboxPolicy, pre_exec_hook, wrap_command};
use std::process::Command;

// ─── Cross-platform tests ───────────────────────────────────────────────

#[test]
fn test_sandbox_none_allows_everything() {
    let (prog, args) = wrap_command(&SandboxPolicy::None, "echo", &["hello"]).unwrap();
    assert_eq!(prog, "echo");
    assert_eq!(args, vec!["hello"]);

    let output = Command::new(&prog).args(&args).output().unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("hello"));
}

#[test]
fn test_sandbox_api_compiles() {
    let policies = vec![
        SandboxPolicy::None,
        SandboxPolicy::ReadOnly,
        SandboxPolicy::WriteDir {
            writable_dirs: vec!["/tmp".into()],
        },
    ];

    for policy in &policies {
        let result = wrap_command(policy, "true", &[]);
        assert!(result.is_ok(), "wrap_command failed for {policy:?}");

        let result = pre_exec_hook(policy);
        assert!(result.is_ok(), "pre_exec_hook failed for {policy:?}");
    }
}

#[test]
fn test_policy_display() {
    let policy = SandboxPolicy::ReadOnly;
    let debug_str = format!("{policy:?}");
    assert!(debug_str.contains("ReadOnly"));

    let policy = SandboxPolicy::WriteDir {
        writable_dirs: vec!["/tmp/test".into()],
    };
    let debug_str = format!("{policy:?}");
    assert!(debug_str.contains("WriteDir"));
    assert!(debug_str.contains("/tmp/test"));
}

// ─── macOS-specific tests ───────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod macos_tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_readonly_blocks_write() {
        let tmp = TempDir::new().unwrap();
        let test_file = tmp.path().join("should_not_exist.txt");

        let (prog, args) = wrap_command(
            &SandboxPolicy::ReadOnly,
            "sh",
            &["-c", &format!("touch {}", test_file.display())],
        )
        .unwrap();

        let output = Command::new(&prog).args(&args).output().unwrap();
        assert!(
            !output.status.success(),
            "touch should fail under ReadOnly sandbox"
        );
        assert!(
            !test_file.exists(),
            "file should not be created under ReadOnly sandbox"
        );
    }

    #[test]
    fn test_readonly_allows_read() {
        let (prog, args) =
            wrap_command(&SandboxPolicy::ReadOnly, "sh", &["-c", "cat /etc/hosts"]).unwrap();

        let output = Command::new(&prog).args(&args).output().unwrap();
        assert!(
            output.status.success(),
            "reading /etc/hosts should succeed under ReadOnly sandbox, stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("localhost"),
            "/etc/hosts should contain 'localhost'"
        );
    }

    #[test]
    fn test_writedir_allows_specified_dir() {
        let writable_tmp = TempDir::new().unwrap();
        let blocked_tmp = TempDir::new().unwrap();

        let allowed_file = writable_tmp.path().join("allowed.txt");
        let blocked_file = blocked_tmp.path().join("blocked.txt");

        // Test: writing to the allowed directory should succeed.
        let policy = SandboxPolicy::WriteDir {
            writable_dirs: vec![writable_tmp.path().to_path_buf()],
        };
        let (prog, args) = wrap_command(
            &policy,
            "sh",
            &["-c", &format!("echo ok > {}", allowed_file.display())],
        )
        .unwrap();

        let output = Command::new(&prog).args(&args).output().unwrap();
        assert!(
            output.status.success(),
            "writing to allowed dir should succeed, stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(allowed_file.exists(), "allowed file should be created");

        // Test: writing outside the allowed directory should fail.
        let policy2 = SandboxPolicy::WriteDir {
            writable_dirs: vec![writable_tmp.path().to_path_buf()],
        };
        let (prog2, args2) = wrap_command(
            &policy2,
            "sh",
            &["-c", &format!("echo fail > {}", blocked_file.display())],
        )
        .unwrap();

        let output2 = Command::new(&prog2).args(&args2).output().unwrap();
        assert!(
            !output2.status.success(),
            "writing to non-allowed dir should fail"
        );
        assert!(!blocked_file.exists(), "blocked file should not be created");
    }
}

// ─── Linux-specific tests ───────────────────────────────────────────────

#[cfg(target_os = "linux")]
mod linux_tests {
    use super::*;
    use std::os::unix::process::CommandExt;
    use tempfile::TempDir;

    #[test]
    fn test_readonly_blocks_write() {
        let tmp = TempDir::new().unwrap();
        let test_file = tmp.path().join("should_not_exist.txt");
        let test_file_str = test_file.display().to_string();

        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(format!("touch {test_file_str}"));
        unsafe {
            cmd.pre_exec(|| {
                pre_exec_hook(&SandboxPolicy::ReadOnly)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
            });
        }

        let output = cmd.output().expect("failed to run sandboxed command");
        assert!(
            !output.status.success(),
            "touch should fail under ReadOnly sandbox"
        );
    }

    #[test]
    fn test_readonly_blocks_network() {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(
            "python3 -c \"import socket; socket.socket(socket.AF_INET, socket.SOCK_STREAM)\" 2>&1 || true",
        );
        unsafe {
            cmd.pre_exec(|| {
                pre_exec_hook(&SandboxPolicy::ReadOnly)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
            });
        }

        let output = cmd.output().expect("failed to run sandboxed command");
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            combined.contains("Operation not permitted")
                || combined.contains("EPERM")
                || !output.status.success(),
            "network socket creation should be blocked, got: {combined}"
        );
    }

    #[test]
    fn test_writedir_allows_specified_dir() {
        let writable_tmp = TempDir::new().unwrap();
        let allowed_file = writable_tmp.path().join("allowed.txt");
        let allowed_file_str = allowed_file.display().to_string();
        let writable_dir = writable_tmp.path().to_path_buf();

        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(format!("echo ok > {allowed_file_str}"));
        unsafe {
            cmd.pre_exec(move || {
                let policy = SandboxPolicy::WriteDir {
                    writable_dirs: vec![writable_dir.clone()],
                };
                pre_exec_hook(&policy)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
            });
        }

        let output = cmd.output().expect("failed to run sandboxed command");
        assert!(
            output.status.success(),
            "writing to allowed dir should succeed, stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
