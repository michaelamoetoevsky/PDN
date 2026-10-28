//! PRoot Distro Core - Container management for PRoot

pub mod container;
pub mod image;
pub mod config;
pub mod install;
pub mod login;
pub mod bindings;
pub mod session;
pub mod paths;
pub mod constants;
pub mod message;
pub mod dirfd;
pub mod guestfile;
pub mod sysdata;
pub mod locking;
pub mod names;
pub mod progress;
pub mod compress;
pub mod shm;

pub use container::{Container, ContainerManager, ContainerConfig, ContainerInfo, ContainerStatus, ResourceLimits, NetworkConfig, SecurityConfig, BindMount, NetworkMode, PortMapping};
pub use image::{Image, ImageManager, ImageSource};
pub use config::{DistroConfig, Config, PathsConfig, LoggingConfig, FileRotationConfig, Repository, Scripts, InstallOptions};
pub use install::{Installer};
pub use login::{LoginManager, LoginOptions};
pub use bindings::{BindingManager, BindConfig};
pub use session::{SessionManager, Session, SessionStatus};
pub use paths::{ContainerPaths, PathResolver};
pub use constants::constants::Constants;
pub use constants::termux::TermuxConstants;
pub use message::{Message, MessageLevel};
pub use dirfd::DirFd;
pub use guestfile::GuestFile;
pub use sysdata::SysdataManager;
pub use locking::LockManager;
pub use names::NameValidator;
pub use progress::ProgressTracker;
pub use compress::{Compressor, CompressionFormat};
pub use shm::ShmManager;

use anyhow::Result;

/// Main entry point for PRoot Distro Core
pub struct ProotDistroCore {
    container_manager: ContainerManager,
    image_manager: ImageManager,
    config_manager: ConfigManager,
    session_manager: SessionManager,
    lock_manager: LockManager,
    sysdata_manager: SysdataManager,
    shm_manager: ShmManager,
}

impl ProotDistroCore {
    pub fn new() -> Result<Self> {
        let container_manager = ContainerManager::new()
            .map_err(|e| anyhow::anyhow!("ContainerManager::new failed: {}", e))?;
        let image_manager = ImageManager::new()
            .map_err(|e| anyhow::anyhow!("ImageManager::new failed: {}", e))?;
        let config_manager = ConfigManager::new()
            .map_err(|e| anyhow::anyhow!("ConfigManager::new failed: {}", e))?;
        let session_manager = SessionManager::new()
            .map_err(|e| anyhow::anyhow!("SessionManager::new failed: {}", e))?;
        let lock_manager = LockManager::new()
            .map_err(|e| anyhow::anyhow!("LockManager::new failed: {}", e))?;
        let sysdata_manager = SysdataManager::new()
            .map_err(|e| anyhow::anyhow!("SysdataManager::new failed: {}", e))?;
        let shm_manager = ShmManager::new()
            .map_err(|e| anyhow::anyhow!("ShmManager::new failed: {}", e))?;
        
        Ok(Self {
            container_manager,
            image_manager,
            config_manager,
            session_manager,
            lock_manager,
            sysdata_manager,
            shm_manager,
        })
    }

    pub fn install(&mut self, options: InstallOptions) -> Result<()> {
        self.image_manager.download_and_verify_source(&options.source)?;
        self.container_manager.create(&options.name, &options.config)?;
        self.shm_manager.create(&options.name, 1024 * 1024)?; // 1MB default
        Ok(())
    }

    pub fn login(&mut self, options: LoginOptions) -> Result<()> {
        let container = self.container_manager.get(&options.container_name)?;
        let session = self.session_manager.create(&options.container_name, &options)?;
        self.lock_manager.acquire(&options.container_name)?;
        // Execute proot with container config
        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> Result<()> {
        self.lock_manager.acquire(name)?;
        self.container_manager.remove(name)?;
        self.shm_manager.remove(name)?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<ContainerInfo>> {
        self.container_manager.list()
    }

    pub fn backup(&self, name: &str, dest: &std::path::Path) -> Result<()> {
        let container = self.container_manager.get(name)?;
        self.container_manager.backup(&container, dest)
    }

    pub fn restore(&mut self, name: &str, src: &std::path::Path) -> Result<()> {
        self.container_manager.restore(name, src)
    }
}

struct ConfigManager {
    // Configuration management
}

impl ConfigManager {
    fn new() -> Result<Self> {
        Ok(Self {})
    }
}

/// Initialize PRoot Distro Core
pub fn init() -> Result<ProotDistroCore> {
    ProotDistroCore::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init() {
        // Don't set PN_TEST_ROOT - let it use temp_dir() fallback which is writable
        // std::env::set_var("PN_TEST_ROOT", "/tmp/pn-next-test-init");
        let core = init();
        if let Err(ref e) = core {
            eprintln!("Init error: {}", e);
            // Print full error chain
            let mut source = e.source();
            while let Some(err) = source {
                eprintln!("  Caused by: {}", err);
                source = err.source();
            }
        }
        assert!(core.is_ok());
    }
}