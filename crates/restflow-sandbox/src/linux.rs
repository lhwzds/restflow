//! Linux sandbox using Landlock (filesystem) and seccomp (network).
//!
//! Called from a `pre_exec` hook so only the child process is affected.

use crate::SandboxError;
use crate::SandboxPolicy;

use landlock::{
    ABI, AccessFs, CompatLevel, Compatible, Ruleset, RulesetAttr, RulesetCreatedAttr,
    RulesetStatus, path_beneath_rules,
};

/// Apply Landlock + seccomp restrictions inside the child process.
///
/// Must be called inside a `pre_exec` closure.
pub(crate) fn pre_exec_hook_linux(policy: &SandboxPolicy) -> Result<(), SandboxError> {
    set_no_new_privs()?;
    apply_landlock(policy)?;
    apply_network_seccomp(policy)?;
    Ok(())
}

/// Enable PR_SET_NO_NEW_PRIVS (required for seccomp and landlock).
fn set_no_new_privs() -> Result<(), SandboxError> {
    let ret = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
    if ret != 0 {
        return Err(SandboxError::Setup(std::io::Error::last_os_error()));
    }
    Ok(())
}

/// Apply Landlock filesystem restrictions.
fn apply_landlock(policy: &SandboxPolicy) -> Result<(), SandboxError> {
    let abi = ABI::V1;
    let access_rw = AccessFs::from_all(abi);
    let access_ro = AccessFs::from_read(abi);

    let mut ruleset = Ruleset::default()
        .set_compatibility(CompatLevel::BestEffort)
        .handle_access(access_rw)
        .map_err(ll_err)?
        .create()
        .map_err(ll_err)?
        .add_rules(path_beneath_rules(&["/"], access_ro))
        .map_err(ll_err)?
        .add_rules(path_beneath_rules(&["/dev/null"], access_rw))
        .map_err(ll_err)?;

    if let SandboxPolicy::WriteDir { writable_dirs } = policy {
        for dir in writable_dirs {
            ruleset = ruleset
                .add_rules(path_beneath_rules(&[dir.as_path()], access_rw))
                .map_err(ll_err)?;
        }
    }

    let status = ruleset.restrict_self().map_err(ll_err)?;
    if status.ruleset == RulesetStatus::NotEnforced {
        tracing::warn!("Landlock rules were not enforced (kernel may be too old)");
    }

    Ok(())
}

/// Install a seccomp BPF filter to block non-AF_UNIX socket creation.
fn apply_network_seccomp(policy: &SandboxPolicy) -> Result<(), SandboxError> {
    if matches!(policy, SandboxPolicy::None) {
        return Ok(());
    }

    let arch: u32 = if cfg!(target_arch = "x86_64") {
        libc::AUDIT_ARCH_X86_64
    } else if cfg!(target_arch = "aarch64") {
        libc::AUDIT_ARCH_AARCH64
    } else {
        tracing::warn!("Seccomp network filter not supported on this architecture");
        return Ok(());
    };

    let sys_socket: u32 = if cfg!(target_arch = "x86_64") {
        41
    } else if cfg!(target_arch = "aarch64") {
        198
    } else {
        return Ok(());
    };

    let af_unix = libc::AF_UNIX as u32;

    // BPF constants
    const BPF_LD: u16 = 0x00;
    const BPF_W: u16 = 0x00;
    const BPF_ABS: u16 = 0x20;
    const BPF_JMP: u16 = 0x05;
    const BPF_JEQ: u16 = 0x10;
    const BPF_RET: u16 = 0x06;
    const BPF_K: u16 = 0x00;
    const SECCOMP_RET_ALLOW: u32 = 0x7fff_0000;
    const SECCOMP_RET_ERRNO: u32 = 0x0005_0000;

    // seccomp_data offsets
    const OFFSET_ARCH: u32 = 4;
    const OFFSET_NR: u32 = 0;
    const OFFSET_ARGS_0: u32 = 16;

    #[repr(C)]
    struct SockFilter {
        code: u16,
        jt: u8,
        jf: u8,
        k: u32,
    }

    #[repr(C)]
    struct SockFprog {
        len: u16,
        filter: *const SockFilter,
    }

    let filter: Vec<SockFilter> = vec![
        // [0] Load arch
        SockFilter {
            code: BPF_LD | BPF_W | BPF_ABS,
            jt: 0,
            jf: 0,
            k: OFFSET_ARCH,
        },
        // [1] If arch != expected -> allow (skip filter)
        SockFilter {
            code: BPF_JMP | BPF_JEQ | BPF_K,
            jt: 0,
            jf: 5,
            k: arch,
        },
        // [2] Load syscall number
        SockFilter {
            code: BPF_LD | BPF_W | BPF_ABS,
            jt: 0,
            jf: 0,
            k: OFFSET_NR,
        },
        // [3] If syscall != socket -> allow
        SockFilter {
            code: BPF_JMP | BPF_JEQ | BPF_K,
            jt: 0,
            jf: 3,
            k: sys_socket,
        },
        // [4] Load first argument (domain)
        SockFilter {
            code: BPF_LD | BPF_W | BPF_ABS,
            jt: 0,
            jf: 0,
            k: OFFSET_ARGS_0,
        },
        // [5] If domain == AF_UNIX -> allow
        SockFilter {
            code: BPF_JMP | BPF_JEQ | BPF_K,
            jt: 1,
            jf: 0,
            k: af_unix,
        },
        // [6] Return EPERM
        SockFilter {
            code: BPF_RET | BPF_K,
            jt: 0,
            jf: 0,
            k: SECCOMP_RET_ERRNO | (libc::EPERM as u32),
        },
        // [7] Allow
        SockFilter {
            code: BPF_RET | BPF_K,
            jt: 0,
            jf: 0,
            k: SECCOMP_RET_ALLOW,
        },
    ];

    let prog = SockFprog {
        len: filter.len() as u16,
        filter: filter.as_ptr(),
    };

    let ret = unsafe {
        libc::prctl(
            libc::PR_SET_SECCOMP,
            libc::SECCOMP_MODE_FILTER,
            &prog as *const SockFprog,
        )
    };

    if ret != 0 {
        return Err(SandboxError::Setup(std::io::Error::last_os_error()));
    }

    Ok(())
}

fn ll_err(e: impl std::fmt::Display) -> SandboxError {
    SandboxError::ProfileGeneration(e.to_string())
}
