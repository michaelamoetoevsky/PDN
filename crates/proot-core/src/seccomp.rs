//! Seccomp filter for PRoot.
//!
//! This installs a real seccomp BPF filter. The previous `install()` set
//! `PR_SET_NO_NEW_PRIVS` and then did nothing, so callers held a security
//! boundary that did not exist (WORKLIST A21).
//!
//! The baseline denies syscalls a guest never needs and that are classic
//! container-escape / kernel-loading primitives: `open_by_handle_at` and
//! `kexec_load`. `ptrace`, `process_vm_readv` and `process_vm_writev` are
//! deliberately NOT denied, and that is measured, not assumed. The filter is
//! installed in this process and seccomp filters are inherited across `execve`,
//! so it also governs the `proot` binary this engine spawns, whose whole
//! mechanism is `ptrace(PTRACE_TRACEME)`. On aarch64 Termux:
//!
//! - denying `ptrace` makes proot abort: `proot error: ptrace(TRACEME): Operation
//!   not permitted`;
//! - denying `process_vm_readv` / `process_vm_writev` makes proot's file I/O fail
//!   with EFAULT: `sh: cannot create ...: Bad address`;
//! - denying `open_by_handle_at` and `kexec_load` leaves proot working.
//!
//! Denying a boundary that breaks the tool it protects is not hardening, so the
//! filter keeps the two the product does not need and allows the rest.

use anyhow::Result;

// BPF instruction classes from linux/bpf_common.h and linux/filter.h. libc
// exposes them for gnu targets but not for Android, so they are spelled here.
const BPF_LD: u32 = 0x00;
const BPF_W: u32 = 0x00;
const BPF_ABS: u32 = 0x20;
const BPF_JMP: u32 = 0x05;
const BPF_JEQ: u32 = 0x10;
const BPF_K: u32 = 0x00;
const BPF_RET: u32 = 0x06;

/// Seccomp filter for PRoot
pub struct SeccompFilter {
    enabled: bool,
    denied_syscalls: Vec<i64>,
    allowed_syscalls: Vec<i64>,
}

impl SeccompFilter {
    pub fn new(config: &crate::cli::ProotConfig) -> Result<Self> {
        Ok(Self {
            enabled: config.seccomp,
            // Deny-named-allow-the-rest. `add_denied_syscall` extends the set;
            // `allowed_syscalls` is kept for callers that record an allow-list.
            denied_syscalls: base_denied_syscalls(),
            allowed_syscalls: Vec::new(),
        })
    }

    /// Install the filter into the calling process.
    ///
    /// Sets `PR_SET_NO_NEW_PRIVS` (required to install a filter without
    /// `CAP_SYS_ADMIN`, and hardening in its own right) and then loads a real
    /// BPF program. A failure to load is returned, never swallowed.
    pub fn install(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        unsafe {
            if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
                return Err(anyhow::anyhow!("prctl(PR_SET_NO_NEW_PRIVS) failed"));
            }
        }

        let mut insns = build_filter(&self.denied_syscalls);
        let prog = libc::sock_fprog {
            len: insns.len() as libc::c_ushort,
            filter: insns.as_mut_ptr(),
        };
        let rc = unsafe {
            libc::prctl(
                libc::PR_SET_SECCOMP,
                libc::SECCOMP_MODE_FILTER as libc::c_ulong,
                &prog as *const libc::sock_fprog,
            )
        };
        if rc != 0 {
            return Err(anyhow::anyhow!(
                "prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER) failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }

    pub fn add_allowed_syscall(&mut self, syscall: i64) {
        self.allowed_syscalls.push(syscall);
    }

    pub fn add_denied_syscall(&mut self, syscall: i64) {
        self.denied_syscalls.push(syscall);
    }
}

/// The default deny set, resolved through `libc::SYS_*` so the numbers are right
/// on every architecture instead of aarch64 numbers spelled inline.
fn base_denied_syscalls() -> Vec<i64> {
    [libc::SYS_open_by_handle_at, libc::SYS_kexec_load]
        .into_iter()
        .map(|n| n as i64)
        .collect()
}

/// Build the BPF program: load `seccomp_data.nr`, return `EPERM` for each denied
/// syscall number, and allow everything else.
fn build_filter(denied: &[i64]) -> Vec<libc::sock_filter> {
    let mut insns = Vec::with_capacity(2 + denied.len() * 2);
    // A = seccomp_data.nr (the first field).
    insns.push(bpf_stmt(
        BPF_LD | BPF_W | BPF_ABS,
        std::mem::offset_of!(libc::seccomp_data, nr) as u32,
    ));
    for &nr in denied {
        // if A == nr: fall through to the EPERM return; else skip it.
        insns.push(libc::sock_filter {
            code: (BPF_JMP | BPF_JEQ | BPF_K) as u16,
            jt: 0,
            jf: 1,
            k: nr as u32,
        });
        insns.push(bpf_stmt(
            BPF_RET | BPF_K,
            libc::SECCOMP_RET_ERRNO | (libc::EPERM as u32 & libc::SECCOMP_RET_DATA),
        ));
    }
    insns.push(bpf_stmt(BPF_RET | BPF_K, libc::SECCOMP_RET_ALLOW));
    insns
}

fn bpf_stmt(code: u32, k: u32) -> libc::sock_filter {
    libc::sock_filter {
        code: code as u16,
        jt: 0,
        jf: 0,
        k,
    }
}
