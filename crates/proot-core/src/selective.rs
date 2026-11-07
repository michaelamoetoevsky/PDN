//! Selective seccomp interception backend (S1).
//!
//! The second execution backend: a `SECCOMP_RET_TRACE` filter over a *selected*
//! syscall set ([`selected_syscalls`]) with `SECCOMP_RET_ALLOW` for everything
//! else, and one tracer that reads each stop's cookie through the existing
//! `PtraceEvent::Seccomp` handler (`tracee.rs`). Everything not selected runs
//! native, with no stop — the model the reference implementation uses (shroot's
//! `ADAPT`/`CONTROL` macros, `fd_filter_program.c:66-80`) and the one PRoot
//! cannot be, because PRoot stops on every syscall.
//!
//! Scope, stated plainly: the command is `execve`'d as-is. There is no guest
//! root, no path translation and no bind mounts, so this is the interception
//! mechanism measured end to end, not a drop-in replacement for `proot`.

use anyhow::{anyhow, Result};
use nix::sys::ptrace::{self, Options};
use nix::sys::signal::Signal;
use nix::sys::wait::WaitStatus;
use nix::unistd::Pid;
use std::ffi::CString;
use std::time::{Duration, Instant};

use crate::arch::Arch;
use crate::cli::ProotConfig;
use crate::path::PathTranslator;
use crate::ptrace::PtraceEvent;
use crate::seccomp::{self, COOKIE_MEMORY, COOKIE_PATH, COOKIE_PROCESS};
use crate::syscall::{SyscallConfig, SyscallHandler};
use crate::tracee::Tracee;
use crate::tracee_state::TraceeStateManager;
use crate::vfs::VfsManager;

/// `PTRACE_EVENT_SECCOMP`: the event code in `WaitStatus::PtraceEvent` for a
/// `SECCOMP_RET_TRACE` stop (linux/ptrace.h; the same literal `ptrace.rs` names).
const PTRACE_EVENT_SECCOMP: i32 = 7;

/// The selected set: the syscalls a PDN guest needs adapted, each with the
/// cookie naming its category. Everything a guest does that is not listed runs
/// native. Kept to what PDN already models (`syscall.rs`) — path-based lookups,
/// the memory mappings a translated path needs, and process creation.
///
/// `ptrace`, `process_vm_readv` and `process_vm_writev` are deliberately absent:
/// on the `proot` path they must stay native (`seccomp.rs:8-22`), and this
/// backend inherits that constraint rather than re-testing it.
pub fn selected_syscalls() -> Vec<(i64, u32)> {
    vec![
        (libc::SYS_openat as i64, COOKIE_PATH),
        (libc::SYS_openat2 as i64, COOKIE_PATH),
        (libc::SYS_mmap as i64, COOKIE_MEMORY),
        (libc::SYS_mprotect as i64, COOKIE_MEMORY),
        (libc::SYS_clone as i64, COOKIE_PROCESS),
        (libc::SYS_execve as i64, COOKIE_PROCESS),
    ]
}

/// One run of the backend.
#[derive(Debug, Clone)]
pub struct SelectedRun {
    pub exit_code: i32,
    pub elapsed: Duration,
    /// The category cookie of every intercepted syscall, in order.
    pub intercepts: Vec<u32>,
}

impl SelectedRun {
    /// How many syscalls stopped the tracer.
    pub fn intercepted(&self) -> usize {
        self.intercepts.len()
    }

    /// How many stops carried this category cookie.
    pub fn count(&self, cookie: u32) -> usize {
        self.intercepts.iter().filter(|c| **c == cookie).count()
    }
}

/// Run `body` in a child under the selective filter, with this process as its
/// tracer. Returns when the child exits. `body` runs after the filter is live
/// and must return the child's exit code.
pub fn run_traced<F: FnOnce() -> i32>(selected: &[(i64, u32)], body: F) -> Result<SelectedRun> {
    let selected = selected.to_vec();
    let start = Instant::now();
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        return Err(anyhow!("fork failed: {}", std::io::Error::last_os_error()));
    }
    if pid == 0 {
        child_main(&selected, body);
    }
    parent_loop(Pid::from_raw(pid), start)
}

/// Run `argv` under the selective backend: the child `execve`s it, so the guest
/// is a normal native process that only stops on the selected syscalls.
pub fn run_selected_cmd(argv: &[String], selected: &[(i64, u32)]) -> Result<SelectedRun> {
    let argv = argv.to_vec();
    run_traced(selected, move || {
        let cstrings: Vec<CString> = argv
            .iter()
            .map(|a| CString::new(a.as_str()).unwrap_or_default())
            .collect();
        let mut ptrs: Vec<*const libc::c_char> = cstrings.iter().map(|c| c.as_ptr()).collect();
        ptrs.push(std::ptr::null());
        unsafe { libc::execv(ptrs[0], ptrs.as_ptr()) };
        127
    })
}

