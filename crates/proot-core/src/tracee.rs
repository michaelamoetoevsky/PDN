//! Tracee management for PRoot

use crate::arch::Arch;
use crate::path::PathTranslator;
use crate::syscall::SyscallHandler;
use crate::tracee_state::TraceeStateManager;
use crate::vfs::VfsManager;
use anyhow::Result;
use nix::unistd::Pid;

/// Unique tracee identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TraceeId(pub nix::unistd::Pid);

impl From<nix::unistd::Pid> for TraceeId {
    fn from(pid: nix::unistd::Pid) -> Self {
        TraceeId(pid)
    }
}

/// Tracee state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceeState {
    Running,
    Stopped,
    SyscallEntry,
    SyscallExit,
    Signaled,
    Exited,
    Zombie,
}

/// Tracee representation
#[derive(Debug, Clone)]
pub struct Tracee {
    pub pid: nix::unistd::Pid,
    pub state: TraceeState,
    pub registers: Registers,
    pub syscall_info: Option<SyscallInfo>,
    pub path_info: PathInfo,
    pub vfs_context: VfsContext,
    pub arch: Arch,
    pub parent: Option<Pid>,
    pub children: Vec<Pid>,
    pub fd_table: FdTable,
    pub signal_mask: u64,
    pub ptrace_options: u32,
    /// One entry per `SECCOMP_RET_TRACE` stop this tracee took, in order: the
    /// category cookie the filter stamped, read back via `PTRACE_GETEVENTMSG`
    /// (S1). Empty until the selective backend intercepts something.
    pub seccomp_intercepts: Vec<u32>,
}

impl Tracee {
    pub fn new(pid: Pid, arch: Arch) -> Self {
        Self {
            pid,
            state: TraceeState::Running,
            registers: Registers::default(),
            syscall_info: None,
            path_info: PathInfo::default(),
            vfs_context: VfsContext::default(),
            arch,
            parent: None,
            children: Vec::new(),
            fd_table: FdTable::new(),
            signal_mask: 0,
            ptrace_options: 0,
            seccomp_intercepts: Vec::new(),
        }
    }

    pub fn handle_syscall_entry(
        &mut self,
        _handler: &mut crate::syscall::SyscallHandler,
        translator: &mut crate::path::PathTranslator,
        _tracee_mgr: &mut crate::tracee_state::TraceeStateManager,
        _vfs_mgr: &mut crate::vfs::VfsManager,
    ) -> Result<()> {
        let syscall = self.read_syscall()?;
        self.syscall_info = Some(SyscallInfo {
            number: syscall.number,
            args: syscall.args,
            retval: 0,
        });

        translator.translate_syscall_args(
            &mut self.syscall_info.as_mut().unwrap().args,
            &self.path_info,
        )?;

        Ok(())
    }

    pub fn handle_syscall_exit(
        &mut self,
        _handler: &mut crate::syscall::SyscallHandler,
        _translator: &mut crate::path::PathTranslator,
        _tracee_mgr: &mut crate::tracee_state::TraceeStateManager,
        _vfs_mgr: &mut crate::vfs::VfsManager,
    ) -> Result<()> {
        let retval = self.read_retval()?;
        if let Some(syscall_info) = &mut self.syscall_info {
            syscall_info.retval = retval;
        }
        self.syscall_info = None;
        Ok(())
    }

    /// Read the syscall the tracee is stopped on and cache it as `syscall_info`.
    ///
    /// Public so a test can stop a child at a known syscall and assert the *number* the
    /// engine reports (p4 step 1).
    pub fn observe_syscall(&mut self) -> Result<()> {
        let raw = self.read_registers()?;
        self.registers = raw.registers;
        self.syscall_info = Some(SyscallInfo {
            number: raw.number,
            args: raw.args,
            retval: 0,
        });
        Ok(())
    }

