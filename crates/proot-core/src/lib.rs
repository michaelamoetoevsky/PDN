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
pub use cli::{ProotConfig, BindMount};
pub use seccomp::SeccompFilter;
pub use tracee_state::TraceeStateManager;
pub use vfs::VfsManager;

use anyhow::Result;
use tokio::process::Command;
use std::sync::Arc;
use parking_lot::Mutex;

/// Main PRoot engine
#[derive(Clone)]
pub struct ProotEngine {
    pub config: ProotConfig,
    ptrace_ctrl: Arc<Mutex<PtraceController>>,
    syscall_handler: Arc<Mutex<SyscallHandler>>,
    path_translator: Arc<Mutex<PathTranslator>>,
    tracee_manager: Arc<Mutex<TraceeStateManager>>,
    vfs_manager: Arc<Mutex<VfsManager>>,
    seccomp_filter: Arc<Mutex<SeccompFilter>>,
}

impl ProotEngine {
    pub fn new(config: ProotConfig) -> Result<Self> {
        let ptrace_ctrl = Arc::new(Mutex::new(PtraceController::new()?));
        let syscall_config = SyscallConfig::default();
        let syscall_handler = Arc::new(Mutex::new(SyscallHandler::new(syscall_config)?));
        let path_translator = Arc::new(Mutex::new(PathTranslator::new(&config)?));
        let tracee_manager = Arc::new(Mutex::new(TraceeStateManager::new()));
        let vfs_manager = Arc::new(Mutex::new(VfsManager::new(&config)?));
        let seccomp_filter = Arc::new(Mutex::new(SeccompFilter::new(&config)?));

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
        self.seccomp_filter.lock().install()?;

        // Start ptrace loop
        self.ptrace_ctrl.lock().run_loop(&mut self.syscall_handler.lock(), 
                                   &mut self.path_translator.lock(),
                                   &mut self.tracee_manager.lock(),
                                   &mut self.vfs_manager.lock())
    }

    pub async fn start(&mut self) -> Result<u32> {
        // Initialize seccomp filter
        self.seccomp_filter.lock().install()?;

        // Build the command to execute
        let mut cmd = Command::new(&self.config.command[0]);
        if self.config.command.len() > 1 {
            cmd.args(&self.config.command[1..]);
        }
        
        // Set up environment
        for env in &self.config.env {
            if let Some((k, v)) = env.split_once('=') {
                cmd.env(k, v);
            }
        }
        
        // Set working directory
        if let Some(cwd) = &self.config.cwd {
            cmd.current_dir(cwd);
        }
        
        // Set up bind mounts (would need to be done via ptrace)
        // For now, we'll just spawn the process
        let child = cmd.spawn()?;
        let pid = child.id().ok_or_else(|| anyhow::anyhow!("Failed to get PID"))?;
        
        // In a real implementation, we would ptrace the child process
        // and set up the chroot environment before it runs
        
        Ok(pid)
    }
    
    /// Execute a command in the foreground and wait for completion, returning exit code
    pub async fn execute(&mut self) -> Result<i32> {
        // Initialize seccomp filter
        self.seccomp_filter.lock().install()?;

        // Build the command to execute
        let mut cmd = Command::new(&self.config.command[0]);
        if self.config.command.len() > 1 {
            cmd.args(&self.config.command[1..]);
        }
        
        // Debug: print command
        eprintln!("[DEBUG] Executing command: {:?}", self.config.command);
        eprintln!("[DEBUG] CWD: {:?}", self.config.cwd);
        eprintln!("[DEBUG] Env: {:?}", self.config.env);
        
        // Set up environment
        for env in &self.config.env {
            if let Some((k, v)) = env.split_once('=') {
                cmd.env(k, v);
            }
        }
        
        // Set working directory
        if let Some(cwd) = &self.config.cwd {
            cmd.current_dir(cwd);
        }
        
        // For now, we'll just spawn the process and wait
        let mut child = cmd
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .stdin(std::process::Stdio::inherit())
            .spawn()?;
        let pid = child.id().ok_or_else(|| anyhow::anyhow!("Failed to get PID"))?;
        eprintln!("[DEBUG] Spawned PID: {}", pid);
        
        // Wait for the process to complete and get exit code
        let exit_status = child.wait().await?;
        eprintln!("[DEBUG] Exit status: {:?}", exit_status);
        let exit_code = exit_status.code().unwrap_or(-1);
        eprintln!("[DEBUG] Exit code: {}", exit_code);
        
        Ok(exit_code)
    }

    pub fn shutdown(&mut self) -> Result<()> {
        self.ptrace_ctrl.lock().shutdown()
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
    
    #[test]
    fn test_proot_engine_new() {
        let config = ProotConfig::default();
        let engine = ProotEngine::new(config);
        assert!(engine.is_ok());
    }
    
    #[test]
    fn test_proot_config_default() {
        let config = ProotConfig::default();
        assert!(config.command.is_empty());
        assert!(config.bind.is_empty());
        assert!(config.env.is_empty());
    }
    
    #[test]
    fn test_proot_config_custom() {
        let config = ProotConfig {
            command: vec!["/bin/bash".to_string()],
            bind: vec!["/host:/guest".to_string()],
            env: vec!["TEST=value".to_string()],
            ..Default::default()
        };
        assert_eq!(config.command, vec!["/bin/bash".to_string()]);
        assert_eq!(config.bind, vec!["/host:/guest".to_string()]);
        assert_eq!(config.env, vec!["TEST=value".to_string()]);
    }
}