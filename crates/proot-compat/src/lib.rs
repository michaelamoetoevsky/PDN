//! PRoot Compat - Compatibility layer for PRoot on Termux Android 16+
//!
//! This crate provides compatibility layers for running PRoot on Termux Android 16+,
//! addressing common issues like:
//! - Platform personality (android -> linux)
//! - Path translation (symlink chain resolution)
//! - Seccomp filter bypass for Android
//! - Stub filesystem creation (/run/dbus, /tmp, /sys/fs/cgroup, /sys/class, /dev/shm)
//! - Essential filesystem mounting (/proc, /sys, /dev, /run, /tmp)
//! - Cgroup v2 support

pub mod platform_shim;
pub mod path_translation;
pub mod seccomp_bypass;
pub mod stub_fs;
pub mod mount_helper;
pub mod cgroup_v2;

/// Initialize all compatibility layers for PRoot on Termux Android 16+.
/// This should be called before launching any guest process.
pub fn init_compat() -> anyhow::Result<()> {
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