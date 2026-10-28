//! Cgroup v2 support for container isolation
//! Setup cgroup v2 mount dan konfigurasi di dalam guest rootfs

use std::fs;
use std::path::Path;
use std::os::unix::fs::PermissionsExt;

/// Setup cgroup v2 inside the guest rootfs.
/// This enables resource control (CPU, memory, IO, PIDs) for containers.
pub fn setup_cgroup_v2(rootfs: &Path) -> Result<(), String> {
    let cgroup_path = rootfs.join("sys/fs/cgroup");

    fs::create_dir_all(&cgroup_path).map_err(|e| {
        format!("Failed to create {:?}: {}", cgroup_path, e)
    })?;

    // Mount cgroup2 filesystem
    mount_cgroup2(&cgroup_path)?;

    // Create default cgroup hierarchy
    create_default_cgroups(&cgroup_path)?;

    Ok(())
}

/// Mount cgroup2 filesystem.
fn mount_cgroup2(target: &Path) -> Result<(), String> {
    use nix::mount::{mount, MsFlags};

    mount(
        Some("cgroup2"),
        target,
        Some("cgroup2"),
        MsFlags::MS_NOEXEC | MsFlags::MS_NOSUID | MsFlags::MS_NODEV,
        None::<&str>,
    )
    .map_err(|e| format!("Failed to mount cgroup2 on {:?}: {}", target, e))?;

    Ok(())
}

/// Create default cgroup hierarchy for container resource control.
fn create_default_cgroups(cgroup_root: &Path) -> Result<(), String> {
    // Create system.slice for system services
    let system_slice = cgroup_root.join("system.slice");
    fs::create_dir_all(&system_slice).map_err(|e| {
        format!("Failed to create {:?}: {}", system_slice, e)
    })?;

    // Create user.slice for user processes
    let user_slice = cgroup_root.join("user.slice");
    fs::create_dir_all(&user_slice).map_err(|e| {
        format!("Failed to create {:?}: {}", user_slice, e)
    })?;

    // Create machine.slice for containers/machines
    let machine_slice = cgroup_root.join("machine.slice");
    fs::create_dir_all(&machine_slice).map_err(|e| {
        format!("Failed to create {:?}: {}", machine_slice, e)
    })?;

    // Set permissions on cgroup directories
    for dir in [system_slice, user_slice, machine_slice] {
        let mut perms = fs::metadata(&dir).map_err(|e| format!("metadata failed: {}", e))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&dir, perms).map_err(|e| format!("chmod failed: {}", e))?;
    }

    Ok(())
}

/// Check if cgroup v2 is available in the rootfs.
pub fn cgroup_v2_available(rootfs: &Path) -> bool {
    rootfs.join("sys/fs/cgroup/cgroup.controllers").exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_cgroup_v2_available_false_when_missing() {
        let dir = tempdir().unwrap();
        assert!(!cgroup_v2_available(dir.path()));
    }

    #[test]
    fn test_setup_cgroup_v2_creates_dirs() {
        let dir = tempdir().unwrap();
        // This will fail on non-Linux or without mount privileges,
        // but we test that directory creation works
        let _ = setup_cgroup_v2(dir.path()); // Best effort
        assert!(dir.path().join("sys/fs/cgroup/system.slice").exists());
        assert!(dir.path().join("sys/fs/cgroup/user.slice").exists());
        assert!(dir.path().join("sys/fs/cgroup/machine.slice").exists());
    }
}