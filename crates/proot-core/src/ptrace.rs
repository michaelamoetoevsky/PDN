//! Ptrace controller for PRoot

use anyhow::{anyhow, Result};
use libc::{c_int, c_long, c_void, iovec, pid_t, siginfo_t, user_regs_struct};
use libc::{
    ptrace, PTRACE_ATTACH, PTRACE_CONT, PTRACE_DETACH, PTRACE_GETEVENTMSG, PTRACE_GETREGS,
    PTRACE_GETREGSET, PTRACE_GETSIGINFO, PTRACE_KILL, PTRACE_PEEKDATA, PTRACE_PEEKTEXT,
    PTRACE_PEEKUSER, PTRACE_POKEDATA, PTRACE_POKETEXT, PTRACE_POKEUSER, PTRACE_SETOPTIONS,
    PTRACE_SETREGS, PTRACE_SETREGSET, PTRACE_SETSIGINFO, PTRACE_SINGLESTEP, PTRACE_SYSCALL,
    PTRACE_TRACEME,
};
use nix::sys::ptrace::{self, Options};
use nix::sys::signal::Signal;
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::Pid;
use parking_lot::Mutex;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, error, trace, warn};

use crate::arch::{Arch, Arch::Arm64};
use crate::path::PathTranslator;
use crate::syscall::SyscallHandler;
use crate::tracee::{
    MountInfo, PathInfo, RawSyscall, Registers, SyscallInfo, Tracee, TraceeState, VfsContext,
};
use crate::tracee_state::TraceeStateManager;
use crate::vfs::VfsManager;

/// Ptrace event types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PtraceEvent {
    SyscallEntry,
    SyscallExit,
    Signal(Signal),
    Exit,
    Stop,
    Clone,
    Fork,
    Vfork,
    Exec,
    Seccomp,
    Unknown(i32),
}

impl PtraceEvent {
    pub fn from_wait_status(status: WaitStatus) -> Option<Self> {
        match status {
            WaitStatus::PtraceEvent(pid, event, _) => match event as i32 {
                1 => Some(PtraceEvent::Clone),
                2 => Some(PtraceEvent::Fork),
                3 => Some(PtraceEvent::Vfork),
                4 => Some(PtraceEvent::Exec),
                7 => Some(PtraceEvent::Seccomp),
                _ => Some(PtraceEvent::Unknown(event as i32)),
            },
            WaitStatus::PtraceSyscall(pid) => Some(PtraceEvent::SyscallEntry),
            WaitStatus::Signaled(pid, signal, _) => Some(PtraceEvent::Signal(signal)),
            WaitStatus::Exited(pid, code) => Some(PtraceEvent::Exit),
            WaitStatus::Stopped(pid, signal) => Some(PtraceEvent::Stop),
            _ => None,
        }
    }
}

/// Ptrace controller managing all tracees
pub struct PtraceController {
    tracees: Arc<Mutex<HashMap<Pid, Tracee>>>,
    tracee_counter: Arc<Mutex<u64>>,
    options: Options,
}

impl PtraceController {
    pub fn new() -> Result<Self> {
        Ok(Self {
            tracees: Arc::new(Mutex::new(HashMap::new())),
            tracee_counter: Arc::new(Mutex::new(0)),
            options: Options::PTRACE_O_TRACESYSGOOD
                | Options::PTRACE_O_TRACECLONE
                | Options::PTRACE_O_TRACEFORK
                | Options::PTRACE_O_TRACEVFORK
                | Options::PTRACE_O_TRACEEXEC
                | Options::PTRACE_O_TRACESECCOMP
                | Options::PTRACE_O_EXITKILL,
        })
    }

    pub fn add_tracee(&self, pid: Pid, tracee: Tracee) {
        self.tracees.lock().insert(pid, tracee);
    }

    pub fn remove_tracee(&self, pid: Pid) -> Option<Tracee> {
        self.tracees.lock().remove(&pid)
    }

    pub fn get_tracee(&self, pid: Pid) -> Option<Tracee> {
        self.tracees.lock().get(&pid).cloned()
    }

    pub fn get_all_tracees(&self) -> Vec<Tracee> {
        self.tracees.lock().values().cloned().collect()
    }

    pub fn run_loop(
        &self,
        syscall_handler: &mut SyscallHandler,
        path_translator: &mut PathTranslator,
        tracee_manager: &mut TraceeStateManager,
        vfs_manager: &mut VfsManager,
    ) -> Result<()> {
        loop {
            // Wait for any tracee event
            match nix::sys::wait::waitpid(None, None) {
                Ok(status) => {
                    self.handle_event(
                        status,
                        syscall_handler,
                        path_translator,
                        tracee_manager,
                        vfs_manager,
                    )?;
                }
                Err(nix::Error::ECHILD) => {
                    // No more tracees
                    break;
                }
                Err(e) => {
                    return Err(anyhow!("waitpid failed: {}", e));
                }
            }
        }
        Ok(())
    }

    fn handle_event(
        &self,
        status: nix::sys::wait::WaitStatus,
        syscall_handler: &mut SyscallHandler,
        path_translator: &mut PathTranslator,
        tracee_manager: &mut TraceeStateManager,
        vfs_manager: &mut VfsManager,
    ) -> Result<()> {
        let pid = match status {
            nix::sys::wait::WaitStatus::PtraceEvent(pid, _, _) => pid,
            nix::sys::wait::WaitStatus::PtraceSyscall(pid) => pid,
            nix::sys::wait::WaitStatus::Signaled(pid, _, _) => pid,
            nix::sys::wait::WaitStatus::Exited(pid, _) => {
                self.remove_tracee(pid);
                return Ok(());
            }
            nix::sys::wait::WaitStatus::Stopped(pid, _) => pid,
            _ => return Ok(()),
        };

        // Get or create tracee
        let mut tracee = {
            let mut tracees = self.tracees.lock();
            match tracees.entry(pid) {
                Entry::Occupied(e) => e.get().clone(),
                Entry::Vacant(e) => {
                    let tracee = Tracee::new(pid, Arch::Arm64);
                    e.insert(tracee.clone());
                    tracee
                }
            }
        };

        // Handle the event using tracee's method
        if let Some(event) = PtraceEvent::from_wait_status(status) {
            tracee.handle_event(
                event,
                syscall_handler,
                path_translator,
                tracee_manager,
                vfs_manager,
            )?;
        }

        // Update tracee
        {
            let mut tracees = self.tracees.lock();
            tracees.insert(pid, tracee);
        }

        Ok(())
    }

    pub fn shutdown(&self) -> Result<()> {
        let tracees = self.tracees.lock();
        for (pid, _tracee) in tracees.iter() {
            ptrace::detach(*pid, None)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ptrace_controller_creation() {
        let ctrl = PtraceController::new();
        assert!(ctrl.is_ok());
    }
}
