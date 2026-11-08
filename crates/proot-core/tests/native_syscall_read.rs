//! p4 step 1: the native engine must report the syscall its tracee actually made.
//!
//! Before the change behind this test, `Tracee::read_syscall` returned `number: 0` for
//! every stop (and `read_retval` returned `0`), so the 155 handler patterns in
//! `syscall.rs` could only ever be selected by `0`, and no `PTRACE_GETREGSET` call
//! existed anywhere in the tree. This test stops a real child at `getpid` and asserts the
//! number the engine reports is `SYS_getpid` - not merely non-zero, since any constant
//! would satisfy that weaker claim.
//!
//! The negative control is the same test against the stubbed version of `read_syscall`:
//! it fails with `got 0` (run it with `crates/proot-core/src/tracee.rs` taken from the
//! parent commit to see the red).

use nix::sys::ptrace::{self, Options};
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{fork, ForkResult};
use proot_core::arch::Arch;
use proot_core::tracee::Tracee;

#[test]
fn the_engine_reports_the_syscall_the_tracee_made() {
    match unsafe { fork() }.expect("fork") {
        ForkResult::Child => {
            ptrace::traceme().expect("TRACEME");
            // Stop once, so the tracer can set its options before the observed syscall.
            unsafe { nix::libc::raise(nix::libc::SIGSTOP) };
            // The one syscall under observation, called through libc's syscall() so it
            // cannot be answered from a value libc cached earlier.
            let _pid = unsafe { nix::libc::syscall(nix::libc::SYS_getpid) };
            unsafe { nix::libc::_exit(0) };
        }
        ForkResult::Parent { child } => {
            // The initial stop from TRACEME plus SIGSTOP.
            assert!(
                matches!(waitpid(child, None).expect("wait"), WaitStatus::Stopped(..)),
                "the child should stop once before its syscall"
            );
            ptrace::setoptions(child, Options::PTRACE_O_TRACESYSGOOD).expect("setoptions");

            let mut tracee = Tracee::new(child, Arch::from_host());
            let mut observed: Option<i64> = None;
            // Bounded: every iteration advances the child by one syscall stop, and the
            // assertion below is what fails when the read is a stub.
            for _ in 0..60 {
                ptrace::syscall(child, None).expect("PTRACE_SYSCALL");
                match waitpid(child, None).expect("wait") {
                    WaitStatus::PtraceSyscall(_) => {
                        tracee.observe_syscall().expect("read the syscall");
                        let number = tracee
                            .syscall_info
                            .as_ref()
                            .expect("observe_syscall records syscall_info")
                            .number;
                        if number == nix::libc::SYS_getpid as i64 {
                            observed = Some(number);
                            break;
                        }
                    }
                    WaitStatus::Exited(..) => break,
                    _ => {}
                }
            }
            let _ = ptrace::kill(child);

            assert_eq!(
                observed,
                Some(nix::libc::SYS_getpid as i64),
                "the engine must report SYS_getpid ({}) for the child's getpid syscall; a \
                 stubbed read reports 0 for every stop",
                nix::libc::SYS_getpid
            );
        }
    }
}
