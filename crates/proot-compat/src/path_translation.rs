//! Path translation for symlink chain resolution
//! Resolusi lengkap dari symlink chains seperti:
//! /usr/local/bin/chromium -> /usr/local/bin/browser -> /opt/browser/chrome

use std::fs;
use std::path::{Path, PathBuf};

const MAX_SYMLINK_HOPS: usize = 40;

/// Resolve a symlink chain fully, following all intermediate symlinks.
/// Unlike OS readlink which only follows one hop, this follows the entire chain.
pub fn resolve_symlink_chain(path: &Path) -> Result<PathBuf, String> {
    let mut current = path.to_path_buf();
    let mut visited: Vec<PathBuf> = Vec::new();

    for _ in 0..MAX_SYMLINK_HOPS {
        // Check for circular symlinks
        if visited.contains(&current) {
            return Err(format!("Circular symlink detected at {:?}", current));
        }
        visited.push(current.clone());

        // Check if current path is a symlink
        if let Ok(metadata) = fs::symlink_metadata(&current) {
            if metadata.file_type().is_symlink() {
                match fs::read_link(&current) {
                    Ok(target) => {
                        // Resolve relative to the symlink's parent directory
                        let resolved = if target.is_absolute() {
                            target
                        } else {
                            current.parent().unwrap_or(Path::new("")).join(target)
                        };
                        current = resolved.canonicalize().unwrap_or(resolved);
                        continue;
                    }
                    Err(e) => return Err(format!("readlink failed: {}", e)),
                }
            }
        }

        // Not a symlink, or can't read metadata - return canonicalized path
        return current.canonicalize().map_err(|e| e.to_string());
    }

    Err(format!("Too many symlink hops (max {})", MAX_SYMLINK_HOPS))
}

/// Check if a path is a symlink.
pub fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

/// Translate a path from guest to host perspective
pub fn translate_guest_to_host(
    guest_path: &Path,
    guest_root: &std::path::Path,
    host_root: &std::path::Path,
) -> PathBuf {
    let relative = guest_path.strip_prefix(guest_root).unwrap_or(guest_path);
    host_root.join(relative)
}

/// Translate a path from host to guest perspective
pub fn translate_host_to_guest(
    host_path: &Path,
    host_root: &std::path::Path,
    guest_root: &std::path::Path,
) -> PathBuf {
    let relative = host_path.strip_prefix(host_root).unwrap_or(host_path);
    guest_root.join(relative)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_resolve_symlink_chain() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("target.txt");
        let link1 = dir.path().join("link1.txt");
        let link2 = dir.path().join("link2.txt");

        // Create target file
        std::fs::File::create(&target)
            .unwrap()
            .write_all(b"test")
            .unwrap();

        // Create symlink chain: link2 -> link1 -> target
        std::os::unix::fs::symlink(&target, &link1).unwrap();
        std::os::unix::fs::symlink(&link1, &link2).unwrap();

        let result = resolve_symlink_chain(&link2).unwrap();
        assert_eq!(result, target);
    }

    #[test]
    fn test_is_symlink() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.txt");
        let link = dir.path().join("link.txt");

        std::fs::File::create(&file).unwrap();
        std::os::unix::fs::symlink(&file, &link).unwrap();

        assert!(!is_symlink(&file));
        assert!(is_symlink(&link));
    }
}
