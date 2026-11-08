//! p4 step 2: the syscall number must reach the handler table, and a registered
//! handler must observably run.
//!
//! Before the two commits behind this test, `Tracee::handle_syscall_entry` took `_handler` -
//! deliberately unused - and `SyscallHandler`'s table was private with no way to add an
//! entry, so nothing could select one of the 150 built-in registrations and no test could
//! see a handler run at all.
//!
//! Two directions, one assertion each, and they are deliberately different questions:
//!   * registered for THIS number  -> the closure runs;
//!   * registered for ANOTHER number -> the closure does NOT run, while `syscall_info` is
//!     still populated. That second half matters: it separates "the number was read" from
//!     "the handler was dispatched", so a broken dispatch cannot hide behind a working read.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use nix::sys::ptrace::{self, Options};
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{fork, ForkResult};
use proot_core::arch::Arch;
use proot_core::cli::ProotConfig;
use proot_core::path::PathTranslator;
use proot_core::syscall::{SyscallConfig, SyscallHandler};
use proot_core::tracee::Tracee;
use proot_core::tracee_state::TraceeStateManager;
use proot_core::vfs::VfsManager;

/// Stop a child at a known syscall, handing the stopped pid to `inspect` once per syscall
/// stop until the child exits. The child makes exactly one observable syscall (`getpid`),
/// called through libc so it cannot be answered from a cached value.
fn with_child_stopped_at_getpid<F>(mut inspect: F)
where
    F: FnMut(&mut Tracee),
{
    match unsafe { fork() }.expect("fork") {
        ForkResult::Child => {
            ptrace::traceme().expect("TRACEME");
            unsafe { nix::libc::raise(nix::libc::SIGSTOP) };
            let _pid = unsafe { nix::libc::syscall(nix::libc::SYS_getpid) };
            unsafe { nix::libc::_exit(0) };
        }
        ForkResult::Parent { child } => {
            assert!(
                matches!(waitpid(child, None).expect("wait"), WaitStatus::Stopped(..)),
                "the child should stop once before its syscall"
            );
            ptrace::setoptions(child, Options::PTRACE_O_TRACESYSGOOD).expect("setoptions");

            let mut tracee = Tracee::new(child, Arch::from_host());
            for _ in 0..60 {
                ptrace::syscall(child, None).expect("PTRACE_SYSCALL");
                match waitpid(child, None).expect("wait") {
                    WaitStatus::PtraceSyscall(_) => {
                        tracee.observe_syscall().expect("read the syscall");
                        inspect(&mut tracee);
                        if tracee
                            .syscall_info
                            .as_ref()
                            .is_some_and(|info| info.number == nix::libc::SYS_getpid as i64)
                        {
                            break;
                        }
                    }
                    WaitStatus::Exited(..) => break,
                    _ => {}
                }
            }
            let _ = ptrace::kill(child);
        }
    }
}

#[test]
fn a_handler_registered_for_the_number_runs() {
    let ran = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&ran);
    let cfg = ProotConfig::default();
    let mut handler = SyscallHandler::new(SyscallConfig::default()).expect("handler");
    handler.register(
        nix::libc::SYS_getpid as i64,
        Box::new(move |_state, _info, _path, _mgr, _vfs| {
            flag.store(true, Ordering::SeqCst);
            Ok(())
        }),
    );
    let mut translator = PathTranslator::new(&cfg).expect("translator");
    let mut mgr = TraceeStateManager::new();
    let mut vfs = VfsManager::new(&cfg).expect("vfs");

    with_child_stopped_at_getpid(|tracee| {
        tracee
            .handle_syscall_entry(&mut handler, &mut translator, &mut mgr, &mut vfs)
            .expect("handle_syscall_entry");
    });

    assert!(
        ran.load(Ordering::SeqCst),
        "the closure registered for SYS_getpid must run when the tracee makes that syscall"
    );
}

#[test]
fn a_handler_registered_for_another_number_does_not_run() {
    let ran = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&ran);
    let cfg = ProotConfig::default();
    let mut handler = SyscallHandler::new(SyscallConfig::default()).expect("handler");
    // Deliberately a number the child never makes. This is the negative control: the wiring
    // is intact, the read is intact, and only the dispatch key differs.
    handler.register(
        nix::libc::SYS_getppid as i64,
        Box::new(move |_state, _info, _path, _mgr, _vfs| {
            flag.store(true, Ordering::SeqCst);
            Ok(())
        }),
    );
    let mut translator = PathTranslator::new(&cfg).expect("translator");
    let mut mgr = TraceeStateManager::new();
    let mut vfs = VfsManager::new(&cfg).expect("vfs");

    let mut saw_number = None;
    with_child_stopped_at_getpid(|tracee| {
        tracee
            .handle_syscall_entry(&mut handler, &mut translator, &mut mgr, &mut vfs)
            .expect("handle_syscall_entry");
        if let Some(info) = tracee.syscall_info.as_ref() {
            saw_number = Some(info.number);
        }
    });

    assert!(
        !ran.load(Ordering::SeqCst),
        "a handler registered for a different number must not run for SYS_getpid"
    );
    assert_eq!(
        saw_number,
        Some(nix::libc::SYS_getpid as i64),
        "the number must still have been read and recorded - this is what separates a \
         missing dispatch from a missing read"
    );
}
