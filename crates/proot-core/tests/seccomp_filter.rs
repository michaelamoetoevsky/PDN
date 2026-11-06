//! WORKLIST A21: `install()` must install a filter that denies syscalls, or the
//! caller must be told it did not.
//!
//! The old `install()` set `PR_SET_NO_NEW_PRIVS` and then did nothing, so a
//! caller held a security boundary that did not exist. This observes a real
//! denial instead of trusting a return value: the filter is irreversible and
//! inherited by children, so it is installed in a forked child, and the child
//! reports through its exit code whether a denied syscall was denied with `EPERM`
//! and an allowed one still worked.
//!
//! The probe denies `getpid`. That is deliberate: `open_by_handle_at` and
//! `kexec_load` already fail with `EPERM` without privilege, so their `EPERM`
//! proves nothing about the filter; `getpid` can never fail unfiltered.

use proot_core::{ProotConfig, SeccompFilter};

#[test]
fn install_denies_a_denied_syscall_and_allows_an_allowed_one() {
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        println!("FIXTURE-UNAVAILABLE: fork() failed, so the filter cannot be isolated in a child");
        return;
    }

    if pid == 0 {
        // Child: install the real filter, then probe two syscalls. Nothing here
        // may allocate or panic after the filter is live, so failures are exit
        // codes and the calls are raw `libc`.
        let config = ProotConfig {
            seccomp: true,
            ..Default::default()
        };
        let installed = match SeccompFilter::new(&config) {
            Ok(mut filter) => {
                filter.add_denied_syscall(libc::SYS_getpid);
                filter.install().is_ok()
            }
            Err(_) => false,
        };
        if !installed {
            unsafe { libc::_exit(3) };
        }

        let denied = unsafe { libc::syscall(libc::SYS_getpid) };
        let denied_errno = std::io::Error::last_os_error().raw_os_error();
        let denied_ok = denied == -1 && denied_errno == Some(libc::EPERM);

        // An allowed syscall must keep working.
        let allowed_ok = unsafe { libc::syscall(libc::SYS_getuid) } >= 0;

        unsafe { libc::_exit(if denied_ok && allowed_ok { 0 } else { 17 }) };
    }

    let mut status: libc::c_int = 0;
    let waited = unsafe { libc::waitpid(pid, &mut status, 0) };
    assert_eq!(waited, pid, "waitpid returned {waited}, expected {pid}");

    let exited = libc::WIFEXITED(status);
    let code = if exited {
        libc::WEXITSTATUS(status)
    } else {
        -1
    };
    assert_eq!(
        (exited, code),
        (true, 0),
        "the installed filter did not deny getpid with EPERM (child status {status}, code {code}: \
         3 = install failed, 17 = no denial observed). install() must install a real filter or \
         report that it did not"
    );
}
