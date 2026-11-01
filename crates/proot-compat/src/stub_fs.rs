//! Stub filesystem creation
//! Membuat stub file/direktori di dalam guest rootfs untuk kompatibilitas Chrome headless
//! dan aplikasi lain yang membutuhkan /run/dbus, /sys/class, dll.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// Create all stub directories and files needed for PRoot guest compatibility.
pub fn create_stub_filesystem(rootfs: &Path) -> Result<(), String> {
    let stub_dirs = ["run/dbus", "tmp", "sys/fs/cgroup", "sys/class", "dev/shm"];

    for dir in &stub_dirs {
        let path = rootfs.join(dir);
        fs::create_dir_all(&path).map_err(|e| format!("Failed to create {:?}: {}", path, e))?;
    }

    // Create /run/dbus/system_bus_socket as empty stub file
    let socket_path = rootfs.join("run/dbus/system_bus_socket");
    fs::File::create(&socket_path)
        .map_err(|e| format!("Failed to create {:?}: {}", socket_path, e))?;

    // Set permissions: /tmp and /dev/shm should be 1777 (world-writable with sticky bit)
    for dir in ["tmp", "dev/shm"] {
        let path = rootfs.join(dir);
        let mut perms = fs::metadata(&path)
            .map_err(|e| format!("metadata failed: {}", e))?
            .permissions();
        perms.set_mode(0o1777);
        fs::set_permissions(&path, perms).map_err(|e| format!("chmod failed: {}", e))?;
    }

    Ok(())
}

/// Check if a stub path exists in the rootfs.
pub fn stub_exists(rootfs: &Path, relative_path: &str) -> bool {
    rootfs.join(relative_path).exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_create_stub_filesystem() {
        let dir = tempdir().unwrap();
        assert!(create_stub_filesystem(dir.path()).is_ok());

        // Verify stubs were created
        assert!(stub_exists(dir.path(), "run/dbus/system_bus_socket"));
        assert!(stub_exists(dir.path(), "tmp"));
        assert!(stub_exists(dir.path(), "sys/fs/cgroup"));
        assert!(stub_exists(dir.path(), "sys/class"));
        assert!(stub_exists(dir.path(), "dev/shm"));
    }
}