    /// The register file as the kernel reports it at this stop.
    ///
    /// Measured 2026-11-08: `read_syscall` and `read_retval` used to return `0` for every
    /// stop, so the 155 handler patterns in `syscall.rs` could only ever be selected by
    /// `0`, and no `PTRACE_GETREGSET` call existed anywhere in the tree. Both now read
    /// through `PTRACE_GETREGSET` with `NT_PRSTATUS`, and a failed read is an error naming
    /// the pid and the errno rather than a silent zero.
    fn read_registers(&self) -> Result<RegisterFile> {
        // `nix 0.27`'s `ptrace::getregs` has no aarch64 implementation (compiling against
        // it fails with "cannot find function `getregs`"), so the call goes through libc
        // directly: PTRACE_GETREGSET with NT_PRSTATUS fills a `user_regs_struct`, which on
        // aarch64 is exactly `Registers`. `libc` arrives via nix rather than as a new
        // dependency.
        let mut raw: nix::libc::user_regs_struct = unsafe { std::mem::zeroed() };
        // NT_PRSTATUS is 1 in the Linux ptrace ABI (`include/uapi/linux/elf.h`:
        // `#define NT_PRSTATUS 1`); libc does not export it for every target, so it is
        // named here rather than spelled 1 at the call site.
        const NT_PRSTATUS: nix::libc::c_uint = 1;
        let mut iov = nix::libc::iovec {
            iov_base: &mut raw as *mut _ as *mut nix::libc::c_void,
            iov_len: std::mem::size_of::<nix::libc::user_regs_struct>(),
        };
        let ret = unsafe {
            nix::libc::ptrace(
                nix::libc::PTRACE_GETREGSET as nix::libc::c_int,
                self.pid.as_raw(),
                NT_PRSTATUS as *mut nix::libc::c_void,
                &mut iov as *mut nix::libc::iovec as *mut nix::libc::c_void,
            )
        };
        if ret < 0 {
            return Err(anyhow::anyhow!(
                "PTRACE_GETREGSET (NT_PRSTATUS) failed for pid {}: {}",
                self.pid,
                std::io::Error::last_os_error()
            ));
        }
        #[cfg(target_arch = "aarch64")]
        {
            // aarch64 reports the syscall number in x8 and the result in x0, and
            // `user_pt_regs` is 31 general registers followed by sp, pc and pstate -
            // exactly the shape of `Registers`.
            let mut args = [0u64; 6];
            args.copy_from_slice(&raw.regs[0..6]);
            Ok(RegisterFile {
                number: raw.regs[8] as i64,
                args,
                retval: raw.regs[0] as isize,
                registers: Registers {
                    regs: raw.regs,
                    pc: raw.pc,
                    sp: raw.sp,
                    pstate: raw.pstate,
                },
            })
        }
        #[cfg(target_arch = "x86_64")]
        {
            Ok(RegisterFile {
                number: raw.orig_rax as i64,
                args: [raw.rdi, raw.rsi, raw.rdx, raw.r10, raw.r8, raw.r9],
                retval: raw.rax as isize,
                // `Registers` is the aarch64 layout, so it stays at its default here
                // rather than being filled with values that mean something else.
                registers: Registers::default(),
            })
        }
        #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
        {
            let _ = raw;
            anyhow::bail!("no syscall register mapping is implemented for this target architecture")
        }
    }

    fn read_syscall(&self) -> Result<RawSyscall> {
        let raw = self.read_registers()?;
        Ok(RawSyscall {
            number: raw.number,
            args: raw.args,
        })
    }

    fn read_retval(&self) -> Result<isize> {
        Ok(self.read_registers()?.retval)
    }

