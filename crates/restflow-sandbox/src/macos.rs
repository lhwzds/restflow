//! macOS sandbox using Seatbelt (`sandbox-exec`).
//!
//! Wraps the original command as:
//! `/usr/bin/sandbox-exec -p <profile> -- <program> <args...>`

use crate::SandboxError;
use crate::SandboxPolicy;

const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

pub(crate) fn wrap_command_macos(
    policy: &SandboxPolicy,
    program: &str,
    args: &[&str],
) -> Result<(String, Vec<String>), SandboxError> {
    let profile = generate_seatbelt_profile(policy)?;

    let mut new_args = vec![
        "-p".to_string(),
        profile,
        "--".to_string(),
        program.to_string(),
    ];
    new_args.extend(args.iter().map(|s| s.to_string()));

    Ok((SANDBOX_EXEC.to_string(), new_args))
}

fn generate_seatbelt_profile(policy: &SandboxPolicy) -> Result<String, SandboxError> {
    let mut profile = String::from("(version 1)\n(deny default)\n");

    // Allow basic process operations.
    profile.push_str("(allow process-exec process-fork)\n");
    profile.push_str("(allow signal)\n");
    profile.push_str("(allow sysctl-read)\n");
    profile.push_str("(allow mach-lookup)\n");

    match policy {
        SandboxPolicy::None => unreachable!("None policy filtered before reaching here"),
        SandboxPolicy::ReadOnly => {
            profile.push_str("(allow file-read*)\n");
            profile.push_str("(deny network*)\n");
        }
        SandboxPolicy::WriteDir { writable_dirs } => {
            profile.push_str("(allow file-read*)\n");
            for dir in writable_dirs {
                // Canonicalize to resolve symlinks like /var -> /private/var on macOS.
                let canonical = dir.canonicalize().unwrap_or_else(|_| dir.clone());
                let dir_str = canonical.to_string_lossy();
                profile.push_str(&format!("(allow file-write* (subpath \"{dir_str}\"))\n"));
            }
            profile.push_str("(deny network*)\n");
        }
    }

    Ok(profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_readonly_profile() {
        let profile = generate_seatbelt_profile(&SandboxPolicy::ReadOnly).unwrap();
        assert!(profile.contains("(deny default)"));
        assert!(profile.contains("(allow file-read*)"));
        assert!(profile.contains("(deny network*)"));
        assert!(!profile.contains("file-write*"));
    }

    #[test]
    fn test_generate_writedir_profile() {
        let policy = SandboxPolicy::WriteDir {
            writable_dirs: vec!["/tmp/sandbox-test".into()],
        };
        let profile = generate_seatbelt_profile(&policy).unwrap();
        assert!(profile.contains("(allow file-read*)"));
        assert!(profile.contains("(allow file-write* (subpath \"/tmp/sandbox-test\"))"));
        assert!(profile.contains("(deny network*)"));
    }

    #[test]
    fn test_wrap_command_readonly() {
        let (prog, args) =
            wrap_command_macos(&SandboxPolicy::ReadOnly, "sh", &["-c", "echo hello"]).unwrap();

        assert_eq!(prog, SANDBOX_EXEC);
        assert_eq!(args[0], "-p");
        // args[1] is the profile
        assert_eq!(args[2], "--");
        assert_eq!(args[3], "sh");
        assert_eq!(args[4], "-c");
        assert_eq!(args[5], "echo hello");
    }
}