/// The child half of [`run_traced`]. Never returns.
fn child_main<F: FnOnce() -> i32>(selected: &[(i64, u32)], body: F) -> ! {
    if unsafe { libc::ptrace(libc::PTRACE_TRACEME, 0, 0, 0) } == -1 {
        unsafe { libc::_exit(126) };
    }
    // Hand control back so the tracer can arm `PTRACE_O_TRACESECCOMP` *before*
    // the filter is live: a `TRACE` decision with no such option fails the
    // selected syscall with `ENOSYS`.
    unsafe { libc::raise(libc::SIGSTOP) };
    if seccomp::install_selected_trace(selected).is_err() {
        unsafe { libc::_exit(125) };
    }
    let code = body();
    unsafe { libc::_exit(code) }
}

/// The tracer half of [`run_traced`]: arm the stop, then serve each
/// `PTRACE_EVENT_SECCOMP` through the existing `Tracee` handler.
fn parent_loop(child: Pid, start: Instant) -> Result<SelectedRun> {
    let mut status: libc::c_int = 0;
    if unsafe { libc::waitpid(child.as_raw(), &mut status, 0) } != child.as_raw() {
        return Err(anyhow!(
            "waitpid for the child's TRACEME stop failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    ptrace::setoptions(
        child,
        Options::PTRACE_O_TRACESECCOMP | Options::PTRACE_O_EXITKILL,
    )?;
    ptrace::cont(child, None)?;

    let mut tracee = Tracee::new(child, Arch::Arm64);
    let mut handler = SyscallHandler::new(SyscallConfig::default())?;
    let mut translator = PathTranslator::new(&ProotConfig::default())?;
    let mut tracee_mgr = TraceeStateManager::new();
    let mut vfs_mgr = VfsManager::new(&ProotConfig::default())?;

    let mut exit_code = 255;
    loop {
        match nix::sys::wait::waitpid(child, None) {
            Ok(WaitStatus::PtraceEvent(pid, _sig, event)) if event == PTRACE_EVENT_SECCOMP => {
                tracee.handle_event(
                    PtraceEvent::Seccomp,
                    &mut handler,
                    &mut translator,
                    &mut tracee_mgr,
                    &mut vfs_mgr,
                )?;
                ptrace::cont(pid, None)?;
            }
            Ok(WaitStatus::PtraceEvent(pid, sig, _)) => {
                ptrace::cont(pid, sig)?;
            }
            Ok(WaitStatus::PtraceSyscall(pid)) => {
                ptrace::cont(pid, None)?;
            }
            Ok(WaitStatus::Stopped(pid, sig)) => {
                // Suppress the exec trap and any group-stop; deliver the rest.
                let deliver = if matches!(sig, Signal::SIGTRAP | Signal::SIGSTOP) {
                    None
                } else {
                    Some(sig)
                };
                ptrace::cont(pid, deliver)?;
            }
            Ok(WaitStatus::Exited(_pid, code)) => {
                exit_code = code;
                break;
            }
            Ok(WaitStatus::Signaled(_pid, sig, _)) => {
                exit_code = 128 + sig as i32;
                break;
            }
            Ok(WaitStatus::Continued(_)) => {}
            Ok(_) => {}
            Err(nix::Error::ECHILD) => break,
            Err(e) => return Err(anyhow!("waitpid failed: {}", e)),
        }
    }

    Ok(SelectedRun {
        exit_code,
        elapsed: start.elapsed(),
        intercepts: tracee.seccomp_intercepts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seccomp::cookie_category;

    #[test]
    fn every_selected_syscall_carries_a_known_category_cookie() {
        for (nr, cookie) in selected_syscalls() {
            assert!(
                cookie_category(cookie).is_some(),
                "syscall {nr} carries an unclassified cookie {cookie:#x}"
            );
        }
    }

    /// The whole point of the backend: a selected syscall stops the tracer, an
    /// unselected one does not. `openat` is selected; `faccessat`, `getpid` and
    /// `close` are not, so a run that only touches those must intercept nothing.
    #[test]
    fn a_selected_syscall_is_intercepted_and_an_unselected_one_runs_native() {
        let selected = selected_syscalls();
        let n = 64usize;
        let path = CString::new("/etc/hostname").unwrap();
        let path_ptr = path.as_ptr();

        let run = run_traced(&selected, move || {
            for _ in 0..n {
                let fd = unsafe { libc::syscall(libc::SYS_openat, libc::AT_FDCWD, path_ptr, 0i64) };
                if fd >= 0 {
                    unsafe { libc::close(fd as i32) };
                }
            }
            for _ in 0..n {
                unsafe { libc::syscall(libc::SYS_faccessat, libc::AT_FDCWD, path_ptr, 0i64) };
                unsafe { libc::syscall(libc::SYS_getpid) };
            }
            0
        })
        .expect("run under the selective filter");

        assert_eq!(run.exit_code, 0, "the child must exit cleanly");
        assert_eq!(
            run.intercepted(),
            n,
            "only the {n} selected openat calls may stop the tracer; faccessat/getpid/close \
             are unselected and must run native"
        );
        assert_eq!(run.count(COOKIE_PATH), n);
    }
}