    pub fn handle_event(
        &mut self,
        event: crate::ptrace::PtraceEvent,
        syscall_handler: &mut SyscallHandler,
        path_translator: &mut PathTranslator,
        tracee_manager: &mut TraceeStateManager,
        vfs_manager: &mut VfsManager,
    ) -> Result<()> {
        match event {
            crate::ptrace::PtraceEvent::SyscallEntry => {
                self.handle_syscall_entry(
                    syscall_handler,
                    path_translator,
                    tracee_manager,
                    vfs_manager,
                )?;
            }
            crate::ptrace::PtraceEvent::SyscallExit => {
                self.handle_syscall_exit(
                    syscall_handler,
                    path_translator,
                    tracee_manager,
                    vfs_manager,
                )?;
            }
            crate::ptrace::PtraceEvent::Signal(sig) => {
                self.handle_signal(sig)?;
            }
            crate::ptrace::PtraceEvent::Clone
            | crate::ptrace::PtraceEvent::Fork
            | crate::ptrace::PtraceEvent::Vfork => {
                self.handle_clone()?;
            }
            crate::ptrace::PtraceEvent::Exec => {
                self.handle_exec()?;
            }
            crate::ptrace::PtraceEvent::Seccomp => {
                self.handle_seccomp()?;
            }
            crate::ptrace::PtraceEvent::Exit => {
                self.state = TraceeState::Exited;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_signal(&mut self, _sig: nix::sys::signal::Signal) -> Result<()> {
        self.state = TraceeState::Signaled;
        Ok(())
    }

    fn handle_clone(&mut self) -> Result<()> {
        // New tracee will be picked up by waitpid
        Ok(())
    }

    fn handle_exec(&mut self) -> Result<()> {
        // Reset path info on exec
        self.path_info = PathInfo::default();
        Ok(())
    }

    /// S1: a `SECCOMP_RET_TRACE` stop. The filter stamps a per-category cookie
    /// into the low 16 bits of the return action; the tracer reads it back with
    /// `PTRACE_GETEVENTMSG` and records it. `seccomp::cookie_category` is the
    /// dispatch table a handler routes through, kept pure so it is testable
    /// without a live tracee.
    fn handle_seccomp(&mut self) -> Result<()> {
        self.seccomp_intercepts.push(read_seccomp_cookie(self.pid)?);
        Ok(())
    }
}

/// Read the `SECCOMP_RET_TRACE` cookie from a stopped tracee.
///
/// `PTRACE_GETEVENTMSG` is the kernel's own read-back of the `SECCOMP_RET_DATA`
/// the filter returned; nix 0.27 has no wrapper, so it is called directly. The
/// tracee must be stopped on the `PTRACE_EVENT_SECCOMP` stop when this runs.
fn read_seccomp_cookie(pid: Pid) -> Result<u32> {
    let mut msg: libc::c_ulong = 0;
    let rc = unsafe {
        libc::ptrace(
            libc::PTRACE_GETEVENTMSG,
            pid.as_raw(),
            0,
            &mut msg as *mut libc::c_ulong,
        )
    };
    if rc == -1 {
        return Err(anyhow::anyhow!(
            "PTRACE_GETEVENTMSG failed on pid {}: {}",
            pid,
            std::io::Error::last_os_error()
        ));
    }
    Ok((msg & u32::MAX as libc::c_ulong) as u32)
}

/// Raw syscall data
#[derive(Debug, Clone, Default)]
pub struct RawSyscall {
    pub number: i64,
    pub args: [u64; 6],
}

/// Syscall information
#[derive(Debug, Clone, Default)]
pub struct SyscallInfo {
    pub number: i64,
    pub args: [u64; 6],
    pub retval: isize,
}

/// One syscall as the registers report it, together with the register file itself.
///
/// Returned by `Tracee::read_registers`; kept private because it exists only to carry
/// those four values out of the arch-specific read in one step.
#[derive(Debug, Clone, Default)]
struct RegisterFile {
    number: i64,
    args: [u64; 6],
    retval: isize,
    registers: Registers,
}

/// Register state
#[derive(Debug, Clone, Default)]
pub struct Registers {
    pub regs: [u64; 31],
    pub pc: u64,
    pub sp: u64,
    pub pstate: u64,
}

/// Path information for tracee
#[derive(Debug, Clone, Default)]
pub struct PathInfo {
    pub cwd: String,
    pub root: String,
    pub mounts: Vec<MountInfo>,
}

/// Mount information
#[derive(Debug, Clone, Default)]
pub struct MountInfo {
    pub source: String,
    pub target: String,
    pub fstype: String,
    pub flags: u64,
}

/// VFS context for tracee
#[derive(Debug, Clone, Default)]
pub struct VfsContext {
    pub open_files: Vec<OpenFile>,
    pub mount_namespace: u64,
}

/// Open file entry
#[derive(Debug, Clone, Default)]
pub struct OpenFile {
    pub fd: i32,
    pub path: String,
    pub flags: i32,
    pub mode: u32,
}

/// File descriptor table
#[derive(Debug, Clone, Default)]
pub struct FdTable {
    pub entries: Vec<Option<FdEntry>>,
}

impl FdTable {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn allocate(&mut self, entry: FdEntry) -> i32 {
        for (i, entry_opt) in self.entries.iter_mut().enumerate() {
            if entry_opt.is_none() {
                *entry_opt = Some(entry);
                return i as i32;
            }
        }
        self.entries.push(Some(entry));
        (self.entries.len() - 1) as i32
    }

    pub fn get(&self, fd: i32) -> Option<&FdEntry> {
        self.entries.get(fd as usize).and_then(|e| e.as_ref())
    }

    pub fn get_mut(&mut self, fd: i32) -> Option<&mut FdEntry> {
        self.entries.get_mut(fd as usize).and_then(|e| e.as_mut())
    }

    pub fn close(&mut self, fd: i32) -> Option<FdEntry> {
        if let Some(entry) = self.entries.get_mut(fd as usize) {
            entry.take()
        } else {
            None
        }
    }
}

/// File descriptor entry
#[derive(Debug, Clone, Default)]
pub struct FdEntry {
    pub path: String,
    pub flags: i32,
    pub mode: u32,
    pub offset: u64,
    pub flags_open: i32,
}
