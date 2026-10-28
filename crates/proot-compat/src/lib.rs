//! PRoot Compat - Compatibility layer for PRoot on Termux Android 16+
//!
//! This crate provides compatibility layers for running PRoot on Termux Android 16+,
//! addressing common issues like:
// - Platform personality (android -> linux)
// - Path translation (symlink chain resolution)
// - Seccomp filter bypass for Android
// - Stub filesystem creation (/run/dbus, /tmp, /sys/fs/cgroup, /sys/class, /dev/shm)
// - Essential filesystem mounting (/proc, /sys, /dev, /run, /tmp)
// - Cgroup v2 support
// - CAP_SYS_ADMIN detection and fallback handling

pub mod platform_shim;
pub mod path_translation;
pub mod seccomp_bypass;
pub mod stub_fs;
pub mod mount_helper;
pub mod cgroup_v2;
pub mod cap_sys_admin;

/// Initialize all compatibility layers for PRoot on Termux Android 16+.
/// This should be called before launching any guest process.
pub fn init_compat() -> anyhow::Result<()> {
    // Initialize global CAP_SYS_ADMIN manager with fallback mode by default
    // Use init_global_manager_with_testing(true) for integration tests that need actual mount capability
    crate::cap_sys_admin::init_global_manager(false);
    
    platform_shim::set_linux_personality().map_err(|e| anyhow::anyhow!(e))?;
    seccomp_bypass::configure_seccomp_filter().map_err(|e| anyhow::anyhow!(e))?;
    Ok(())
}

/// Apply all patches to a guest rootfs directory.
pub fn patch_rootfs(rootfs: &std::path::Path) -> anyhow::Result<()> {
    stub_fs::create_stub_filesystem(rootfs).map_err(|e| anyhow::anyhow!(e))?;
    mount_helper::mount_essential_filesystems(rootfs).map_err(|e| anyhow::anyhow!(e))?;
    cgroup_v2::setup_cgroup_v2(rootfs).map_err(|e| anyhow::anyhow!(e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_compat() {
        assert!(init_compat().is_ok());
    }
}