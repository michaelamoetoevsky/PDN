//! PRoot Core - Rust Implementation
//! 
//! A ptrace-based chroot, mount --bind, and binfmt_misc implementation
//! without privilege/setup for Linux. Compatible with Termux Android 16+.

pub mod arch;
pub mod ptrace;
pub mod syscall;
pub mod path;
pub mod tracee;
pub mod extension;
pub mod cli;
pub mod seccomp;
pub mod tracee_state;
pub mod vfs;
pub mod compat;

pub use arch::{Arch, get_arch, sysnum};
pub use ptrace::{PtraceController, PtraceEvent};
pub use syscall::{SyscallHandler, SyscallConfig};
pub use path::{PathTranslator};
pub use tracee::{Tracee, TraceeId, TraceeState, Registers, SyscallInfo, PathInfo, MountInfo, VfsContext, FdTable, FdEntry, RawSyscall};
pub use extension::{Extension, ExtensionManager};
pub use cli::ProotConfig;
pub use seccomp::SeccompFilter;
pub use tracee_state::TraceeStateManager;
pub use vfs::VfsManager;

use anyhow::Result;

/// Main PRoot engine
pub struct ProotEngine {
    config: ProotConfig,
    ptrace_ctrl: PtraceController,
    syscall_handler: SyscallHandler,
    path_translator: PathTranslator,
    tracee_manager: TraceeStateManager,
    vfs_manager: VfsManager,
    seccomp_filter: SeccompFilter,
}

impl ProotEngine {
    pub fn new(config: ProotConfig) -> Result<Self> {
        let ptrace_ctrl = PtraceController::new()?;
        let syscall_config = SyscallConfig::default();
        let syscall_handler = SyscallHandler::new(syscall_config)?;
        let path_translator = PathTranslator::new(&config)?;
        let tracee_manager = TraceeStateManager::new();
        let vfs_manager = VfsManager::new(&config)?;
        let seccomp_filter = SeccompFilter::new(&config)?;

        Ok(Self {
            config,
            ptrace_ctrl,
            syscall_handler,
            path_translator,
            tracee_manager,
            vfs_manager,
            seccomp_filter,
        })
    }

    pub fn run(&mut self) -> Result<()> {
        // Initialize seccomp filter
        self.seccomp_filter.install()?;

        // Start ptrace loop
        self.ptrace_ctrl.run_loop(&mut self.syscall_handler, 
                                   &mut self.path_translator,
                                   &mut self.tracee_manager,
                                   &mut self.vfs_manager)
    }

    pub fn shutdown(&mut self) -> Result<()> {
        self.ptrace_ctrl.shutdown()
    }
}

/// Initialize PRoot core with default configuration
pub fn init() -> Result<ProotEngine> {
    let config = ProotConfig::default();
    ProotEngine::new(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init() {
        let engine = init();
        assert!(engine.is_ok());
    }
}